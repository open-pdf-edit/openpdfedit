//! Rendering/text-geometry engine behind an object-safe [`Engine`] trait.
//!
//! PDFium (via `pdfium-render`) is the only implementation for now. Per
//! PLAN.md §5/§6, nothing outside this crate may name `pdfium_render`
//! types directly — that boundary is what makes a future `hayro` backend
//! (pure Rust, still maturing) a drop-in swap instead of a rewrite, and
//! it contains the bus-factor risk of depending on a single-maintainer
//! binding crate.
//!
//! ## Why documents are handles, not borrowed references
//!
//! `pdfium_render::PdfDocument<'a>` borrows from the `Pdfium` binding
//! instance that opened it. Storing document + bindings together in one
//! struct is self-referential, which safe Rust can't express directly.
//! PDFium needs to stay loaded for the whole process lifetime anyway (a
//! desktop PDF editor is never going to unload its render engine mid-run),
//! so [`PdfiumEngine`] leaks its `Pdfium` binding once at construction to
//! get a `'static` reference, then every open document is `'static` too
//! and can live in a plain `HashMap` behind a handle. This trades a few
//! hundred bytes of intentionally-never-freed memory for an API with no
//! lifetime parameters — the right trade for a long-lived singleton.
//!
//! PDFium itself is not thread-safe across arbitrary concurrent calls, and
//! its global init may only run once *per process*, full stop — not just
//! once per thread. Callers should almost never construct a
//! [`PdfiumEngine`] directly; use [`EngineHandle`] instead, which owns the
//! one process-wide instance on a dedicated thread and hides the raw
//! engine entirely. This crate's own tests found the hard way that two
//! independent `PdfiumEngine`s in one process — even on different
//! threads — segfault; see `thread.rs`'s test module for the single
//! shared-instance pattern this forces on every test in this crate.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;

use pdfium_render::prelude::*;
use thiserror::Error;

mod thread;
pub use thread::EngineHandle;

pub type DocHandle = u64;

#[derive(Debug, Error)]
pub enum EngineError {
    #[error("failed to load the PDFium library: {0}")]
    BindingFailed(String),
    #[error("failed to open document: {0}")]
    OpenFailed(String),
    #[error("unknown document handle {0}")]
    UnknownHandle(DocHandle),
    #[error("page index {index} out of range (document has {page_count} pages)")]
    PageOutOfRange { index: u32, page_count: u32 },
    #[error("render failed: {0}")]
    RenderFailed(String),
    #[error("no fillable form field named {0:?}")]
    UnknownFormField(String),
    #[error("form field {0:?} (type {1:?}) cannot be filled with a plain value")]
    FieldNotFillable(String, FormFieldKind),
    #[error("save failed: {0}")]
    SaveFailed(String),
}

/// The PDFium-recognized widget type of an interactive AcroForm field.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FormFieldKind {
    Text,
    Checkbox,
    RadioButton,
    ComboBox,
    ListBox,
    PushButton,
    Signature,
    Unknown,
}

impl FormFieldKind {
    fn from_pdfium(kind: PdfFormFieldType) -> Self {
        match kind {
            PdfFormFieldType::Text => FormFieldKind::Text,
            PdfFormFieldType::Checkbox => FormFieldKind::Checkbox,
            PdfFormFieldType::RadioButton => FormFieldKind::RadioButton,
            PdfFormFieldType::ComboBox => FormFieldKind::ComboBox,
            PdfFormFieldType::ListBox => FormFieldKind::ListBox,
            PdfFormFieldType::PushButton => FormFieldKind::PushButton,
            PdfFormFieldType::Signature => FormFieldKind::Signature,
            PdfFormFieldType::Unknown => FormFieldKind::Unknown,
        }
    }
}

/// One selectable option of a `ComboBox`/`ListBox` field. Read-only in
/// this pass — see [`PdfiumEngine::fill_form_fields`]'s doc for why combo
/// and list box selections can be listed but not set yet.
#[derive(Debug, Clone)]
pub struct FormFieldOption {
    pub label: Option<String>,
    pub is_selected: bool,
}

/// One interactive AcroForm field, flattened out of PDFium's per-page
/// widget-annotation model into a list a form-fill UI can render
/// directly. A `Checkbox` or `RadioButton` field name can appear more
/// than once in the list returned by [`PdfiumEngine::list_form_fields`]
/// — once per widget in its control group, each with its own `value`
/// (that widget's export value) and `is_checked`.
#[derive(Debug, Clone)]
pub struct FormField {
    pub page_index: u32,
    pub name: String,
    pub kind: FormFieldKind,
    /// For `Text`/`ComboBox`/`ListBox`: the field's current value. For
    /// `Checkbox`/`RadioButton`: this specific widget's own export
    /// value — the string [`PdfiumEngine::fill_form_fields`] expects to
    /// select it (checkboxes also accept the simpler `"true"`/`"false"`;
    /// see that method's doc).
    pub value: Option<String>,
    /// `Some(true/false)` for `Checkbox`/`RadioButton` widgets; `None`
    /// for every other kind.
    pub is_checked: Option<bool>,
    pub is_read_only: bool,
    /// Populated for `ComboBox`/`ListBox` fields only.
    pub options: Vec<FormFieldOption>,
}

/// One rendered page tile, as tightly-packed RGBA8 rows.
#[derive(Clone)]
pub struct RenderedTile {
    pub width: u32,
    pub height: u32,
    /// `width * height * 4` bytes, row-major, RGBA8.
    pub rgba: Vec<u8>,
}

/// A page's untransformed size in PDF points (1/72 inch), independent of
/// any render zoom level. The viewer uses this to reserve correctly-
/// proportioned layout space for a page before its tile has loaded, so
/// scrolling never jumps as pixels arrive.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PageSize {
    pub width: f32,
    pub height: f32,
}

/// The bounding box of one glyph on a page, in PDF page-space points
/// (origin bottom-left), for text selection and hit-testing.
#[derive(Debug, Clone, Copy)]
pub struct CharBox {
    pub char_index: u32,
    pub left: f32,
    pub top: f32,
    pub right: f32,
    pub bottom: f32,
}

/// Finds the character in `chars` (as returned by [`Engine::page_char_boxes`])
/// whose center is closest to `(x, y)` — the "which glyph did the user
/// click near" primitive real text selection needs, since a raw
/// pointer-down/up position is essentially never exactly inside a glyph's
/// box (people click between/near letters, not precisely on their ink).
/// Returns `None` only if `chars` is empty.
pub fn nearest_char_index(chars: &[CharBox], x: f32, y: f32) -> Option<u32> {
    chars
        .iter()
        .min_by(|a, b| {
            char_center_distance_sq(a, x, y)
                .partial_cmp(&char_center_distance_sq(b, x, y))
                .unwrap_or(std::cmp::Ordering::Equal)
        })
        .map(|c| c.char_index)
}

fn char_center_distance_sq(c: &CharBox, x: f32, y: f32) -> f32 {
    let cx = (c.left + c.right) / 2.0;
    let cy = (c.top + c.bottom) / 2.0;
    (cx - x).powi(2) + (cy - y).powi(2)
}

/// Groups the characters between `start` and `end` (inclusive
/// `char_index`es, in either order — a drag from bottom-right back to
/// top-left is just `start > end`) into one bounding quad per visual
/// line, as `[x0, y0, x1, y1]` (`y0` the line's bottom, `y1` its top,
/// matching this workspace's `Rect`/quad convention elsewhere). This is
/// the geometry real text selection needs: a click-drag over text should
/// highlight/underline/strike exactly the selected characters, snapped
/// to their real glyph positions from PDFium — not an arbitrary freehand
/// rectangle a user has to eyeball onto the text themselves.
///
/// Lines are detected purely by vertical overlap: a character starts a
/// new line whenever its `[bottom, top]` band doesn't overlap the
/// current line's band at all. `chars` is assumed to be in
/// `page_char_boxes`'s original document/reading order (not resorted),
/// which is what makes a simple "does the band overlap the *current*
/// line" check correct without a full line-clustering pass. This is
/// deliberately the common case, not a general one: rotated text or two
/// columns whose lines happen to share the exact same y-range would
/// confuse it — a scope limit worth documenting rather than silently
/// mishandling.
pub fn char_range_to_line_quads(chars: &[CharBox], start: u32, end: u32) -> Vec<[f32; 4]> {
    let (lo, hi) = if start <= end {
        (start, end)
    } else {
        (end, start)
    };
    let selected = chars
        .iter()
        .filter(|c| c.char_index >= lo && c.char_index <= hi);

    let mut quads: Vec<[f32; 4]> = Vec::new();
    let mut current: Option<(f32, f32, f32, f32)> = None; // (left, top, right, bottom)
    for c in selected {
        match current {
            Some((l, t, r, b)) if c.bottom < t && c.top > b => {
                current = Some((l.min(c.left), t.max(c.top), r.max(c.right), b.min(c.bottom)));
            }
            Some((l, t, r, b)) => {
                quads.push([l, b, r, t]);
                current = Some((c.left, c.top, c.right, c.bottom));
            }
            None => current = Some((c.left, c.top, c.right, c.bottom)),
        }
    }
    if let Some((l, t, r, b)) = current {
        quads.push([l, b, r, t]);
    }
    quads
}

/// Everything the UI needs from a render engine. Object-safe so the
/// desktop app can hold a `Box<dyn Engine>` and swap implementations
/// without touching call sites.
pub trait Engine: Send {
    fn open(&self, path: &Path) -> Result<DocHandle, EngineError>;
    fn close(&self, handle: DocHandle);
    fn page_count(&self, handle: DocHandle) -> Result<u32, EngineError>;
    /// Renders `page_index` at `target_width` pixels wide, preserving the
    /// page's aspect ratio for height.
    fn render_page(
        &self,
        handle: DocHandle,
        page_index: u32,
        target_width: u32,
    ) -> Result<RenderedTile, EngineError>;
    fn page_char_boxes(
        &self,
        handle: DocHandle,
        page_index: u32,
    ) -> Result<Vec<CharBox>, EngineError>;
    /// Sizes of every page in the document, in reading order. One call
    /// instead of one round trip per page — a viewer needs all of them
    /// up front to lay out a virtualized scroll container.
    fn page_sizes(&self, handle: DocHandle) -> Result<Vec<PageSize>, EngineError>;
    /// Like `open`, but from an in-memory buffer rather than a filesystem
    /// path — the entry point a browser extension build needs, since
    /// `wasm32-unknown-unknown` has no filesystem at all.
    fn open_bytes(&self, bytes: Vec<u8>) -> Result<DocHandle, EngineError>;
    /// Like `save_document`, but returns the saved bytes instead of
    /// writing them to a path — same reason as `open_bytes`.
    fn save_to_bytes(&self, handle: DocHandle) -> Result<Vec<u8>, EngineError>;
    /// Lists every interactive AcroForm field in the document. See
    /// [`PdfiumEngine::list_form_fields`] for the exact flattening rules.
    fn list_form_fields(&self, handle: DocHandle) -> Result<Vec<FormField>, EngineError>;
    /// Fills the named fields with the given values. See
    /// [`PdfiumEngine::fill_form_fields`] for exactly which field types
    /// are supported and how values are interpreted; does not save.
    fn fill_form_fields(
        &self,
        handle: DocHandle,
        values: HashMap<String, String>,
    ) -> Result<(), EngineError>;
}

/// PDFium-backed [`Engine`] implementation.
pub struct PdfiumEngine {
    pdfium: &'static Pdfium,
    documents: Mutex<HashMap<DocHandle, PdfDocument<'static>>>,
    next_handle: AtomicU64,
}

impl PdfiumEngine {
    /// Loads PDFium. Tries, in order: an explicit directory (typically the
    /// app bundle's resource dir, or `.vendor/pdfium/lib` in dev builds),
    /// then the system library search path.
    ///
    /// **Construct at most one `PdfiumEngine` per process.** PDFium's
    /// global init (`FPDF_InitLibrary`/`FPDF_DestroyLibrary`) is not safe
    /// to run concurrently across independent bindings — verified the
    /// hard way in this crate's test suite, where each `#[test]` calling
    /// `new_dev()` on its own thread crashed with SIGTRAP until the tests
    /// were switched to share one instance behind a `OnceLock`. The
    /// desktop app follows the same rule: one engine, owned by the
    /// dedicated render thread (see PLAN.md §6 invariant 5).
    pub fn new(lib_dir: Option<&Path>) -> Result<Self, EngineError> {
        // On wasm32 there is no dynamic library loading at all — pdfium-render's
        // wasm32 build instead expects a separate `pdfium.wasm` module to have
        // already been loaded and initialized from JS (via its exported
        // `initialize_pdfium_render()`) before this ever runs; `lib_dir` is
        // meaningless there, and `bind_to_library`/`pdfium_platform_library_name_at_path`
        // don't even exist for this target (see pdfium-render's own cfg-gates).
        #[cfg(target_arch = "wasm32")]
        let bindings = {
            let _ = lib_dir;
            Pdfium::bind_to_system_library()
                .map_err(|e| EngineError::BindingFailed(e.to_string()))?
        };
        #[cfg(not(target_arch = "wasm32"))]
        let bindings = if let Some(dir) = lib_dir {
            Pdfium::bind_to_library(Pdfium::pdfium_platform_library_name_at_path(dir))
                .map_err(|e| EngineError::BindingFailed(e.to_string()))?
        } else {
            Pdfium::bind_to_system_library()
                .map_err(|e| EngineError::BindingFailed(e.to_string()))?
        };

        // See the module doc: PDFium is a process-lifetime singleton, so
        // leaking it once is the intended trade-off, not an oversight.
        let pdfium: &'static Pdfium = Box::leak(Box::new(Pdfium::new(bindings)));

        Ok(Self {
            pdfium,
            documents: Mutex::new(HashMap::new()),
            next_handle: AtomicU64::new(1),
        })
    }

    /// Convenience constructor for dev/test: locates the vendored dylib
    /// fetched by `scripts/fetch-pdfium.sh` relative to the workspace
    /// root, falling back to the system library.
    pub fn new_dev() -> Result<Self, EngineError> {
        let vendor_dir = dev_vendor_lib_dir();
        match vendor_dir {
            Some(dir) if dir.exists() => Self::new(Some(&dir)),
            _ => Self::new(None),
        }
    }
}

fn dev_vendor_lib_dir() -> Option<PathBuf> {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    // crates/openpdfedit-engine -> workspace root
    let workspace_root = manifest_dir.parent()?.parent()?;
    Some(workspace_root.join(if cfg!(windows) {
        ".vendor/pdfium/bin"
    } else {
        ".vendor/pdfium/lib"
    }))
}

impl Engine for PdfiumEngine {
    fn open(&self, path: &Path) -> Result<DocHandle, EngineError> {
        // `load_pdf_from_file` doesn't exist for wasm32 (no filesystem) — see
        // pdfium-render's own cfg-gates. `wasm32-unknown-unknown` callers must
        // use `open_bytes` instead; that's the entire reason `open_bytes`
        // exists alongside this method rather than replacing it.
        #[cfg(target_arch = "wasm32")]
        {
            let _ = path;
            Err(EngineError::OpenFailed(
                "path-based open is not supported on wasm32; use open_bytes".into(),
            ))
        }
        #[cfg(not(target_arch = "wasm32"))]
        {
            let document = self
                .pdfium
                .load_pdf_from_file(path, None)
                .map_err(|e| EngineError::OpenFailed(e.to_string()))?;

            let handle = self.next_handle.fetch_add(1, Ordering::Relaxed);
            self.documents
                .lock()
                .expect("engine document map lock poisoned")
                .insert(handle, document);
            Ok(handle)
        }
    }

    fn close(&self, handle: DocHandle) {
        self.documents
            .lock()
            .expect("engine document map lock poisoned")
            .remove(&handle);
    }

    fn page_count(&self, handle: DocHandle) -> Result<u32, EngineError> {
        let documents = self
            .documents
            .lock()
            .expect("engine document map lock poisoned");
        let document = documents
            .get(&handle)
            .ok_or(EngineError::UnknownHandle(handle))?;
        Ok(document.pages().len() as u32)
    }

    fn render_page(
        &self,
        handle: DocHandle,
        page_index: u32,
        target_width: u32,
    ) -> Result<RenderedTile, EngineError> {
        let documents = self
            .documents
            .lock()
            .expect("engine document map lock poisoned");
        let document = documents
            .get(&handle)
            .ok_or(EngineError::UnknownHandle(handle))?;

        let page_count = document.pages().len() as u32;
        let page =
            document
                .pages()
                .get(page_index as i32)
                .map_err(|_| EngineError::PageOutOfRange {
                    index: page_index,
                    page_count,
                })?;

        let render_config = PdfRenderConfig::new()
            .set_target_width(target_width as i32)
            .set_maximum_height(target_width as i32 * 4);

        let bitmap = page
            .render_with_config(&render_config)
            .map_err(|e| EngineError::RenderFailed(e.to_string()))?;

        let width = bitmap.width() as u32;
        let height = bitmap.height() as u32;
        let rgba = bitmap.as_rgba_bytes();

        Ok(RenderedTile {
            width,
            height,
            rgba,
        })
    }

    fn page_char_boxes(
        &self,
        handle: DocHandle,
        page_index: u32,
    ) -> Result<Vec<CharBox>, EngineError> {
        let documents = self
            .documents
            .lock()
            .expect("engine document map lock poisoned");
        let document = documents
            .get(&handle)
            .ok_or(EngineError::UnknownHandle(handle))?;

        let page_count = document.pages().len() as u32;
        let page =
            document
                .pages()
                .get(page_index as i32)
                .map_err(|_| EngineError::PageOutOfRange {
                    index: page_index,
                    page_count,
                })?;

        let text = page
            .text()
            .map_err(|e| EngineError::RenderFailed(e.to_string()))?;

        let boxes = text
            .chars()
            .iter()
            .enumerate()
            .filter_map(|(i, ch)| {
                let bounds = ch.loose_bounds().ok()?;
                Some(CharBox {
                    char_index: i as u32,
                    left: bounds.left().value,
                    top: bounds.top().value,
                    right: bounds.right().value,
                    bottom: bounds.bottom().value,
                })
            })
            .collect();

        Ok(boxes)
    }

    fn page_sizes(&self, handle: DocHandle) -> Result<Vec<PageSize>, EngineError> {
        let documents = self
            .documents
            .lock()
            .expect("engine document map lock poisoned");
        let document = documents
            .get(&handle)
            .ok_or(EngineError::UnknownHandle(handle))?;

        document
            .pages()
            .iter()
            .map(|page| {
                Ok(PageSize {
                    width: page.width().value,
                    height: page.height().value,
                })
            })
            .collect()
    }

    fn open_bytes(&self, bytes: Vec<u8>) -> Result<DocHandle, EngineError> {
        // `load_pdf_from_byte_vec` takes ownership of the buffer and
        // embeds it in the returned `PdfDocument`, so the buffer is freed
        // when the document is dropped instead of leaked for the process
        // lifetime. Since `self.pdfium` is `&'static Pdfium`, the returned
        // document's lifetime unifies to `'static` and can be stored directly
        // in `self.documents`.
        let document = self
            .pdfium
            .load_pdf_from_byte_vec(bytes, None)
            .map_err(|e| EngineError::OpenFailed(e.to_string()))?;

        let handle = self.next_handle.fetch_add(1, Ordering::Relaxed);
        self.documents
            .lock()
            .expect("engine document map lock poisoned")
            .insert(handle, document);
        Ok(handle)
    }

    fn save_to_bytes(&self, handle: DocHandle) -> Result<Vec<u8>, EngineError> {
        let documents = self
            .documents
            .lock()
            .expect("engine document map lock poisoned");
        let document = documents
            .get(&handle)
            .ok_or(EngineError::UnknownHandle(handle))?;
        let mut buffer = std::io::Cursor::new(Vec::new());
        document
            .save_to_writer(&mut buffer)
            .map_err(|e| EngineError::SaveFailed(e.to_string()))?;
        Ok(buffer.into_inner())
    }

    fn list_form_fields(&self, handle: DocHandle) -> Result<Vec<FormField>, EngineError> {
        PdfiumEngine::list_form_fields(self, handle)
    }

    fn fill_form_fields(
        &self,
        handle: DocHandle,
        values: HashMap<String, String>,
    ) -> Result<(), EngineError> {
        PdfiumEngine::fill_form_fields(self, handle, &values)
    }
}

/// Interactive AcroForm operations. `list_form_fields`/`fill_form_fields`
/// are also exposed on the [`Engine`] trait (Phase 3 of the extension
/// port — see `thread.rs`'s `list_and_fill_form_fields_work_through_the_engine_trait`
/// test): session logic generic over `E: Engine`, which the wasm/
/// extension build needs, has to reach form fill without depending on
/// `EngineHandle` directly, so keeping these off the trait stopped being
/// accurate once that need showed up. These inherent methods stay
/// alongside the trait ones — desktop callers keep calling them directly,
/// unchanged.
///
/// `save_document` alone stays inherent-only. Not because form-filling is
/// somehow exempt from the trait's swap-a-backend promise (it isn't,
/// anymore — see above), but because it's a path-based *write*, and the
/// trait already has a wasm32-safe equivalent for that: `Engine::
/// save_to_bytes` (added alongside `open_bytes`, for the same "wasm32 has
/// no filesystem" reason). A third `save_*` trait method would be a
/// redundant capability, not a missing one.
///
/// Unlike `openpdfedit-doc`'s lopdf-based edits (which mutate a *separate*
/// object graph and reach disk via incremental save),
/// these methods mutate the same `PdfDocument` this engine already has
/// open for rendering, in place — so a render of the same [`DocHandle`]
/// right after a fill reflects it immediately, with no reopen needed on
/// this side. (The Tauri layer still rotates the handle after a fill —
/// see `apps/desktop/src-tauri/src/forms.rs` — purely to reuse the
/// existing tile-cache-and-client-cache invalidation that a fresh handle
/// already gets for free, not because this engine requires it.)
///
/// Rendering already paints filled-in values without any change here:
/// `render_page`'s `PdfRenderConfig::new()` has `do_render_form_data:
/// true` as pdfium-render's own default, which makes PDFium call
/// `FPDF_FFLDraw` using this document's bound form handle whenever one
/// exists (confirmed by reading `pdfium-render`'s `render_config.rs` and
/// `page.rs` — not just assumed).
impl PdfiumEngine {
    /// Lists every interactive AcroForm field in the document, flattened
    /// across all pages in reading order. A `Checkbox`/`RadioButton`
    /// control group (several widgets sharing one field name) appears as
    /// several entries — see [`FormField`]'s doc.
    pub fn list_form_fields(&self, handle: DocHandle) -> Result<Vec<FormField>, EngineError> {
        let documents = self
            .documents
            .lock()
            .expect("engine document map lock poisoned");
        let document = documents
            .get(&handle)
            .ok_or(EngineError::UnknownHandle(handle))?;

        let mut fields = Vec::new();
        for (page_index, page) in document.pages().iter().enumerate() {
            for annotation in page.annotations().iter() {
                if let Some(field) = annotation.as_form_field() {
                    fields.push(form_field_to_dto(page_index as u32, field));
                }
            }
        }
        Ok(fields)
    }

    /// Fills every field named in `values` and leaves every other field
    /// untouched. Does **not** save to disk — call
    /// [`PdfiumEngine::save_document`] afterwards to persist the change
    /// (kept separate so a caller can fill several times, or inspect the
    /// result via `list_form_fields`, before deciding to write).
    ///
    /// Validates every requested name *before* changing anything, so a
    /// bad request (unknown field name, or a field type this method can't
    /// fill) returns an error without leaving the document half-filled.
    ///
    /// Supported today: **Text** (`value` becomes the field's text) and
    /// **Checkbox** (`value` of `"true"`/`"Yes"`/`"on"`/`"1"` checks it,
    /// anything else unchecks it — every widget sharing a checkbox's
    /// field name gets the same state, which is right for the common
    /// single-checkbox case and a deliberate simplification for the rarer
    /// multi-widget checkbox group). Both are verified end-to-end: fill,
    /// save, reopen, and read back the same value through a fresh handle.
    ///
    /// **RadioButton is best-effort, not verified reliable.** The call
    /// itself succeeds (it validates the field name and calls
    /// `pdfium-render`'s `PdfFormRadioButtonField::set_checked()`, which
    /// matches `value` against the target widget's own export value — see
    /// [`FormField::value`] for how to discover those), but confirmed via
    /// a raw structural dump (an independent PDF library, not this
    /// engine's own reader) that `set_checked()` writes `/V` onto the
    /// *widget's own* annotation dict rather than the shared non-terminal
    /// parent field a proper multi-option radio group uses — so the
    /// selection does not reliably read back as selected afterwards
    /// (`FormField::is_checked`/`value` for `RadioButton` reports
    /// whatever PDFium's own — also unreliable, by its own warning in
    /// `pdfium-render`'s doc comments — `is_checked()` says, not a value
    /// this engine invents). Whether this also affects appearance-stream-
    /// bearing, tool-authored PDFs (as opposed to this crate's minimal
    /// hand-built test fixture, which has none) is unverified — there's
    /// no real-world multi-option radio-group PDF in the test corpus yet.
    /// Treat radio-group fill as experimental until validated against one.
    ///
    /// **Not supported: ComboBox and ListBox selection.** `pdfium-render`
    /// 0.9.3's `PdfFormFieldOptions` (the combo/list options collection)
    /// exposes only read access (`is_set()`/`label()`) — there is no
    /// public API in this dependency version to select an option
    /// (PDFium's own `FPDFAnnot_SetOptionSelected`-equivalent isn't
    /// wrapped). A `values` entry naming a ComboBox/ListBox/PushButton/
    /// Signature field returns [`EngineError::FieldNotFillable`].
    pub fn fill_form_fields(
        &self,
        handle: DocHandle,
        values: &HashMap<String, String>,
    ) -> Result<(), EngineError> {
        let documents = self
            .documents
            .lock()
            .expect("engine document map lock poisoned");
        let document = documents
            .get(&handle)
            .ok_or(EngineError::UnknownHandle(handle))?;

        // Pass 1: validate, mutating nothing yet.
        let mut seen: HashSet<String> = HashSet::new();
        for page in document.pages().iter() {
            for annotation in page.annotations().iter() {
                let Some(field) = annotation.as_form_field() else {
                    continue;
                };
                let Some(name) = field.name() else {
                    continue;
                };
                if !values.contains_key(&name) {
                    continue;
                }
                match field {
                    PdfFormField::Text(_)
                    | PdfFormField::Checkbox(_)
                    | PdfFormField::RadioButton(_) => {
                        seen.insert(name);
                    }
                    _ => {
                        return Err(EngineError::FieldNotFillable(
                            name,
                            FormFieldKind::from_pdfium(field.field_type()),
                        ));
                    }
                }
            }
        }
        if let Some(missing) = values.keys().find(|name| !seen.contains(*name)) {
            return Err(EngineError::UnknownFormField(missing.clone()));
        }

        // Pass 2: apply.
        for page in document.pages().iter() {
            for mut annotation in page.annotations().iter() {
                let Some(field) = annotation.as_form_field_mut() else {
                    continue;
                };
                let Some(name) = field.name() else {
                    continue;
                };
                let Some(value) = values.get(&name) else {
                    continue;
                };
                apply_form_field_value(field, value)?;
            }
        }

        Ok(())
    }

    /// Writes the document's current in-memory state (including any
    /// `fill_form_fields` edits) to `path` via PDFium's own save — a full
    /// rewrite, not `openpdfedit-doc`'s incremental-save pipeline, because
    /// form-filling goes through PDFium's own object model rather than
    /// `openpdfedit-doc`'s lopdf one. The two write paths are deliberately
    /// kept separate; see this impl block's module doc.
    pub fn save_document(&self, handle: DocHandle, path: &Path) -> Result<(), EngineError> {
        // `save_to_file` doesn't exist for wasm32 (no filesystem) — see
        // pdfium-render's own cfg-gates. `wasm32-unknown-unknown` callers must
        // use `save_to_bytes` instead.
        #[cfg(target_arch = "wasm32")]
        {
            let _ = (handle, path);
            Err(EngineError::SaveFailed(
                "path-based save_document is not supported on wasm32; use save_to_bytes".into(),
            ))
        }
        #[cfg(not(target_arch = "wasm32"))]
        {
            let documents = self
                .documents
                .lock()
                .expect("engine document map lock poisoned");
            let document = documents
                .get(&handle)
                .ok_or(EngineError::UnknownHandle(handle))?;
            document
                .save_to_file(path)
                .map_err(|e| EngineError::SaveFailed(e.to_string()))
        }
    }
}

fn form_field_to_dto(page_index: u32, field: &PdfFormField) -> FormField {
    let kind = FormFieldKind::from_pdfium(field.field_type());
    let (value, is_checked) = match field {
        PdfFormField::Text(f) => (f.value(), None),
        PdfFormField::Checkbox(f) => {
            let checked = f.is_checked().unwrap_or(false);
            let export = f.appearance_stream().or_else(|| Some("Yes".to_string()));
            (export, Some(checked))
        }
        PdfFormField::RadioButton(f) => {
            // `is_checked`/`group_value` are best-effort for RadioButton
            // — see `fill_form_fields`'s doc for the confirmed
            // reliability gap. Reported as-is (not papered over) so a
            // caller can see PDFium's own answer, unreliable as it is,
            // rather than a value this code invented.
            let checked = f.is_checked().unwrap_or(false);
            (f.appearance_stream(), Some(checked))
        }
        PdfFormField::ComboBox(f) => (f.value(), None),
        PdfFormField::ListBox(f) => (f.value(), None),
        PdfFormField::PushButton(_) | PdfFormField::Signature(_) | PdfFormField::Unknown(_) => {
            (None, None)
        }
    };

    let options = match field {
        PdfFormField::ComboBox(f) => f
            .options()
            .iter()
            .map(|o| FormFieldOption {
                label: o.label().cloned(),
                is_selected: o.is_set(),
            })
            .collect(),
        PdfFormField::ListBox(f) => f
            .options()
            .iter()
            .map(|o| FormFieldOption {
                label: o.label().cloned(),
                is_selected: o.is_set(),
            })
            .collect(),
        _ => Vec::new(),
    };

    FormField {
        page_index,
        name: field.name().unwrap_or_default(),
        kind,
        value,
        is_checked,
        is_read_only: field.is_read_only(),
        options,
    }
}

fn apply_form_field_value(field: &mut PdfFormField, value: &str) -> Result<(), EngineError> {
    match field {
        PdfFormField::Text(f) => f
            .set_value(value)
            .map_err(|e| EngineError::RenderFailed(e.to_string())),
        PdfFormField::Checkbox(f) => {
            let checked = matches!(value, "true" | "Yes" | "on" | "1");
            f.set_checked(checked)
                .map_err(|e| EngineError::RenderFailed(e.to_string()))
        }
        PdfFormField::RadioButton(f) => {
            // `value` is the export value of the *option* to select. Each
            // radio widget in a group shares one field name but carries
            // its own fixed appearance-stream name (its export value);
            // only the widget whose own name matches gets selected here —
            // see this method's doc for what happens when nothing matches.
            if f.appearance_stream().as_deref() == Some(value) {
                f.set_checked()
                    .map_err(|e| EngineError::RenderFailed(e.to_string()))
            } else {
                Ok(())
            }
        }
        _ => Err(EngineError::FieldNotFillable(
            field.name().unwrap_or_default(),
            FormFieldKind::from_pdfium(field.field_type()),
        )),
    }
}

// PDFium's own types are not `Send`/`Sync` by default in every configuration;
// `PdfiumEngine` restricts all access through `Mutex`-guarded handle maps and
// never exposes a raw document reference, so it is sound to assert `Send`
// here. `pdfium-render`'s default `thread_safe` feature (see Cargo.toml)
// backs this with an internal global lock on the underlying C bindings.
unsafe impl Sync for PdfiumEngine {}

// No direct tests of `PdfiumEngine` here — deliberately. This crate's test
// suite runs as one process, and PDFium's global init may only happen
// once per process; a `PdfiumEngine`-level test module here would race
// the `EngineHandle`-level tests in `thread.rs` (which construct their own
// `PdfiumEngine` internally, on the render thread) and segfault. All
// coverage of `open`/`page_count`/`render_page`/`page_char_boxes`/error
// behavior lives in `thread.rs`'s test module instead, exercised through
// the one sanctioned entry point, `EngineHandle`.

// `nearest_char_index`/`char_range_to_line_quads` are pure geometry over
// plain `CharBox` values — no PDFium involved, so (unlike everything
// else in this crate) they're safe to test directly here without racing
// the single-process-PDFium-init constraint above.
#[cfg(test)]
mod tests {
    use super::*;

    /// Two lines of text, `"AB"` then `"CD"` directly below it — real
    /// `page_char_boxes` output looks like this: char_index in reading
    /// order, each line's chars sharing a `[bottom, top]` band that
    /// doesn't overlap the other line's.
    fn two_lines() -> Vec<CharBox> {
        vec![
            CharBox {
                char_index: 0,
                left: 10.0,
                top: 110.0,
                right: 20.0,
                bottom: 100.0,
            }, // 'A'
            CharBox {
                char_index: 1,
                left: 20.0,
                top: 110.0,
                right: 30.0,
                bottom: 100.0,
            }, // 'B'
            CharBox {
                char_index: 2,
                left: 10.0,
                top: 90.0,
                right: 20.0,
                bottom: 80.0,
            }, // 'C'
            CharBox {
                char_index: 3,
                left: 20.0,
                top: 90.0,
                right: 30.0,
                bottom: 80.0,
            }, // 'D'
        ]
    }

    #[test]
    fn nearest_char_index_picks_the_closest_center() {
        let chars = two_lines();
        // Dead center of 'B' (char 1).
        assert_eq!(nearest_char_index(&chars, 25.0, 105.0), Some(1));
        // Dead center of 'C' (char 2).
        assert_eq!(nearest_char_index(&chars, 15.0, 85.0), Some(2));
        // Far off to the upper-left of everything — still resolves to
        // the geometrically nearest glyph ('A'), never panics/None on a
        // non-empty list.
        assert_eq!(nearest_char_index(&chars, -1000.0, 1000.0), Some(0));
    }

    #[test]
    fn nearest_char_index_on_empty_input_returns_none_not_panic() {
        assert_eq!(nearest_char_index(&[], 0.0, 0.0), None);
    }

    #[test]
    fn char_range_to_line_quads_selecting_one_line_produces_one_quad() {
        let chars = two_lines();
        let quads = char_range_to_line_quads(&chars, 0, 1);
        assert_eq!(quads, vec![[10.0, 100.0, 30.0, 110.0]]);
    }

    #[test]
    fn char_range_to_line_quads_spanning_two_lines_produces_two_quads() {
        let chars = two_lines();
        let quads = char_range_to_line_quads(&chars, 0, 3);
        assert_eq!(
            quads,
            vec![[10.0, 100.0, 30.0, 110.0], [10.0, 80.0, 30.0, 90.0]]
        );
    }

    #[test]
    fn char_range_to_line_quads_handles_a_reversed_drag_direction() {
        // Dragging from the end of the selection back to the start (e.g.
        // bottom-right to top-left) must select the same range as the
        // forward drag, not come out empty or reversed.
        let chars = two_lines();
        assert_eq!(
            char_range_to_line_quads(&chars, 3, 0),
            char_range_to_line_quads(&chars, 0, 3)
        );
    }

    #[test]
    fn char_range_to_line_quads_partial_line_selection_is_tight_not_full_line() {
        let chars = two_lines();
        // Only 'A' selected — the quad must not include 'B''s width.
        let quads = char_range_to_line_quads(&chars, 0, 0);
        assert_eq!(quads, vec![[10.0, 100.0, 20.0, 110.0]]);
    }

    #[test]
    fn char_range_to_line_quads_on_empty_chars_returns_no_quads() {
        assert_eq!(char_range_to_line_quads(&[], 0, 5), Vec::<[f32; 4]>::new());
    }
}

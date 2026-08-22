//! Object-level PDF document model, built on `lopdf`.
//!
//! This crate owns the invariant from PLAN.md §6: edits to a document that
//! already carries a digital signature must be appended as incremental
//! updates, never a wholesale rewrite, or the existing signature breaks.
//! `openpdfedit-engine` (PDFium) is the read/render path; this crate is the
//! write path. The two never share a lock — see the architecture note in
//! PLAN.md before adding cross-calls between them.
//!
//! ## Encryption is NOT implemented — blocked on an upstream lopdf bug
//!
//! A `Document::encrypt`/`decrypt` pair was implemented here (following
//! lopdf's own `examples/encrypt.rs`/`decrypt.rs` call pattern) and then
//! **removed** after testing found it produces documents that cannot be
//! reliably read back — including by lopdf's own loader. Root cause,
//! confirmed via multiple independent minimal reproductions built
//! entirely from lopdf's own documented API (not this crate's code):
//! `Document::encrypt` unconditionally writes a cross-reference
//! **stream** trailer regardless of `SaveOptions` (`use_xref_streams:
//! false` is ignored once a document is encrypted), and reloading that
//! output loses most of the object graph — anywhere from 0 to a handful
//! of objects out of several survive, varying with whether `compress()`
//! was called first and which encryption version (V4/AES-128 or
//! V5/AES-256) was used. This matches a confirmed, currently-open
//! upstream report — [lopdf#479, "save_modern() produces corrupt
//! encrypted PDFs (unencrypted ObjStm)"](https://github.com/J-F-Liu/lopdf/issues/479)
//! — though what we hit is broader than that issue's title suggests: it
//! also reproduces through the plain `save()`/`save_to()` path the issue
//! reports as a workaround, not only `save_modern()`.
//!
//! Shipping this would mean a "password protect" feature that silently
//! corrupts the user's document — worse than not having the feature at
//! all, so it isn't here. Real options going forward, in rough order of
//! preference: (1) wait for/contribute a lopdf fix upstream and retest
//! against a newer release; (2) hand-roll the PDF Standard Security
//! Handler directly against RustCrypto primitives (`aes`, `sha2`, both
//! already transitive lopdf dependencies) — a well-specified but
//! genuinely security-sensitive undertaking (ISO 32000-2 Algorithm 2.A/
//! 2.B for AES-256 revision 6) that deserves its own dedicated,
//! carefully-tested pass rather than being rushed in alongside other
//! work; (3) shell out to `qpdf --encrypt` (Apache-2.0, mature,
//! spec-correct) as a subprocess for just this operation. Tracked as a
//! standing TODO; do not re-attempt via bare `lopdf::Document::encrypt`
//! without first confirming upstream has actually fixed the xref-stream
//! behavior — re-verify with the reproduction steps above before trusting
//! any lopdf version bump to have silently fixed this.
//!
//! ## Incremental saves
//!
//! [`Document`] keeps two copies of the object graph: `original` (the
//! state as of the last successful load or save — never mutated in
//! place) and `current` (the working copy every mutation method touches).
//! [`Document::save_incremental`] diffs `current` against `original`,
//! writes only what changed as a proper PDF incremental update (via
//! `lopdf::IncrementalDocument`, appended after the untouched original
//! bytes — never a wholesale rewrite), then folds `current` back into
//! `original` so the *next* save is relative to this one. That's what
//! makes a chain of edits produce a chain of revisions instead of one
//! save silently invalidating the last, and it's why a signature already
//! present in the file survives every edit after it: its bytes are never
//! touched, only appended after.

use std::collections::{HashMap, HashSet};
use std::path::Path;

use lopdf::{Dictionary, Object, ObjectId};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum DocError {
    #[error("failed to load PDF: {0}")]
    Load(#[from] lopdf::Error),
    #[error("document has no pages")]
    NoPages,
    #[error("page index {index} out of range (document has {page_count} pages)")]
    PageOutOfRange { index: u32, page_count: u32 },
    #[error("object {0:?} is not a dictionary")]
    NotADictionary(ObjectId),
    #[error("failed to write incremental update: {0}")]
    Save(#[from] std::io::Error),
    #[error("reorder must be a permutation of every page index (got {given} indices for {page_count} pages)")]
    NotAPermutation { given: usize, page_count: usize },
    #[error("document has no root Pages object")]
    NoRootPages,
    #[error("annotation {0:?} is not on the given page's /Annots array")]
    AnnotationNotOnPage(ObjectId),
}

/// A loaded PDF, held as its object graph rather than a byte stream.
pub struct Document {
    /// State as of the last load/save. Never mutated directly — it's the
    /// diff baseline for the next incremental save.
    original: lopdf::Document,
    original_bytes: Vec<u8>,
    /// The working copy. Every mutation method (annotations today; page
    /// ops, form fields, etc. later) operates on this.
    current: lopdf::Document,
}

impl Document {
    pub fn open(path: impl AsRef<Path>) -> Result<Self, DocError> {
        let bytes = std::fs::read(path.as_ref()).map_err(DocError::Save)?;
        Self::from_bytes(&bytes)
    }

    pub fn from_bytes(bytes: &[u8]) -> Result<Self, DocError> {
        let original = lopdf::Document::load_mem(bytes)?;
        let current = original.clone();
        Ok(Self {
            original,
            original_bytes: bytes.to_vec(),
            current,
        })
    }

    pub fn page_count(&self) -> Result<u32, DocError> {
        let count = self.current.get_pages().len() as u32;
        if count == 0 {
            return Err(DocError::NoPages);
        }
        Ok(count)
    }

    /// True if the document already contains a `/ByteRange` digital
    /// signature dictionary anywhere in its object graph. Callers use this
    /// to decide whether an edit must go through the incremental-save path.
    /// (In this crate, that's not a choice — every edit already goes
    /// through it — but it's the signal the UI uses to warn "this will be
    /// appended after an existing signature" vs. staying silent.)
    pub fn has_signature(&self) -> bool {
        self.current
            .objects
            .values()
            .any(|obj| matches!(obj.as_dict(), Ok(dict) if dict.has(b"ByteRange")))
    }

    /// The `lopdf` object id of the given 0-based page index, resolved
    /// against the current working state.
    pub fn page_object_id(&self, page_index: u32) -> Result<ObjectId, DocError> {
        let pages = self.current.get_pages();
        let page_count = pages.len() as u32;
        pages
            .get(&(page_index + 1)) // lopdf's get_pages() is keyed 1-based
            .copied()
            .ok_or(DocError::PageOutOfRange {
                index: page_index,
                page_count,
            })
    }

    /// Adds a new indirect object to the working document, returning its id.
    pub fn add_object(&mut self, object: Object) -> ObjectId {
        self.current.add_object(object)
    }

    /// Appends `annot_id` to the given page's `/Annots` array — creating
    /// the array (and, if `/Annots` was stored as an indirect reference,
    /// updating that separate array object rather than the page dict) as
    /// needed. This is the one existing-object mutation annotations
    /// require: the page dictionary (or its `/Annots` array) has to be
    /// rewritten to point at the new annotation, which is exactly the
    /// kind of change an incremental update exists to carry safely.
    pub fn append_annotation_ref(
        &mut self,
        page_index: u32,
        annot_id: ObjectId,
    ) -> Result<(), DocError> {
        let page_id = self.page_object_id(page_index)?;
        let page_dict = self.dict_at(page_id)?.clone();

        match page_dict.get(b"Annots") {
            Ok(Object::Reference(annots_id)) => {
                let annots_id = *annots_id;
                let mut annots = self.array_at(annots_id)?.clone();
                annots.push(Object::Reference(annot_id));
                self.current
                    .objects
                    .insert(annots_id, Object::Array(annots));
            }
            Ok(Object::Array(existing)) => {
                let mut annots = existing.clone();
                annots.push(Object::Reference(annot_id));
                let mut updated = page_dict;
                updated.set("Annots", Object::Array(annots));
                self.current
                    .objects
                    .insert(page_id, Object::Dictionary(updated));
            }
            _ => {
                let mut updated = page_dict;
                updated.set("Annots", Object::Array(vec![Object::Reference(annot_id)]));
                self.current
                    .objects
                    .insert(page_id, Object::Dictionary(updated));
            }
        }

        Ok(())
    }

    /// Ensures the page's `/Resources`/`/Font` contains a standard
    /// (non-embedded) font, adding it if absent, and returns the
    /// resource name to reference it by in a content stream (`/Helv 12
    /// Tf`).
    ///
    /// This exists for text replacement's fallback path. Embedded
    /// *subset* fonts only carry the glyphs the document actually used,
    /// so a perfectly ordinary letter can be missing from the one font a
    /// given run uses — a heading set in a display face may contain no
    /// `b` at all. Refusing the edit in that case is a poor answer when
    /// the alternative is simply drawing the replacement in a standard
    /// font. The base-14 fonts (Helvetica and friends) need no embedded
    /// program, so adding one is cheap and always works; the trade-off
    /// is that the replaced run's typeface may not match its neighbours,
    /// which is a visible, explainable difference rather than a hard
    /// failure.
    ///
    /// Idempotent: a page already carrying this exact base font under
    /// some name reuses that name instead of adding a duplicate.
    pub fn ensure_page_font(
        &mut self,
        page_index: u32,
        base_font: &str,
    ) -> Result<String, DocError> {
        let page_id = self.page_object_id(page_index)?;
        let page_dict = self.dict_at(page_id)?.clone();

        // Where do this page's font resources live: a shared indirect
        // /Resources object, an inline dict on the page, or nowhere yet?
        let resources_ref = match page_dict.get(b"Resources") {
            Ok(Object::Reference(id)) => Some(*id),
            _ => None,
        };
        let mut resources = match page_dict.get(b"Resources") {
            Ok(Object::Reference(id)) => self.dict_at(*id)?.clone(),
            Ok(Object::Dictionary(d)) => d.clone(),
            _ => Dictionary::new(),
        };

        let fonts_ref = match resources.get(b"Font") {
            Ok(Object::Reference(id)) => Some(*id),
            _ => None,
        };
        let mut fonts = match resources.get(b"Font") {
            Ok(Object::Reference(id)) => self.dict_at(*id)?.clone(),
            Ok(Object::Dictionary(d)) => d.clone(),
            _ => Dictionary::new(),
        };

        // Reuse an existing entry for the same base font if there is one.
        for (name, value) in fonts.iter() {
            let font_dict = match value {
                Object::Reference(id) => self.dict_at(*id).ok(),
                Object::Dictionary(d) => Some(d),
                _ => None,
            };
            let matches = font_dict
                .and_then(|d| d.get(b"BaseFont").ok())
                .and_then(|o| o.as_name().ok())
                .is_some_and(|n| n == base_font.as_bytes());
            if matches {
                return Ok(String::from_utf8_lossy(name).into_owned());
            }
        }

        // Pick a resource name that can't collide with an existing one.
        let mut resource_name = "OPEHelv".to_string();
        let mut suffix = 0;
        while fonts.has(resource_name.as_bytes()) {
            suffix += 1;
            resource_name = format!("OPEHelv{suffix}");
        }

        let mut font_dict = Dictionary::new();
        font_dict.set("Type", Object::Name(b"Font".to_vec()));
        font_dict.set("Subtype", Object::Name(b"Type1".to_vec()));
        font_dict.set("BaseFont", Object::Name(base_font.as_bytes().to_vec()));
        font_dict.set("Encoding", Object::Name(b"WinAnsiEncoding".to_vec()));
        let font_id = self.add_object(Object::Dictionary(font_dict));
        fonts.set(resource_name.as_str(), Object::Reference(font_id));

        // Write the (possibly new) /Font dict back where it came from,
        // then /Resources likewise — mutating the shared indirect object
        // when there is one, so other pages sharing it stay consistent.
        match fonts_ref {
            Some(id) => {
                self.current.objects.insert(id, Object::Dictionary(fonts));
            }
            None => resources.set("Font", Object::Dictionary(fonts)),
        }
        match resources_ref {
            Some(id) => {
                self.current
                    .objects
                    .insert(id, Object::Dictionary(resources));
            }
            None => {
                let mut updated = page_dict;
                updated.set("Resources", Object::Dictionary(resources));
                self.current
                    .objects
                    .insert(page_id, Object::Dictionary(updated));
            }
        }

        Ok(resource_name)
    }

    /// The inverse of [`Document::append_annotation_ref`]: removes
    /// `annot_id` from the page's `/Annots` array (same direct-array vs.
    /// indirect-reference handling as the append side). Does **not**
    /// delete the annotation's own dictionary or appearance-stream
    /// objects — they become orphaned, unreferenced objects in the file,
    /// the same trade-off `openpdfedit-forms`' font-merging already makes
    /// elsewhere in this codebase (a few hundred bytes of dead weight per
    /// deletion vs. the complexity of a real mark-and-sweep GC over the
    /// object graph, which nothing here has needed before). Errors if
    /// `annot_id` isn't actually in this page's `/Annots` — the caller
    /// (the desktop command layer) always sources `annot_id` from a
    /// freshly-listed [`crate`]-adjacent `AnnotationSummary`, so this
    /// should only fire if the page changed under the caller between
    /// listing and deleting.
    pub fn remove_annotation_ref(
        &mut self,
        page_index: u32,
        annot_id: ObjectId,
    ) -> Result<(), DocError> {
        let page_id = self.page_object_id(page_index)?;
        let page_dict = self.dict_at(page_id)?.clone();

        let (container_id, mut annots) = match page_dict.get(b"Annots") {
            Ok(Object::Reference(annots_id)) => {
                let annots_id = *annots_id;
                (Some(annots_id), self.array_at(annots_id)?.clone())
            }
            Ok(Object::Array(inline)) => (None, inline.clone()),
            _ => (None, Vec::new()),
        };

        let before_len = annots.len();
        annots.retain(|o| !matches!(o, Object::Reference(id) if *id == annot_id));
        if annots.len() == before_len {
            return Err(DocError::AnnotationNotOnPage(annot_id));
        }

        match container_id {
            Some(annots_id) => {
                self.current
                    .objects
                    .insert(annots_id, Object::Array(annots));
            }
            None => {
                let mut updated = page_dict;
                updated.set("Annots", Object::Array(annots));
                self.current
                    .objects
                    .insert(page_id, Object::Dictionary(updated));
            }
        }

        Ok(())
    }

    /// Public counterpart to the private `dict_at` — lets other crates
    /// (e.g. `openpdfedit-annot`) read an object they already have the id
    /// for without reimplementing `lopdf` object resolution themselves.
    pub fn dictionary(&self, id: ObjectId) -> Result<&Dictionary, DocError> {
        self.dict_at(id)
    }

    /// Follows `obj` through one level of indirection if it's a
    /// `Reference`, otherwise hands it back as-is. PDF lets almost any
    /// value be either inline or an indirect reference, so consumers
    /// walking a nested structure (font dict -> `/DescendantFonts` ->
    /// `/W`, say) would otherwise need this two-case match at every hop.
    pub fn resolve<'a>(&'a self, obj: &'a Object) -> &'a Object {
        match obj {
            Object::Reference(id) => self.current.get_object(*id).unwrap_or(obj),
            other => other,
        }
    }

    /// The decompressed bytes of a stream object — the same
    /// filter-decoding [`Document::page_content_bytes`] applies, exposed
    /// for the other streams a caller may need to read (a font's
    /// `/ToUnicode` CMap, for instance). Falls back to the raw bytes if
    /// the stream has no filter or the filter can't be decoded, matching
    /// `page_content_bytes`'s behavior.
    pub fn decoded_stream(&self, id: ObjectId) -> Result<Vec<u8>, DocError> {
        let stream = self
            .current
            .get_object(id)
            .map_err(DocError::Load)?
            .as_stream()
            .map_err(|_| DocError::NotADictionary(id))?;
        Ok(stream
            .decompressed_content()
            .unwrap_or_else(|_| stream.content.clone()))
    }

    /// The page's `/Resources`/`/Font` entries as `(resource name, font
    /// dictionary)` pairs — the resource names being exactly the ones a
    /// content stream's `Tf` operator refers to (`/F4 9 Tf`). Font
    /// entries that are indirect references are resolved; anything that
    /// isn't a dictionary is skipped rather than erroring, since one
    /// malformed font resource shouldn't make an otherwise-readable page
    /// unreadable. Returns an empty list if the page declares no fonts.
    pub fn page_font_resources(
        &self,
        page_index: u32,
    ) -> Result<Vec<(String, Dictionary)>, DocError> {
        let page_id = self.page_object_id(page_index)?;
        let page_dict = self.dict_at(page_id)?;

        let Ok(resources) = page_dict.get(b"Resources") else {
            return Ok(Vec::new());
        };
        let Object::Dictionary(resources) = self.resolve(resources) else {
            return Ok(Vec::new());
        };
        let Ok(fonts) = resources.get(b"Font") else {
            return Ok(Vec::new());
        };
        let Object::Dictionary(fonts) = self.resolve(fonts) else {
            return Ok(Vec::new());
        };

        Ok(fonts
            .iter()
            .filter_map(|(name, value)| match self.resolve(value) {
                Object::Dictionary(d) => {
                    Some((String::from_utf8_lossy(name).into_owned(), d.clone()))
                }
                _ => None,
            })
            .collect())
    }

    /// Finds the document's `/Root`/`/AcroForm` dictionary, creating an
    /// empty one (and materializing an inline `/AcroForm` dict as its own
    /// indirect object, if that's how it was already stored) if missing,
    /// and returns its object id. Shared by
    /// [`Document::ensure_acroform_and_append_field`] and
    /// [`Document::merge_acroform_entries`].
    fn find_or_create_acroform(&mut self) -> Result<ObjectId, DocError> {
        let root_id = self
            .current
            .trailer
            .get(b"Root")
            .ok()
            .and_then(|o| o.as_reference().ok())
            .ok_or(DocError::NoRootPages)?;
        let catalog_dict = self.dict_at(root_id)?.clone();

        let acroform_id = match catalog_dict.get(b"AcroForm") {
            Ok(Object::Reference(id)) => *id,
            Ok(Object::Dictionary(d)) => {
                let id = self.current.add_object(Object::Dictionary(d.clone()));
                let mut updated_catalog = catalog_dict;
                updated_catalog.set("AcroForm", Object::Reference(id));
                self.current
                    .objects
                    .insert(root_id, Object::Dictionary(updated_catalog));
                id
            }
            _ => {
                let id = self
                    .current
                    .add_object(Object::Dictionary(Dictionary::new()));
                let mut updated_catalog = catalog_dict;
                updated_catalog.set("AcroForm", Object::Reference(id));
                self.current
                    .objects
                    .insert(root_id, Object::Dictionary(updated_catalog));
                id
            }
        };
        Ok(acroform_id)
    }

    /// Adds `base_font` to the document's `/AcroForm`/`/DR`/`/Font`
    /// under the resource name `resource_name`, creating whatever part of
    /// that chain is missing, and returns the AcroForm's object id.
    ///
    /// Deliberately *not* built on [`Document::merge_acroform_entries`],
    /// which overwrites whole top-level keys. Setting `/DR` that way
    /// replaces the entire default-resources dictionary, which has two
    /// consequences: adding a checkbox after a text field would drop the
    /// text field's `/Helv`, and adding any field to a PDF that already
    /// had a real form would discard that form's own fonts and encodings,
    /// so its *existing* fields could stop rendering. This merges into
    /// the nested `/Font` dictionary instead, leaving every other entry
    /// alone, and is idempotent: a resource name that already resolves to
    /// a font is left as-is rather than replaced.
    pub fn ensure_acroform_font(
        &mut self,
        resource_name: &str,
        base_font: &str,
    ) -> Result<ObjectId, DocError> {
        let acroform_id = self.find_or_create_acroform()?;
        let acroform_dict = self.dict_at(acroform_id)?.clone();

        // `/DR` and `/DR`/`/Font` may each be direct or indirect.
        let dr_ref = acroform_dict.get(b"DR").ok().and_then(|o| match o {
            Object::Reference(id) => Some(*id),
            _ => None,
        });
        let mut dr = match acroform_dict.get(b"DR") {
            Ok(Object::Dictionary(d)) => d.clone(),
            Ok(Object::Reference(id)) => self.dict_at(*id)?.clone(),
            _ => Dictionary::new(),
        };
        let font_ref = dr.get(b"Font").ok().and_then(|o| match o {
            Object::Reference(id) => Some(*id),
            _ => None,
        });
        let mut fonts = match dr.get(b"Font") {
            Ok(Object::Dictionary(d)) => d.clone(),
            Ok(Object::Reference(id)) => self.dict_at(*id)?.clone(),
            _ => Dictionary::new(),
        };

        if fonts.has(resource_name.as_bytes()) {
            return Ok(acroform_id);
        }

        let mut font_dict = Dictionary::new();
        font_dict.set("Type", Object::Name(b"Font".to_vec()));
        font_dict.set("Subtype", Object::Name(b"Type1".to_vec()));
        font_dict.set("BaseFont", Object::Name(base_font.as_bytes().to_vec()));
        let font_id = self.current.add_object(Object::Dictionary(font_dict));
        fonts.set(resource_name, Object::Reference(font_id));

        match font_ref {
            Some(id) => {
                self.current.objects.insert(id, Object::Dictionary(fonts));
            }
            None => dr.set("Font", Object::Dictionary(fonts)),
        }
        match dr_ref {
            Some(id) => {
                self.current.objects.insert(id, Object::Dictionary(dr));
            }
            None => {
                let mut updated = acroform_dict;
                updated.set("DR", Object::Dictionary(dr));
                self.current
                    .objects
                    .insert(acroform_id, Object::Dictionary(updated));
            }
        }
        Ok(acroform_id)
    }

    /// Merges `entries` into the document's `/AcroForm` dictionary
    /// (creating it if missing — see [`Document::find_or_create_acroform`]),
    /// overwriting any existing key of the same name. Use
    /// [`Document::ensure_acroform_font`] for `/DR` fonts — this call's
    /// whole-key overwrite is wrong for nested resource dictionaries.
    /// Returns the AcroForm's object id.
    pub fn merge_acroform_entries(&mut self, entries: Dictionary) -> Result<ObjectId, DocError> {
        let acroform_id = self.find_or_create_acroform()?;
        let mut acroform_dict = self.dict_at(acroform_id)?.clone();
        for (key, value) in entries.iter() {
            acroform_dict.set(key.clone(), value.clone());
        }
        self.current
            .objects
            .insert(acroform_id, Object::Dictionary(acroform_dict));
        Ok(acroform_id)
    }

    /// Appends `field_id` to the document's `/AcroForm`/`/Fields` array
    /// (creating the AcroForm dict and/or the array if missing). The
    /// AcroForm-level counterpart to `append_annotation_ref`'s page-level
    /// `/Annots` append — same "the array may already be direct or
    /// indirect, handle either" shape. Built for `openpdfedit-forms`'
    /// field-creation path.
    pub fn ensure_acroform_and_append_field(&mut self, field_id: ObjectId) -> Result<(), DocError> {
        let acroform_id = self.find_or_create_acroform()?;
        let mut acroform_dict = self.dict_at(acroform_id)?.clone();
        match acroform_dict.get(b"Fields") {
            Ok(Object::Reference(fields_id)) => {
                let fields_id = *fields_id;
                let mut fields = self.array_at(fields_id)?.clone();
                fields.push(Object::Reference(field_id));
                self.current
                    .objects
                    .insert(fields_id, Object::Array(fields));
            }
            Ok(Object::Array(existing)) => {
                let mut fields = existing.clone();
                fields.push(Object::Reference(field_id));
                acroform_dict.set("Fields", fields);
                self.current
                    .objects
                    .insert(acroform_id, Object::Dictionary(acroform_dict));
            }
            _ => {
                acroform_dict.set("Fields", vec![Object::Reference(field_id)]);
                self.current
                    .objects
                    .insert(acroform_id, Object::Dictionary(acroform_dict));
            }
        }

        Ok(())
    }

    /// The object ids referenced by the given page's `/Annots` array
    /// (whether stored inline in the page dict or as a separate indirect
    /// array — see `append_annotation_ref`), in document order.
    pub fn page_annotation_refs(&self, page_index: u32) -> Result<Vec<ObjectId>, DocError> {
        let page_id = self.page_object_id(page_index)?;
        let page_dict = self.dict_at(page_id)?;

        let annots = match page_dict.get(b"Annots") {
            Ok(Object::Reference(annots_id)) => self.array_at(*annots_id)?.clone(),
            Ok(Object::Array(inline)) => inline.clone(),
            _ => return Ok(Vec::new()),
        };

        Ok(annots
            .into_iter()
            .filter_map(|o| match o {
                Object::Reference(id) => Some(id),
                _ => None,
            })
            .collect())
    }

    /// The document's root `/Pages` node — the entry every page ultimately
    /// descends from, reached via the trailer's `/Root` (Catalog) `/Pages`.
    fn root_pages_id(&self) -> Result<ObjectId, DocError> {
        let root_id = self
            .current
            .trailer
            .get(b"Root")
            .ok()
            .and_then(|o| o.as_reference().ok())
            .ok_or(DocError::NoRootPages)?;
        let catalog = self.dict_at(root_id)?;
        catalog
            .get(b"Pages")
            .ok()
            .and_then(|o| o.as_reference().ok())
            .ok_or(DocError::NoRootPages)
    }

    /// Sets the given page's `/Rotate` entry (viewer-applied clockwise
    /// rotation, normalized to one of 0/90/180/270 per spec) to
    /// `current_rotation + delta_degrees`.
    pub fn rotate_page(&mut self, page_index: u32, delta_degrees: i32) -> Result<(), DocError> {
        let page_id = self.page_object_id(page_index)?;
        let mut page_dict = self.dict_at(page_id)?.clone();

        let current_rotation = page_dict
            .get(b"Rotate")
            .ok()
            .and_then(|o| o.as_i64().ok())
            .unwrap_or(0);
        let new_rotation = (current_rotation + delta_degrees as i64).rem_euclid(360);

        page_dict.set("Rotate", new_rotation);
        self.current
            .objects
            .insert(page_id, Object::Dictionary(page_dict));
        Ok(())
    }

    /// Sets the given page's `/CropBox` — the visible/printable region,
    /// independent of `/MediaBox` (the full physical page size). `rect`
    /// is `[x0, y0, x1, y1]` in PDF page-space points.
    pub fn set_crop_box(&mut self, page_index: u32, rect: [f32; 4]) -> Result<(), DocError> {
        let page_id = self.page_object_id(page_index)?;
        let mut page_dict = self.dict_at(page_id)?.clone();
        page_dict.set("CropBox", rect.map(Object::from).to_vec());
        self.current
            .objects
            .insert(page_id, Object::Dictionary(page_dict));
        Ok(())
    }

    /// Removes the given page from the document. Walks up the page-tree
    /// parent chain decrementing `/Count` at every ancestor (per spec,
    /// each `Pages` node's `/Count` is the number of leaf `Page`
    /// descendants, not just its immediate `/Kids`), and removes the
    /// page's own id from its immediate parent's `/Kids`. The page
    /// object itself is left as an orphaned, unreachable object — the
    /// incremental-save diff only records the objects that changed
    /// (the ancestors), so no bytes are wasted rewriting content that
    /// simply isn't referenced anymore.
    pub fn delete_page(&mut self, page_index: u32) -> Result<(), DocError> {
        if self.page_count()? <= 1 {
            return Err(DocError::NoPages);
        }
        let page_id = self.page_object_id(page_index)?;
        let page_dict = self.dict_at(page_id)?.clone();
        let mut parent_id = page_dict
            .get(b"Parent")
            .ok()
            .and_then(|o| o.as_reference().ok())
            .ok_or(DocError::NoRootPages)?;

        // Remove from the immediate parent's Kids, decrement its Count.
        let mut parent_dict = self.dict_at(parent_id)?.clone();
        if let Ok(Object::Array(kids)) = parent_dict.get(b"Kids") {
            let new_kids: Vec<Object> = kids
                .iter()
                .filter(|k| !matches!(k, Object::Reference(id) if *id == page_id))
                .cloned()
                .collect();
            parent_dict.set("Kids", new_kids);
        }
        decrement_count(&mut parent_dict);
        self.current
            .objects
            .insert(parent_id, Object::Dictionary(parent_dict));

        // Every ancestor above that only needs Count decremented — its
        // own Kids entry (pointing at the node we just updated) is
        // unchanged, since that intermediate node still exists.
        loop {
            let dict = self.dict_at(parent_id)?.clone();
            let Some(grandparent_id) = dict.get(b"Parent").ok().and_then(|o| o.as_reference().ok())
            else {
                break;
            };
            let mut grandparent_dict = self.dict_at(grandparent_id)?.clone();
            decrement_count(&mut grandparent_dict);
            self.current
                .objects
                .insert(grandparent_id, Object::Dictionary(grandparent_dict));
            parent_id = grandparent_id;
        }

        Ok(())
    }

    /// Reorders every page in the document according to `new_order`, a
    /// permutation of `0..page_count` giving, for each new position, the
    /// **old** 0-based page index that should land there.
    ///
    /// This flattens the page tree to a single level under the root
    /// `/Pages` node — a nested tree (Pages nodes grouping other Pages
    /// nodes, common in PDFs assembled from multiple source documents)
    /// is a spec-legal optimization for readers, not something callers
    /// need preserved, and flattening keeps the reorder logic correct
    /// and simple regardless of the original tree shape. Every moved
    /// page's `/Parent` is updated to point at the (now-flat) root.
    pub fn reorder_pages(&mut self, new_order: &[u32]) -> Result<(), DocError> {
        let pages = self.current.get_pages(); // 1-based page number -> id, in reading order
        let page_count = pages.len();

        let mut seen = vec![false; page_count];
        for &old_index in new_order {
            let idx = old_index as usize;
            if idx >= page_count || seen[idx] {
                return Err(DocError::NotAPermutation {
                    given: new_order.len(),
                    page_count,
                });
            }
            seen[idx] = true;
        }
        if new_order.len() != page_count || seen.iter().any(|&s| !s) {
            return Err(DocError::NotAPermutation {
                given: new_order.len(),
                page_count,
            });
        }

        let ordered_ids: Vec<ObjectId> = new_order
            .iter()
            .map(|&old_index| pages[&(old_index + 1)]) // get_pages() is 1-based
            .collect();

        let root_id = self.root_pages_id()?;
        for &page_id in &ordered_ids {
            let mut page_dict = self.dict_at(page_id)?.clone();
            page_dict.set("Parent", Object::Reference(root_id));
            self.current
                .objects
                .insert(page_id, Object::Dictionary(page_dict));
        }

        let mut root_dict = self.dict_at(root_id)?.clone();
        root_dict.set(
            "Kids",
            ordered_ids
                .into_iter()
                .map(Object::Reference)
                .collect::<Vec<_>>(),
        );
        root_dict.set("Count", page_count as i64);
        self.current
            .objects
            .insert(root_id, Object::Dictionary(root_dict));

        Ok(())
    }

    /// Appends `stream_id` to the given page's `/Contents` — creating an
    /// array if it was a single stream reference, or pushing onto an
    /// existing array — and merges `font_name -> font_id` into the page's
    /// `/Resources`/`/Font` dictionary, creating either as needed.
    ///
    /// `/Contents` may be a single reference *or* an array per spec
    /// (readers concatenate every stream in order), so appending a new
    /// stream is additive: it never touches the bytes of any existing
    /// content stream, only the page dict that points at them — the same
    /// "only the pointer changes, not the pointee" shape
    /// `append_annotation_ref` uses for `/Annots`. `/Resources` (and its
    /// `/Font` sub-dictionary) may themselves be direct or indirect;
    /// resolved either way but always written back *inline* on the page,
    /// which is simpler than preserving whichever indirection the
    /// original happened to use and just as spec-legal.
    ///
    /// Built for `openpdfedit-ocr`'s invisible text-layer append (see
    /// that crate), but generic enough for any future "draw more content
    /// on top without disturbing what's already there" need.
    pub fn append_content_stream(
        &mut self,
        page_index: u32,
        stream_id: ObjectId,
        font_name: &str,
        font_id: ObjectId,
    ) -> Result<(), DocError> {
        let page_id = self.page_object_id(page_index)?;
        let mut page_dict = self.dict_at(page_id)?.clone();

        let new_contents = match page_dict.get(b"Contents") {
            Ok(Object::Reference(existing_id)) => {
                vec![
                    Object::Reference(*existing_id),
                    Object::Reference(stream_id),
                ]
            }
            Ok(Object::Array(existing)) => {
                let mut contents = existing.clone();
                contents.push(Object::Reference(stream_id));
                contents
            }
            _ => vec![Object::Reference(stream_id)],
        };
        page_dict.set("Contents", new_contents);

        let mut resources = match page_dict.get(b"Resources") {
            Ok(Object::Reference(id)) => self.dict_at(*id)?.clone(),
            Ok(Object::Dictionary(d)) => d.clone(),
            _ => Dictionary::new(),
        };
        let mut fonts = match resources.get(b"Font") {
            Ok(Object::Reference(id)) => self.dict_at(*id)?.clone(),
            Ok(Object::Dictionary(d)) => d.clone(),
            _ => Dictionary::new(),
        };
        fonts.set(font_name, Object::Reference(font_id));
        resources.set("Font", Object::Dictionary(fonts));
        page_dict.set("Resources", Object::Dictionary(resources));

        self.current
            .objects
            .insert(page_id, Object::Dictionary(page_dict));
        Ok(())
    }

    /// Returns the given page's content stream bytes, decompressed and
    /// concatenated in order — `/Contents` may be a single stream or an
    /// array of streams per spec, and per spec they're meant to be read
    /// as one continuous token stream once concatenated (an operator is
    /// never split across two content streams, but a stream boundary can
    /// fall anywhere else), so this joins the raw decompressed bytes with
    /// a separating space before handing them back, rather than decoding
    /// each stream into operations separately and concatenating those.
    pub fn page_content_bytes(&self, page_index: u32) -> Result<Vec<u8>, DocError> {
        let page_id = self.page_object_id(page_index)?;
        let page_dict = self.dict_at(page_id)?;

        let stream_ids: Vec<ObjectId> = match page_dict.get(b"Contents") {
            Ok(Object::Reference(id)) => vec![*id],
            Ok(Object::Array(refs)) => refs
                .iter()
                .filter_map(|o| match o {
                    Object::Reference(id) => Some(*id),
                    _ => None,
                })
                .collect(),
            _ => Vec::new(),
        };

        let mut bytes = Vec::new();
        for (i, id) in stream_ids.iter().enumerate() {
            if i > 0 {
                bytes.push(b' ');
            }
            let stream = self
                .current
                .get_object(*id)
                .map_err(DocError::Load)?
                .as_stream()
                .map_err(|_| DocError::NotADictionary(*id))?;
            bytes.extend(
                stream
                    .decompressed_content()
                    .unwrap_or_else(|_| stream.content.clone()),
            );
        }
        Ok(bytes)
    }

    /// Replaces the given page's `/Contents` entirely with one new
    /// (uncompressed) stream holding `content_bytes`, dropping the
    /// reference to whatever content stream(s) it had before — those
    /// objects become orphaned (same "leave it unreferenced, don't
    /// bother rewriting bytes that are simply no longer pointed at" shape
    /// `delete_page` uses for a removed page object), not rewritten in
    /// place, since a page's content really is being replaced wholesale
    /// here (unlike `append_content_stream`'s additive case).
    ///
    /// Built for `openpdfedit-redact`'s "rewrite the page's visible
    /// content with the redacted region's operators removed" need — see
    /// that crate's module doc for why redaction *replaces* rather than
    /// appends.
    pub fn set_page_contents(
        &mut self,
        page_index: u32,
        content_bytes: Vec<u8>,
    ) -> Result<(), DocError> {
        let page_id = self.page_object_id(page_index)?;
        let mut page_dict = self.dict_at(page_id)?.clone();

        let stream_id = self.current.add_object(Object::Stream(lopdf::Stream::new(
            Dictionary::new(),
            content_bytes,
        )));
        page_dict.set("Contents", Object::Reference(stream_id));

        self.current
            .objects
            .insert(page_id, Object::Dictionary(page_dict));
        Ok(())
    }

    /// The page's effective `/MediaBox` — read from the page dictionary
    /// itself or inherited up the `/Parent` chain (MediaBox is an
    /// inheritable page attribute per ISO 32000 §7.7.3.4), as
    /// `[x0, y0, x1, y1]` points. Falls back to US Letter when absent or
    /// malformed anywhere along the chain — callers here are drawing
    /// overlays (watermarks), where a sensible default beats refusing the
    /// whole edit over one broken page node.
    pub fn page_media_box(&self, page_index: u32) -> Result<[f32; 4], DocError> {
        let mut dict_id = self.page_object_id(page_index)?;
        // Parent chains are shallow in practice; bound the walk so a
        // cyclic /Parent in a hostile file can't spin forever.
        for _ in 0..64 {
            let dict = self.dict_at(dict_id)?;
            if let Ok(obj) = dict.get(b"MediaBox") {
                let arr = match self.resolve(obj) {
                    Object::Array(a) => a.clone(),
                    _ => break,
                };
                if arr.len() != 4 {
                    break;
                }
                let mut out = [0f32; 4];
                for (slot, value) in out.iter_mut().zip(arr.iter()) {
                    *slot = match self.resolve(value) {
                        Object::Integer(n) => *n as f32,
                        Object::Real(r) => *r,
                        _ => return Ok([0.0, 0.0, 612.0, 792.0]),
                    };
                }
                return Ok(out);
            }
            match dict.get(b"Parent") {
                Ok(Object::Reference(parent)) => dict_id = *parent,
                _ => break,
            }
        }
        Ok([0.0, 0.0, 612.0, 792.0])
    }

    /// Appends stamp-style operators to a page's content in a
    /// graphics-state-safe way: the page's existing content stream(s) get
    /// a `q` prefix stream and the appended stream starts with `Q`, so
    /// `appended` always executes in the page's initial coordinate system
    /// regardless of any unbalanced state the original content leaves
    /// behind (the classic overlay/stamp shape). `/Contents` becomes
    /// `[q-prefix, …existing, Q+appended]`. Unlike
    /// [`Document::append_content_stream`] this registers no resources —
    /// pair it with [`Document::merge_page_resource`].
    pub fn wrap_and_append_page_content(
        &mut self,
        page_index: u32,
        appended: &[u8],
    ) -> Result<(), DocError> {
        let page_id = self.page_object_id(page_index)?;
        let mut page_dict = self.dict_at(page_id)?.clone();

        let existing: Vec<Object> = match page_dict.get(b"Contents") {
            Ok(Object::Reference(id)) => vec![Object::Reference(*id)],
            Ok(Object::Array(refs)) => refs.clone(),
            _ => Vec::new(),
        };

        let prefix_id = self.current.add_object(Object::Stream(lopdf::Stream::new(
            Dictionary::new(),
            b"q\n".to_vec(),
        )));
        let mut suffix_bytes = b"\nQ\n".to_vec();
        suffix_bytes.extend_from_slice(appended);
        let suffix_id = self.current.add_object(Object::Stream(lopdf::Stream::new(
            Dictionary::new(),
            suffix_bytes,
        )));

        let mut contents = Vec::with_capacity(existing.len() + 2);
        contents.push(Object::Reference(prefix_id));
        contents.extend(existing);
        contents.push(Object::Reference(suffix_id));
        page_dict.set("Contents", contents);

        self.current
            .objects
            .insert(page_id, Object::Dictionary(page_dict));
        Ok(())
    }

    /// Registers `id` under the page's `/Resources /<category>` dict
    /// (`category` e.g. `"ExtGState"` or `"XObject"`), creating either
    /// dict as needed, under `base_name` or a numbered variant when that
    /// name is already taken by a *different* object. Returns the name
    /// actually used (an existing entry already pointing at `id` is
    /// reused as-is). Mirrors [`Document::ensure_page_font`]'s
    /// indirect-vs-inline handling so a `/Resources` object shared
    /// between pages stays consistent for all of them.
    pub fn merge_page_resource(
        &mut self,
        page_index: u32,
        category: &str,
        base_name: &str,
        id: ObjectId,
    ) -> Result<String, DocError> {
        let page_id = self.page_object_id(page_index)?;
        let page_dict = self.dict_at(page_id)?.clone();

        let resources_ref = match page_dict.get(b"Resources") {
            Ok(Object::Reference(id)) => Some(*id),
            _ => None,
        };
        let mut resources = match page_dict.get(b"Resources") {
            Ok(Object::Reference(id)) => self.dict_at(*id)?.clone(),
            Ok(Object::Dictionary(d)) => d.clone(),
            _ => Dictionary::new(),
        };

        let category_ref = match resources.get(category.as_bytes()) {
            Ok(Object::Reference(id)) => Some(*id),
            _ => None,
        };
        let mut entries = match resources.get(category.as_bytes()) {
            Ok(Object::Reference(id)) => self.dict_at(*id)?.clone(),
            Ok(Object::Dictionary(d)) => d.clone(),
            _ => Dictionary::new(),
        };

        let mut name = base_name.to_string();
        let mut suffix = 0;
        loop {
            match entries.get(name.as_bytes()) {
                Ok(Object::Reference(existing)) if *existing == id => return Ok(name),
                Ok(_) => {
                    suffix += 1;
                    name = format!("{base_name}{suffix}");
                }
                Err(_) => break,
            }
        }
        entries.set(name.as_str(), Object::Reference(id));

        match category_ref {
            Some(cid) => {
                self.current
                    .objects
                    .insert(cid, Object::Dictionary(entries));
            }
            None => resources.set(category, Object::Dictionary(entries)),
        }
        match resources_ref {
            Some(rid) => {
                self.current
                    .objects
                    .insert(rid, Object::Dictionary(resources));
            }
            None => {
                let mut updated = page_dict;
                updated.set("Resources", Object::Dictionary(resources));
                self.current
                    .objects
                    .insert(page_id, Object::Dictionary(updated));
            }
        }
        Ok(name)
    }

    fn dict_at(&self, id: ObjectId) -> Result<&Dictionary, DocError> {
        self.current
            .get_object(id)
            .map_err(DocError::Load)?
            .as_dict()
            .map_err(|_| DocError::NotADictionary(id))
    }

    fn array_at(&self, id: ObjectId) -> Result<&Vec<Object>, DocError> {
        self.current
            .get_object(id)
            .map_err(DocError::Load)?
            .as_array()
            .map_err(|_| DocError::NotADictionary(id))
    }

    /// Diffs `current` against `original`, writes the difference as a
    /// PDF incremental update appended after the original bytes (never
    /// rewriting them), and returns the full new file contents. On
    /// success, `original`/`original_bytes` become the new baseline, so a
    /// second call only writes what changed *since this call* — a proper
    /// revision chain, not a repeated full diff against the first load.
    pub fn save_incremental(&mut self) -> Result<Vec<u8>, DocError> {
        let mut incremental = lopdf::IncrementalDocument::create_from(
            self.original_bytes.clone(),
            self.original.clone(),
        );

        for (id, object) in &self.current.objects {
            let changed = match self.original.objects.get(id) {
                Some(original_object) => original_object != object,
                None => true, // a new object the original didn't have
            };
            if changed {
                incremental.new_document.objects.insert(*id, object.clone());
                // `IncrementalDocument::save_to` allocates its own fresh
                // object (an xref stream) during save and picks its id
                // from `new_document.max_id` — but inserting straight
                // into `.objects` (there's no other way to reuse an
                // already-known id) bypasses lopdf's own bookkeeping for
                // that field, which `create_from` otherwise only seeds
                // from `original.max_id`. Without this, save_to's own
                // allocation can pick the same id we just used and
                // silently overwrite our object at that id, corrupting
                // the very edit an incremental save exists to carry —
                // found via `save_incremental_output_reopens_with_the_new_annotation_reachable`.
                incremental.new_document.max_id = incremental.new_document.max_id.max(id.0);
            }
        }

        let mut out = Vec::new();
        incremental.save_to(&mut out)?;

        self.original = self.current.clone();
        self.original_bytes = out.clone();

        Ok(out)
    }
}

/// Decrements a `Pages` node's `/Count` by one, treating a missing/
/// non-numeric value as `0` (so it clamps at `0` rather than going
/// negative on a malformed document instead of panicking).
fn decrement_count(dict: &mut Dictionary) {
    let count = dict
        .get(b"Count")
        .ok()
        .and_then(|o| o.as_i64().ok())
        .unwrap_or(0);
    dict.set("Count", (count - 1).max(0));
}

#[cfg(test)]
mod tests {
    use super::*;
    use lopdf::content::{Content, Operation};
    use lopdf::{dictionary, Stream};

    /// Builds a valid minimal single-page PDF via `lopdf` itself (rather
    /// than a hand-written byte literal, which is fiddly to get
    /// byte-exact — xref offsets must match object positions precisely,
    /// and `lopdf::Document::save`/`compress` already does that correctly).
    fn minimal_pdf_bytes() -> Vec<u8> {
        minimal_pdf_doc().0
    }

    /// Same as `minimal_pdf_bytes`, but also returns the page object id,
    /// which several tests need to reference directly.
    fn minimal_pdf_doc() -> (Vec<u8>, ObjectId) {
        let mut doc = lopdf::Document::with_version("1.5");
        let pages_id = doc.new_object_id();

        let content = Content {
            operations: vec![Operation::new("BT", vec![]), Operation::new("ET", vec![])],
        };
        let content_id = doc.add_object(Stream::new(dictionary! {}, content.encode().unwrap()));

        let page_id = doc.add_object(dictionary! {
            "Type" => "Page",
            "Parent" => pages_id,
            "MediaBox" => vec![0.into(), 0.into(), 612.into(), 792.into()],
            "Contents" => content_id,
            "Resources" => dictionary! {},
        });

        let pages = dictionary! {
            "Type" => "Pages",
            "Kids" => vec![page_id.into()],
            "Count" => 1,
        };
        doc.objects.insert(pages_id, Object::Dictionary(pages));

        let catalog_id = doc.add_object(dictionary! {
            "Type" => "Catalog",
            "Pages" => pages_id,
        });
        doc.trailer.set("Root", catalog_id);
        // Deliberately NOT calling doc.compress() here: it packs objects
        // into object streams behind a compressed xref stream, and
        // lopdf's incremental-update writer does not round-trip that
        // correctly today — appending after a compressed base produced
        // an annotation reference that resolved to the wrong object on
        // reload. A classic (uncompressed) xref table is also the more
        // conservative, broadly-compatible baseline for incremental
        // updates in general. Tracked as a follow-up if/when compressed
        // basis documents need incremental-save support.
        let mut bytes = Vec::new();
        doc.save_to(&mut bytes)
            .expect("in-memory save should succeed");
        (bytes, page_id)
    }

    /// An `n`-page flat document (one root Pages node, all pages direct
    /// children), each page's content stream a lone `Tj` string naming
    /// the page (e.g. "page 0", "page 1", ...) so tests can tell pages
    /// apart after reordering/deleting them. Returns the bytes and the
    /// page object ids in original (0-based) order.
    fn multi_page_pdf_doc(n: usize) -> (Vec<u8>, Vec<ObjectId>) {
        let mut doc = lopdf::Document::with_version("1.5");
        let pages_id = doc.new_object_id();

        let page_ids: Vec<ObjectId> = (0..n)
            .map(|i| {
                let content = Content {
                    operations: vec![
                        Operation::new("BT", vec![]),
                        Operation::new("Tj", vec![Object::string_literal(format!("page {i}"))]),
                        Operation::new("ET", vec![]),
                    ],
                };
                let content_id =
                    doc.add_object(Stream::new(dictionary! {}, content.encode().unwrap()));
                doc.add_object(dictionary! {
                    "Type" => "Page",
                    "Parent" => pages_id,
                    "MediaBox" => vec![0.into(), 0.into(), 612.into(), 792.into()],
                    "Contents" => content_id,
                    "Resources" => dictionary! {},
                })
            })
            .collect();

        doc.objects.insert(
            pages_id,
            Object::Dictionary(dictionary! {
                "Type" => "Pages",
                "Kids" => page_ids.iter().map(|&id| id.into()).collect::<Vec<Object>>(),
                "Count" => n as i64,
            }),
        );
        let catalog_id = doc.add_object(dictionary! { "Type" => "Catalog", "Pages" => pages_id });
        doc.trailer.set("Root", catalog_id);

        let mut bytes = Vec::new();
        doc.save_to(&mut bytes)
            .expect("in-memory save should succeed");
        (bytes, page_ids)
    }

    #[test]
    fn opens_minimal_pdf_and_counts_pages() {
        let bytes = minimal_pdf_bytes();
        let doc = Document::from_bytes(&bytes).expect("minimal PDF should parse");
        assert_eq!(doc.page_count().unwrap(), 1);
    }

    #[test]
    fn unsigned_minimal_pdf_reports_no_signature() {
        let bytes = minimal_pdf_bytes();
        let doc = Document::from_bytes(&bytes).expect("minimal PDF should parse");
        assert!(!doc.has_signature());
    }

    #[test]
    fn signed_document_reports_signature_present() {
        // has_signature() must not just always return false — build a
        // minimal doc with a /ByteRange dict anywhere in the object graph
        // (a real signature dict has more fields, but this is the one
        // has_signature() actually keys on) and confirm it flips to true.
        let mut doc = lopdf::Document::with_version("1.5");
        let pages_id = doc.new_object_id();
        let page_id = doc.add_object(dictionary! {
            "Type" => "Page",
            "Parent" => pages_id,
            "MediaBox" => vec![0.into(), 0.into(), 612.into(), 792.into()],
            "Resources" => dictionary! {},
        });
        let pages = dictionary! {
            "Type" => "Pages",
            "Kids" => vec![page_id.into()],
            "Count" => 1,
        };
        doc.objects.insert(pages_id, Object::Dictionary(pages));
        doc.add_object(dictionary! {
            "Type" => "Sig",
            "ByteRange" => vec![0.into(), 0.into(), 0.into(), 0.into()],
        });
        let catalog_id = doc.add_object(dictionary! {
            "Type" => "Catalog",
            "Pages" => pages_id,
        });
        doc.trailer.set("Root", catalog_id);

        let mut bytes = Vec::new();
        doc.save_to(&mut bytes).unwrap();

        let loaded = Document::from_bytes(&bytes).expect("should parse");
        assert!(loaded.has_signature());
    }

    #[test]
    fn empty_bytes_returns_error_not_panic() {
        let result = Document::from_bytes(&[]);
        assert!(result.is_err());
    }

    #[test]
    fn random_garbage_bytes_returns_error_not_panic() {
        // Not a fuzzer substitute (see fuzz/fuzz_targets/parse_document.rs
        // for that) — just a fixed, deterministic smoke case so this
        // property is checked on every `cargo test`, not just when the
        // fuzz target happens to be run.
        let garbage: Vec<u8> = (0..2048u32)
            .map(|i| i.wrapping_mul(2654435761).to_le_bytes()[0])
            .collect();
        let result = Document::from_bytes(&garbage);
        assert!(result.is_err());
    }

    #[test]
    fn truncated_valid_pdf_returns_error_not_panic() {
        let bytes = minimal_pdf_bytes();
        for cut in [1, bytes.len() / 4, bytes.len() / 2, bytes.len() - 1] {
            // Truncating a valid PDF is a completely realistic real-world
            // case (a crashed download, a half-written file) — it must
            // error cleanly, not panic, at every cut point tried.
            let _ = Document::from_bytes(&bytes[..cut]);
        }
    }

    #[test]
    fn opening_nonexistent_path_returns_error_not_panic() {
        let result = Document::open("/nonexistent/path/that/should/not/exist.pdf");
        assert!(result.is_err());
    }

    #[test]
    fn pages_dict_with_zero_count_reports_no_pages_error() {
        let mut doc = lopdf::Document::with_version("1.5");
        let pages_id = doc.new_object_id();
        let pages = dictionary! {
            "Type" => "Pages",
            "Kids" => Vec::<Object>::new(),
            "Count" => 0,
        };
        doc.objects.insert(pages_id, Object::Dictionary(pages));
        let catalog_id = doc.add_object(dictionary! {
            "Type" => "Catalog",
            "Pages" => pages_id,
        });
        doc.trailer.set("Root", catalog_id);

        let mut bytes = Vec::new();
        doc.save_to(&mut bytes).unwrap();

        let loaded = Document::from_bytes(&bytes).expect("should parse structurally");
        assert!(matches!(loaded.page_count(), Err(DocError::NoPages)));
    }

    #[test]
    fn page_object_id_resolves_zero_based_index() {
        let (bytes, page_id) = minimal_pdf_doc();
        let doc = Document::from_bytes(&bytes).expect("should parse");
        assert_eq!(doc.page_object_id(0).unwrap(), page_id);
    }

    #[test]
    fn page_object_id_out_of_range_returns_error_not_panic() {
        let bytes = minimal_pdf_bytes();
        let doc = Document::from_bytes(&bytes).expect("should parse");
        let result = doc.page_object_id(5);
        assert!(matches!(
            result,
            Err(DocError::PageOutOfRange {
                index: 5,
                page_count: 1
            })
        ));
    }

    #[test]
    fn save_incremental_output_starts_with_the_original_bytes() {
        // The whole point of an incremental save: the original bytes are
        // never touched, only appended to. If this doesn't hold, a
        // pre-existing signature's /ByteRange would no longer cover
        // exactly what it signed.
        let bytes = minimal_pdf_bytes();
        let mut doc = Document::from_bytes(&bytes).expect("should parse");

        let new_id = doc.add_object(Object::Dictionary(dictionary! {
            "Type" => "Annot",
            "Subtype" => "Highlight",
        }));
        doc.append_annotation_ref(0, new_id)
            .expect("append should succeed");

        let saved = doc
            .save_incremental()
            .expect("incremental save should succeed");
        assert!(saved.len() > bytes.len());
        assert_eq!(
            &saved[..bytes.len()],
            &bytes[..],
            "incremental save must not rewrite the original bytes"
        );
    }

    #[test]
    fn save_incremental_output_reopens_with_the_new_annotation_reachable() {
        let bytes = minimal_pdf_bytes();
        let mut doc = Document::from_bytes(&bytes).expect("should parse");

        let annot_id = doc.add_object(Object::Dictionary(dictionary! {
            "Type" => "Annot",
            "Subtype" => "Highlight",
        }));
        doc.append_annotation_ref(0, annot_id)
            .expect("append should succeed");
        let saved = doc
            .save_incremental()
            .expect("incremental save should succeed");

        let reopened = lopdf::Document::load_mem(&saved).expect("saved bytes should reparse");
        let page_id = reopened.get_pages()[&1];
        let page_dict = reopened
            .get_dictionary(page_id)
            .expect("page should be a dict");
        let annots = page_dict
            .get(b"Annots")
            .expect("Annots should exist")
            .as_array()
            .expect("Annots should be an array");
        assert_eq!(annots.len(), 1);
        assert_eq!(annots[0], Object::Reference(annot_id));

        let annot_dict = reopened
            .get_dictionary(annot_id)
            .expect("annotation object should exist");
        assert_eq!(
            annot_dict.get(b"Subtype").unwrap().as_name().unwrap(),
            b"Highlight"
        );
    }

    #[test]
    fn second_incremental_save_only_appends_the_second_change() {
        let bytes = minimal_pdf_bytes();
        let mut doc = Document::from_bytes(&bytes).expect("should parse");

        let first_id = doc.add_object(Object::Dictionary(dictionary! { "Marker" => "first" }));
        doc.append_annotation_ref(0, first_id)
            .expect("append should succeed");
        let after_first_save = doc.save_incremental().expect("first save should succeed");

        let second_id = doc.add_object(Object::Dictionary(dictionary! { "Marker" => "second" }));
        doc.append_annotation_ref(0, second_id)
            .expect("append should succeed");
        let after_second_save = doc.save_incremental().expect("second save should succeed");

        // The second save's output must start with everything the first
        // save produced — a proper revision chain, not a full rewrite
        // (which would also, incidentally, break the first annotation's
        // own hypothetical signature if one had been added over it).
        assert!(after_second_save.len() > after_first_save.len());
        assert_eq!(
            &after_second_save[..after_first_save.len()],
            &after_first_save[..],
            "second incremental save must build on top of the first, not rewrite it"
        );

        let reopened = lopdf::Document::load_mem(&after_second_save).expect("should reparse");
        let page_dict = reopened.get_dictionary(reopened.get_pages()[&1]).unwrap();
        let annots = page_dict.get(b"Annots").unwrap().as_array().unwrap();
        assert_eq!(
            annots.len(),
            2,
            "both annotations from both saves should be present"
        );
    }

    #[test]
    fn remove_annotation_ref_deletes_only_the_targeted_annotation() {
        let bytes = minimal_pdf_bytes();
        let mut doc = Document::from_bytes(&bytes).expect("should parse");

        let keep_id = doc.add_object(Object::Dictionary(dictionary! {
            "Type" => "Annot", "Subtype" => "Highlight",
        }));
        let remove_id = doc.add_object(Object::Dictionary(dictionary! {
            "Type" => "Annot", "Subtype" => "Underline",
        }));
        doc.append_annotation_ref(0, keep_id).unwrap();
        doc.append_annotation_ref(0, remove_id).unwrap();
        assert_eq!(
            doc.page_annotation_refs(0).unwrap(),
            vec![keep_id, remove_id]
        );

        doc.remove_annotation_ref(0, remove_id)
            .expect("remove should succeed");
        assert_eq!(
            doc.page_annotation_refs(0).unwrap(),
            vec![keep_id],
            "only the targeted annotation should be gone; the other must survive"
        );

        let saved = doc.save_incremental().expect("save should succeed");
        let reopened = lopdf::Document::load_mem(&saved).expect("should reparse");
        let page_dict = reopened.get_dictionary(reopened.get_pages()[&1]).unwrap();
        let annots = page_dict.get(b"Annots").unwrap().as_array().unwrap();
        assert_eq!(
            annots,
            &vec![Object::Reference(keep_id)],
            "the deletion must survive a real incremental save + reparse round trip"
        );
    }

    #[test]
    fn remove_annotation_ref_errors_for_an_annotation_not_on_the_page() {
        let bytes = minimal_pdf_bytes();
        let mut doc = Document::from_bytes(&bytes).expect("should parse");

        let real_id = doc.add_object(Object::Dictionary(dictionary! { "Type" => "Annot" }));
        doc.append_annotation_ref(0, real_id).unwrap();

        let bogus_id = (real_id.0 + 999, 0);
        assert!(doc.remove_annotation_ref(0, bogus_id).is_err());
        assert_eq!(
            doc.page_annotation_refs(0).unwrap(),
            vec![real_id],
            "a failed removal must not touch the existing Annots array"
        );
    }

    #[test]
    fn signature_bytes_survive_an_incremental_save_unmodified() {
        // The concrete promise from this crate's module doc: adding an
        // annotation to an already-signed document must not touch a
        // single byte the signature covers.
        let mut doc = lopdf::Document::with_version("1.5");
        let pages_id = doc.new_object_id();
        let page_id = doc.add_object(dictionary! {
            "Type" => "Page",
            "Parent" => pages_id,
            "MediaBox" => vec![0.into(), 0.into(), 612.into(), 792.into()],
            "Resources" => dictionary! {},
        });
        doc.objects.insert(
            pages_id,
            Object::Dictionary(dictionary! {
                "Type" => "Pages",
                "Kids" => vec![page_id.into()],
                "Count" => 1,
            }),
        );
        doc.add_object(dictionary! {
            "Type" => "Sig",
            "ByteRange" => vec![0.into(), 0.into(), 0.into(), 0.into()],
        });
        let catalog_id = doc.add_object(dictionary! {
            "Type" => "Catalog",
            "Pages" => pages_id,
        });
        doc.trailer.set("Root", catalog_id);

        let mut signed_bytes = Vec::new();
        doc.save_to(&mut signed_bytes).unwrap();

        let mut edited = Document::from_bytes(&signed_bytes).expect("should parse");
        assert!(edited.has_signature());

        let annot_id = edited.add_object(Object::Dictionary(dictionary! { "Type" => "Annot" }));
        edited
            .append_annotation_ref(0, annot_id)
            .expect("append should succeed");
        let saved = edited.save_incremental().expect("save should succeed");

        assert_eq!(
            &saved[..signed_bytes.len()],
            &signed_bytes[..],
            "every byte of the originally-signed document must be preserved verbatim"
        );

        let reloaded = Document::from_bytes(&saved).expect("should reparse");
        assert!(
            reloaded.has_signature(),
            "signature must still be present after the edit"
        );
    }

    /// Reads back the literal string operand of a page's lone `Tj` op —
    /// how the multi-page fixture's pages identify themselves, so
    /// reorder/delete tests can confirm not just page *count* but which
    /// page actually ended up where.
    fn page_marker_text(doc: &lopdf::Document, page_id: ObjectId) -> String {
        let page_dict = doc.get_dictionary(page_id).unwrap();
        let content_id = page_dict.get(b"Contents").unwrap().as_reference().unwrap();
        let stream = doc.get_object(content_id).unwrap().as_stream().unwrap();
        let content = Content::decode(&stream.content).unwrap();
        let tj = content
            .operations
            .iter()
            .find(|op| op.operator == "Tj")
            .unwrap();
        String::from_utf8(tj.operands[0].as_str().unwrap().to_vec()).unwrap()
    }

    #[test]
    fn rotate_page_sets_and_accumulates_rotate_entry() {
        let bytes = minimal_pdf_bytes();
        let mut doc = Document::from_bytes(&bytes).expect("should parse");

        doc.rotate_page(0, 90).expect("rotate should succeed");
        let saved = doc.save_incremental().expect("save should succeed");
        let reopened = lopdf::Document::load_mem(&saved).unwrap();
        let page = reopened.get_dictionary(reopened.get_pages()[&1]).unwrap();
        assert_eq!(page.get(b"Rotate").unwrap().as_i64().unwrap(), 90);

        // A second rotation accumulates, and wraps at 360 rather than
        // growing unboundedly.
        doc.rotate_page(0, 300).expect("rotate should succeed");
        let saved2 = doc.save_incremental().expect("save should succeed");
        let reopened2 = lopdf::Document::load_mem(&saved2).unwrap();
        let page2 = reopened2.get_dictionary(reopened2.get_pages()[&1]).unwrap();
        assert_eq!(page2.get(b"Rotate").unwrap().as_i64().unwrap(), 30);
    }

    #[test]
    fn rotate_page_out_of_range_returns_error_not_panic() {
        let bytes = minimal_pdf_bytes();
        let mut doc = Document::from_bytes(&bytes).expect("should parse");
        assert!(doc.rotate_page(5, 90).is_err());
    }

    #[test]
    fn set_crop_box_round_trips() {
        let bytes = minimal_pdf_bytes();
        let mut doc = Document::from_bytes(&bytes).expect("should parse");

        doc.set_crop_box(0, [10.0, 20.0, 500.0, 700.0])
            .expect("crop should succeed");
        let saved = doc.save_incremental().expect("save should succeed");

        let reopened = lopdf::Document::load_mem(&saved).unwrap();
        let page = reopened.get_dictionary(reopened.get_pages()[&1]).unwrap();
        let crop = page.get(b"CropBox").unwrap().as_array().unwrap();
        let values: Vec<f32> = crop.iter().map(|o| o.as_float().unwrap()).collect();
        assert_eq!(values, vec![10.0, 20.0, 500.0, 700.0]);
    }

    #[test]
    fn delete_page_removes_it_and_decrements_count_at_every_ancestor() {
        let (bytes, page_ids) = multi_page_pdf_doc(3);
        let mut doc = Document::from_bytes(&bytes).expect("should parse");

        doc.delete_page(1).expect("delete should succeed"); // remove "page 1"
        let saved = doc.save_incremental().expect("save should succeed");

        let reopened = lopdf::Document::load_mem(&saved).unwrap();
        let pages = reopened.get_pages();
        assert_eq!(pages.len(), 2, "page count must drop by exactly one");

        let remaining: Vec<String> = (1..=2u32)
            .map(|n| page_marker_text(&reopened, pages[&n]))
            .collect();
        assert_eq!(
            remaining,
            vec!["page 0", "page 2"],
            "the correct page must be the one removed"
        );

        // The root Pages node's own /Count must reflect the deletion too
        // (this is the property the ancestor walk-up exists for).
        let root_pages_id = reopened
            .get_dictionary(
                reopened
                    .trailer
                    .get(b"Root")
                    .unwrap()
                    .as_reference()
                    .unwrap(),
            )
            .unwrap()
            .get(b"Pages")
            .unwrap()
            .as_reference()
            .unwrap();
        let root_dict = reopened.get_dictionary(root_pages_id).unwrap();
        assert_eq!(root_dict.get(b"Count").unwrap().as_i64().unwrap(), 2);

        let _ = page_ids; // silence unused warning if the ids themselves aren't asserted on directly
    }

    #[test]
    fn delete_page_out_of_range_returns_error_not_panic() {
        let (bytes, _) = multi_page_pdf_doc(2);
        let mut doc = Document::from_bytes(&bytes).expect("should parse");
        assert!(doc.delete_page(9).is_err());
    }

    #[test]
    fn delete_page_refuses_to_empty_the_document() {
        let bytes = minimal_pdf_bytes();
        let mut doc = Document::from_bytes(&bytes).expect("should parse");
        // A single-page document has nothing left to delete down to —
        // this must be a clean error, not a document with zero pages.
        assert!(matches!(doc.delete_page(0), Err(DocError::NoPages)));
    }

    #[test]
    fn reorder_pages_moves_content_not_just_positions() {
        let (bytes, _) = multi_page_pdf_doc(3);
        let mut doc = Document::from_bytes(&bytes).expect("should parse");

        // Reverse the pages: new position 0 = old page 2, etc.
        doc.reorder_pages(&[2, 0, 1])
            .expect("reorder should succeed");
        let saved = doc.save_incremental().expect("save should succeed");

        let reopened = lopdf::Document::load_mem(&saved).unwrap();
        let pages = reopened.get_pages();
        assert_eq!(pages.len(), 3);
        let order: Vec<String> = (1..=3u32)
            .map(|n| page_marker_text(&reopened, pages[&n]))
            .collect();
        assert_eq!(order, vec!["page 2", "page 0", "page 1"]);
    }

    #[test]
    fn reorder_pages_rejects_non_permutations() {
        let (bytes, _) = multi_page_pdf_doc(3);
        let mut doc = Document::from_bytes(&bytes).expect("should parse");

        // Wrong length.
        assert!(doc.reorder_pages(&[0, 1]).is_err());
        // Duplicate index.
        assert!(doc.reorder_pages(&[0, 0, 1]).is_err());
        // Out-of-range index.
        assert!(doc.reorder_pages(&[0, 1, 9]).is_err());
    }

    #[test]
    fn reorder_pages_updates_every_moved_pages_parent() {
        // Correctness beyond "the Kids array looks right": every page's
        // own /Parent must point at the (now-flat) root, or a reader
        // that trusts /Parent over tree traversal could get confused.
        let (bytes, _) = multi_page_pdf_doc(3);
        let mut doc = Document::from_bytes(&bytes).expect("should parse");
        doc.reorder_pages(&[1, 2, 0])
            .expect("reorder should succeed");
        let saved = doc.save_incremental().expect("save should succeed");

        let reopened = lopdf::Document::load_mem(&saved).unwrap();
        let root_pages_id = reopened
            .get_dictionary(
                reopened
                    .trailer
                    .get(b"Root")
                    .unwrap()
                    .as_reference()
                    .unwrap(),
            )
            .unwrap()
            .get(b"Pages")
            .unwrap()
            .as_reference()
            .unwrap();
        for (_, page_id) in reopened.get_pages() {
            let page_dict = reopened.get_dictionary(page_id).unwrap();
            let parent = page_dict.get(b"Parent").unwrap().as_reference().unwrap();
            assert_eq!(parent, root_pages_id);
        }
    }

    #[test]
    fn append_content_stream_adds_to_a_single_reference_contents() {
        let bytes = minimal_pdf_bytes();
        let mut doc = Document::from_bytes(&bytes).expect("should parse");

        let font_id = doc.add_object(Object::Dictionary(dictionary! {
            "Type" => "Font",
            "Subtype" => "Type1",
            "BaseFont" => "Helvetica",
        }));
        let stream_id = doc.add_object(Object::Stream(Stream::new(
            dictionary! {},
            b"BT /F1 12 Tf (hi) Tj ET".to_vec(),
        )));
        doc.append_content_stream(0, stream_id, "F1", font_id)
            .expect("append should succeed");
        let saved = doc.save_incremental().expect("save should succeed");

        let reopened = lopdf::Document::load_mem(&saved).unwrap();
        let page = reopened.get_dictionary(reopened.get_pages()[&1]).unwrap();
        let contents = page.get(b"Contents").unwrap().as_array().unwrap();
        assert_eq!(
            contents.len(),
            2,
            "original content stream plus the new one"
        );
        assert_eq!(contents[1], Object::Reference(stream_id));

        let fonts = page
            .get(b"Resources")
            .unwrap()
            .as_dict()
            .unwrap()
            .get(b"Font")
            .unwrap()
            .as_dict()
            .unwrap();
        assert_eq!(fonts.get(b"F1").unwrap(), &Object::Reference(font_id));
    }

    #[test]
    fn append_content_stream_pushes_onto_an_existing_contents_array() {
        // Build a fixture whose /Contents is already an array of two
        // streams, to exercise the "push, don't replace" branch.
        let mut doc = lopdf::Document::with_version("1.5");
        let pages_id = doc.new_object_id();
        let content_a = doc.add_object(Stream::new(dictionary! {}, b"BT ET".to_vec()));
        let content_b = doc.add_object(Stream::new(dictionary! {}, b"BT ET".to_vec()));
        let page_id = doc.add_object(dictionary! {
            "Type" => "Page",
            "Parent" => pages_id,
            "MediaBox" => vec![0.into(), 0.into(), 612.into(), 792.into()],
            "Contents" => vec![content_a.into(), content_b.into()],
            "Resources" => dictionary! {},
        });
        doc.objects.insert(
            pages_id,
            Object::Dictionary(dictionary! {
                "Type" => "Pages",
                "Kids" => vec![page_id.into()],
                "Count" => 1,
            }),
        );
        let catalog_id = doc.add_object(dictionary! { "Type" => "Catalog", "Pages" => pages_id });
        doc.trailer.set("Root", catalog_id);
        let mut bytes = Vec::new();
        doc.save_to(&mut bytes).unwrap();

        let mut doc = Document::from_bytes(&bytes).expect("should parse");
        let font_id = doc.add_object(Object::Dictionary(dictionary! { "Type" => "Font" }));
        let stream_id = doc.add_object(Object::Stream(Stream::new(
            dictionary! {},
            b"BT ET".to_vec(),
        )));
        doc.append_content_stream(0, stream_id, "F1", font_id)
            .expect("append should succeed");
        let saved = doc.save_incremental().expect("save should succeed");

        let reopened = lopdf::Document::load_mem(&saved).unwrap();
        let page = reopened.get_dictionary(reopened.get_pages()[&1]).unwrap();
        let contents = page.get(b"Contents").unwrap().as_array().unwrap();
        assert_eq!(
            contents.len(),
            3,
            "two originals plus the new one, none dropped"
        );
        assert_eq!(contents[2], Object::Reference(stream_id));
    }

    #[test]
    fn page_content_bytes_returns_a_single_streams_content() {
        let bytes = minimal_pdf_bytes();
        let doc = Document::from_bytes(&bytes).expect("should parse");
        let content = doc.page_content_bytes(0).expect("should succeed");
        // `minimal_pdf_bytes()` builds its content stream via
        // `Content::encode()`, which emits its own separators (a
        // newline between operators) rather than the literal source
        // text — decode back to operations for a robust comparison
        // instead of asserting on exact bytes.
        let decoded = lopdf::content::Content::decode(&content).expect("should decode");
        let operators: Vec<&str> = decoded
            .operations
            .iter()
            .map(|op| op.operator.as_str())
            .collect();
        assert_eq!(operators, vec!["BT", "ET"]);
    }

    #[test]
    fn page_content_bytes_concatenates_a_contents_array() {
        let mut doc = lopdf::Document::with_version("1.5");
        let pages_id = doc.new_object_id();
        let content_a = doc.add_object(Stream::new(dictionary! {}, b"BT".to_vec()));
        let content_b = doc.add_object(Stream::new(dictionary! {}, b"ET".to_vec()));
        let page_id = doc.add_object(dictionary! {
            "Type" => "Page",
            "Parent" => pages_id,
            "MediaBox" => vec![0.into(), 0.into(), 612.into(), 792.into()],
            "Contents" => vec![content_a.into(), content_b.into()],
            "Resources" => dictionary! {},
        });
        doc.objects.insert(
            pages_id,
            Object::Dictionary(dictionary! {
                "Type" => "Pages",
                "Kids" => vec![page_id.into()],
                "Count" => 1,
            }),
        );
        let catalog_id = doc.add_object(dictionary! { "Type" => "Catalog", "Pages" => pages_id });
        doc.trailer.set("Root", catalog_id);
        let mut bytes = Vec::new();
        doc.save_to(&mut bytes).unwrap();

        let doc = Document::from_bytes(&bytes).expect("should parse");
        let content = doc.page_content_bytes(0).expect("should succeed");
        assert_eq!(
            content,
            b"BT ET".to_vec(),
            "streams joined with a separating space"
        );
    }

    #[test]
    fn set_page_contents_replaces_and_orphans_the_old_stream() {
        let bytes = minimal_pdf_bytes();
        let mut doc = Document::from_bytes(&bytes).expect("should parse");

        doc.set_page_contents(0, b"BT (new) Tj ET".to_vec())
            .expect("should succeed");
        let saved = doc.save_incremental().expect("save should succeed");

        let reopened = lopdf::Document::load_mem(&saved).unwrap();
        let page = reopened.get_dictionary(reopened.get_pages()[&1]).unwrap();
        let content_id = page.get(b"Contents").unwrap().as_reference().unwrap();
        let stream = reopened
            .get_object(content_id)
            .unwrap()
            .as_stream()
            .unwrap();
        assert_eq!(stream.content, b"BT (new) Tj ET".to_vec());

        let reloaded = Document::from_bytes(&saved).expect("should reparse");
        assert_eq!(
            reloaded.page_content_bytes(0).unwrap(),
            b"BT (new) Tj ET".to_vec()
        );
    }

    #[test]
    fn merge_acroform_entries_creates_acroform_and_sets_keys() {
        let bytes = minimal_pdf_bytes();
        let mut doc = Document::from_bytes(&bytes).expect("should parse");

        let font_id = doc.add_object(Object::Dictionary(dictionary! {
            "Type" => "Font", "Subtype" => "Type1", "BaseFont" => "Helvetica",
        }));
        doc.merge_acroform_entries(dictionary! {
            "DR" => dictionary! { "Font" => dictionary! { "Helv" => font_id } },
            "DA" => Object::string_literal("/Helv 0 Tf 0 g"),
        })
        .expect("should succeed");
        let saved = doc.save_incremental().expect("save should succeed");

        let reopened = lopdf::Document::load_mem(&saved).unwrap();
        let root_id = reopened
            .trailer
            .get(b"Root")
            .unwrap()
            .as_reference()
            .unwrap();
        let catalog = reopened.get_dictionary(root_id).unwrap();
        let acroform_id = catalog.get(b"AcroForm").unwrap().as_reference().unwrap();
        let acroform = reopened.get_dictionary(acroform_id).unwrap();
        assert_eq!(
            acroform.get(b"DA").unwrap().as_str().unwrap(),
            b"/Helv 0 Tf 0 g"
        );
        let dr = acroform.get(b"DR").unwrap().as_dict().unwrap();
        let fonts = dr.get(b"Font").unwrap().as_dict().unwrap();
        assert_eq!(fonts.get(b"Helv").unwrap(), &Object::Reference(font_id));
    }

    #[test]
    fn merge_acroform_entries_overwrites_an_existing_key_without_dropping_others() {
        let bytes = minimal_pdf_bytes();
        let mut doc = Document::from_bytes(&bytes).expect("should parse");

        doc.merge_acroform_entries(dictionary! { "DA" => Object::string_literal("first") })
            .expect("first merge should succeed");
        doc.merge_acroform_entries(dictionary! { "SigFlags" => 3 })
            .expect("second merge should succeed");
        let saved = doc.save_incremental().expect("save should succeed");

        let reopened = lopdf::Document::load_mem(&saved).unwrap();
        let root_id = reopened
            .trailer
            .get(b"Root")
            .unwrap()
            .as_reference()
            .unwrap();
        let catalog = reopened.get_dictionary(root_id).unwrap();
        let acroform_id = catalog.get(b"AcroForm").unwrap().as_reference().unwrap();
        let acroform = reopened.get_dictionary(acroform_id).unwrap();
        assert_eq!(acroform.get(b"DA").unwrap().as_str().unwrap(), b"first");
        assert_eq!(acroform.get(b"SigFlags").unwrap().as_i64().unwrap(), 3);
    }

    #[test]
    fn ensure_acroform_and_append_field_creates_acroform_when_absent() {
        let bytes = minimal_pdf_bytes();
        let mut doc = Document::from_bytes(&bytes).expect("should parse");

        let field_id = doc.add_object(Object::Dictionary(dictionary! { "Type" => "Annot" }));
        doc.ensure_acroform_and_append_field(field_id)
            .expect("should succeed");
        let saved = doc.save_incremental().expect("save should succeed");

        let reopened = lopdf::Document::load_mem(&saved).unwrap();
        let root_id = reopened
            .trailer
            .get(b"Root")
            .unwrap()
            .as_reference()
            .unwrap();
        let catalog = reopened.get_dictionary(root_id).unwrap();
        let acroform_id = catalog.get(b"AcroForm").unwrap().as_reference().unwrap();
        let acroform = reopened.get_dictionary(acroform_id).unwrap();
        let fields = acroform.get(b"Fields").unwrap().as_array().unwrap();
        assert_eq!(fields.len(), 1);
        assert_eq!(fields[0], Object::Reference(field_id));
    }

    #[test]
    fn ensure_acroform_and_append_field_appends_to_an_existing_fields_array() {
        let mut raw = lopdf::Document::with_version("1.5");
        let pages_id = raw.new_object_id();
        let page_id = raw.add_object(dictionary! {
            "Type" => "Page",
            "Parent" => pages_id,
            "MediaBox" => vec![0.into(), 0.into(), 612.into(), 792.into()],
            "Resources" => dictionary! {},
        });
        raw.objects.insert(
            pages_id,
            Object::Dictionary(dictionary! {
                "Type" => "Pages",
                "Kids" => vec![page_id.into()],
                "Count" => 1,
            }),
        );
        let existing_field_id = raw.add_object(dictionary! { "Type" => "Annot" });
        let acroform_id =
            raw.add_object(dictionary! { "Fields" => vec![existing_field_id.into()] });
        let catalog_id = raw.add_object(dictionary! {
            "Type" => "Catalog",
            "Pages" => pages_id,
            "AcroForm" => acroform_id,
        });
        raw.trailer.set("Root", catalog_id);
        let mut bytes = Vec::new();
        raw.save_to(&mut bytes).unwrap();

        let mut doc = Document::from_bytes(&bytes).expect("should parse");
        let new_field_id = doc.add_object(Object::Dictionary(dictionary! { "Type" => "Annot" }));
        doc.ensure_acroform_and_append_field(new_field_id)
            .expect("should succeed");
        let saved = doc.save_incremental().expect("save should succeed");

        let reopened = lopdf::Document::load_mem(&saved).unwrap();
        let acroform = reopened.get_dictionary(acroform_id).unwrap();
        let fields = acroform.get(b"Fields").unwrap().as_array().unwrap();
        assert_eq!(fields.len(), 2, "existing field kept, new one appended");
        assert_eq!(fields[0], Object::Reference(existing_field_id));
        assert_eq!(fields[1], Object::Reference(new_field_id));
    }

    // No encrypt/decrypt tests here — the feature was implemented, found
    // broken (a genuine upstream lopdf bug, not a bug in this crate's
    // usage of it), and removed. See the "Encryption is NOT implemented"
    // section of this file's module doc for the full writeup and what a
    // correct fix looks like.

    #[test]
    fn page_media_box_reads_the_pages_own_box() {
        let doc = Document::from_bytes(&minimal_pdf_bytes()).expect("should parse");
        assert_eq!(
            doc.page_media_box(0).expect("should succeed"),
            [0.0, 0.0, 612.0, 792.0]
        );
    }

    #[test]
    fn page_media_box_inherits_from_the_pages_parent() {
        // A page with NO MediaBox of its own; the box lives on the root
        // Pages node, the inheritable-attribute case.
        let mut raw = lopdf::Document::with_version("1.5");
        let pages_id = raw.new_object_id();
        let content_id = raw.add_object(Stream::new(dictionary! {}, b"".to_vec()));
        let page_id = raw.add_object(dictionary! {
            "Type" => "Page",
            "Parent" => pages_id,
            "Contents" => content_id,
        });
        raw.objects.insert(
            pages_id,
            Object::Dictionary(dictionary! {
                "Type" => "Pages",
                "Kids" => vec![page_id.into()],
                "Count" => 1,
                "MediaBox" => vec![0.into(), 0.into(), 595.into(), 842.into()],
            }),
        );
        let catalog_id = raw.add_object(dictionary! { "Type" => "Catalog", "Pages" => pages_id });
        raw.trailer.set("Root", catalog_id);
        let mut bytes = Vec::new();
        raw.save_to(&mut bytes).unwrap();

        let doc = Document::from_bytes(&bytes).expect("should parse");
        assert_eq!(
            doc.page_media_box(0).expect("should succeed"),
            [0.0, 0.0, 595.0, 842.0],
            "MediaBox must be inherited from /Parent when the page has none"
        );
    }

    #[test]
    fn wrap_and_append_page_content_wraps_existing_content_in_q_big_q() {
        let mut doc = Document::from_bytes(&minimal_pdf_bytes()).expect("should parse");
        doc.wrap_and_append_page_content(0, b"1 0 0 RG")
            .expect("should succeed");
        let content = doc.page_content_bytes(0).expect("should read back");
        let text = String::from_utf8_lossy(&content);
        assert!(
            text.trim_start().starts_with('q'),
            "existing content must gain a q prefix, got: {text}"
        );
        assert!(
            text.contains("Q\n1 0 0 RG"),
            "appended ops must run after the balancing Q, got: {text}"
        );

        // And the wrap must survive a real incremental save + reload.
        let saved = doc.save_incremental().expect("save should succeed");
        let reopened = Document::from_bytes(&saved).expect("saved bytes should reopen");
        let round_tripped = reopened.page_content_bytes(0).expect("should read back");
        assert!(String::from_utf8_lossy(&round_tripped).contains("Q\n1 0 0 RG"));
    }

    #[test]
    fn merge_page_resource_registers_reuses_and_dodges_collisions() {
        let mut doc = Document::from_bytes(&minimal_pdf_bytes()).expect("should parse");
        let gs_a = doc.add_object(Object::Dictionary(
            dictionary! { "Type" => "ExtGState", "ca" => 0.5 },
        ));
        let gs_b = doc.add_object(Object::Dictionary(
            dictionary! { "Type" => "ExtGState", "ca" => 0.7 },
        ));

        let name_a = doc
            .merge_page_resource(0, "ExtGState", "OPEWmGs", gs_a)
            .expect("should register");
        assert_eq!(name_a, "OPEWmGs");

        // Same object again: the existing entry is reused, not duplicated.
        let name_a_again = doc
            .merge_page_resource(0, "ExtGState", "OPEWmGs", gs_a)
            .expect("should reuse");
        assert_eq!(name_a_again, "OPEWmGs");

        // A different object under the same base name: numbered variant.
        let name_b = doc
            .merge_page_resource(0, "ExtGState", "OPEWmGs", gs_b)
            .expect("should register under a fresh name");
        assert_eq!(name_b, "OPEWmGs1");

        // The registration must survive a save + reload.
        let saved = doc.save_incremental().expect("save should succeed");
        let reopened = lopdf::Document::load_mem(&saved).unwrap();
        let pages = reopened.get_pages();
        let page_id = pages[&1];
        let page = reopened.get_dictionary(page_id).unwrap();
        let resources = page.get(b"Resources").unwrap().as_dict().unwrap();
        let ext = resources.get(b"ExtGState").unwrap().as_dict().unwrap();
        assert!(ext.has(b"OPEWmGs") && ext.has(b"OPEWmGs1"));
    }
}

/// One entry in a document's outline (what readers call bookmarks).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OutlineItem {
    pub title: String,
    /// The page it jumps to, if its destination could be resolved to one
    /// in this document. `None` for an entry that only expands its
    /// children, or whose destination points somewhere this crate
    /// doesn't follow (another file, a URI, an unresolvable name).
    pub page_index: Option<u32>,
    pub children: Vec<OutlineItem>,
}

/// Depth and size limits for outline walking.
///
/// Not a product opinion about how deep a table of contents may be — a
/// guard against malformed files. `/Next` and `/First` are raw object
/// references, so a corrupted or hostile document can describe a cycle,
/// and a naive walk of one never terminates.
const MAX_OUTLINE_DEPTH: usize = 32;
const MAX_OUTLINE_ITEMS: usize = 20_000;

impl Document {
    /// The document's outline as a tree, in reading order.
    ///
    /// Empty when the document has none, which is the common case —
    /// `/Outlines` is optional and most everyday PDFs (a scan, an
    /// invoice, an export from a word processor) carry none.
    ///
    /// Destinations are resolved as far as a page index, following both
    /// the direct form (`/Dest`) and the action form (`/A` with `/S
    /// /GoTo`), and both kinds of named destination: the PDF 1.1
    /// `/Dests` dictionary and the 1.2+ `/Names /Dests` name tree. An
    /// entry whose destination can't be resolved is still returned, with
    /// `page_index: None` — a bookmark you can see but not follow beats
    /// a table of contents with holes in it.
    pub fn outline(&self) -> Result<Vec<OutlineItem>, DocError> {
        let Ok(catalog) = self.current.catalog() else {
            return Ok(Vec::new());
        };
        let Ok(outlines_id) = catalog.get(b"Outlines").and_then(Object::as_reference) else {
            return Ok(Vec::new());
        };
        let Ok(outlines) = self.dict_at(outlines_id) else {
            return Ok(Vec::new());
        };
        let Ok(first) = outlines.get(b"First").and_then(Object::as_reference) else {
            return Ok(Vec::new());
        };

        // Page id -> index, built once: resolving each destination
        // against `get_pages()` separately would be quadratic in a
        // document with a large table of contents.
        let page_indices: HashMap<ObjectId, u32> = self
            .current
            .get_pages()
            .into_iter()
            .map(|(number, id)| (id, number - 1))
            .collect();

        let mut seen = HashSet::new();
        let mut budget = MAX_OUTLINE_ITEMS;
        Ok(self.walk_outline(first, 0, &mut seen, &mut budget, &page_indices))
    }

    fn walk_outline(
        &self,
        first: ObjectId,
        depth: usize,
        seen: &mut HashSet<ObjectId>,
        budget: &mut usize,
        page_indices: &HashMap<ObjectId, u32>,
    ) -> Vec<OutlineItem> {
        let mut items = Vec::new();
        if depth >= MAX_OUTLINE_DEPTH {
            return items;
        }

        let mut current = Some(first);
        while let Some(id) = current {
            // A repeat means the `/Next` chain loops. Stopping is the
            // only correct response; continuing never terminates.
            if *budget == 0 || !seen.insert(id) {
                break;
            }
            *budget -= 1;

            let Ok(dict) = self.dict_at(id) else { break };

            let children = match dict.get(b"First").and_then(Object::as_reference) {
                Ok(child) => self.walk_outline(child, depth + 1, seen, budget, page_indices),
                Err(_) => Vec::new(),
            };

            items.push(OutlineItem {
                title: dict
                    .get(b"Title")
                    .ok()
                    .map(|title| self.decode_pdf_text(title))
                    .unwrap_or_default(),
                page_index: self.destination_page(dict, page_indices),
                children,
            });

            current = dict.get(b"Next").and_then(Object::as_reference).ok();
        }
        items
    }

    /// Resolves an outline entry's destination to a page index.
    fn destination_page(
        &self,
        item: &Dictionary,
        page_indices: &HashMap<ObjectId, u32>,
    ) -> Option<u32> {
        // `/Dest` is the direct form; `/A` is an action, which for a
        // bookmark is almost always `/GoTo` carrying the same
        // destination under `/D`. A URI or launch action has no page and
        // correctly resolves to None.
        let destination = match item.get(b"Dest") {
            Ok(dest) => dest,
            Err(_) => {
                let action = self.resolve(item.get(b"A").ok()?).as_dict().ok()?;
                let is_goto = matches!(action.get(b"S"), Ok(Object::Name(s)) if s == b"GoTo");
                if !is_goto {
                    return None;
                }
                action.get(b"D").ok()?
            }
        };

        self.destination_array_page(destination, page_indices, 0)
    }

    /// A destination is either an explicit array whose first element is
    /// the target page, or a name standing for one. Resolving a name
    /// yields another destination, hence the bounded recursion — a name
    /// that resolves to itself is otherwise an infinite loop.
    fn destination_array_page(
        &self,
        destination: &Object,
        page_indices: &HashMap<ObjectId, u32>,
        depth: usize,
    ) -> Option<u32> {
        if depth > 4 {
            return None;
        }
        match self.resolve(destination) {
            Object::Array(array) => match array.first()? {
                Object::Reference(page_id) => page_indices.get(page_id).copied(),
                // A remote destination gives a bare page *number*
                // instead of a reference. Meaningless for another file,
                // but valid within this one.
                Object::Integer(page_number) => u32::try_from(*page_number).ok(),
                _ => None,
            },
            Object::Name(name) => {
                let target = self.named_destination(name)?;
                self.destination_array_page(&target, page_indices, depth + 1)
            }
            Object::String(bytes, _) => {
                let target = self.named_destination(bytes)?;
                self.destination_array_page(&target, page_indices, depth + 1)
            }
            _ => None,
        }
    }

    /// Looks a named destination up in both places it can live: the PDF
    /// 1.1 `/Dests` dictionary in the catalog, and the 1.2+ `/Names
    /// /Dests` name tree. Real documents use both, and which one depends
    /// on the age of the tool that produced the file rather than on
    /// anything the reader can predict.
    fn named_destination(&self, name: &[u8]) -> Option<Object> {
        let catalog = self.current.catalog().ok()?;

        if let Ok(dests) = catalog.get(b"Dests") {
            if let Ok(dict) = self.resolve(dests).as_dict() {
                if let Ok(found) = dict.get(name) {
                    return Some(self.unwrap_destination(found));
                }
            }
        }

        let names = self.resolve(catalog.get(b"Names").ok()?).as_dict().ok()?;
        let tree_root = names.get(b"Dests").ok()?;
        self.search_name_tree(tree_root, name, 0)
            .map(|found| self.unwrap_destination(&found))
    }

    /// A named destination may be the array itself, or a dictionary
    /// wrapping it under `/D`.
    fn unwrap_destination(&self, object: &Object) -> Object {
        match self.resolve(object) {
            Object::Dictionary(dict) => dict
                .get(b"D")
                .map(|d| self.resolve(d).clone())
                .unwrap_or_else(|_| Object::Null),
            other => other.clone(),
        }
    }

    /// Walks a PDF name tree looking for `name`.
    ///
    /// The tree's `/Names` arrays are sorted, but this scans them
    /// linearly rather than bisecting: a bookmark lookup happens once
    /// per outline entry when a document is opened, and a linear scan of
    /// a sorted array is not what makes that slow. The `/Limits` check
    /// on each `/Kids` branch is the pruning that actually matters, and
    /// that is done.
    fn search_name_tree(&self, node: &Object, name: &[u8], depth: usize) -> Option<Object> {
        if depth > MAX_OUTLINE_DEPTH {
            return None;
        }
        let dict = self.resolve(node).as_dict().ok()?;

        if let Ok(Object::Array(entries)) = dict.get(b"Names").map(|n| self.resolve(n)) {
            // Flat [key1, value1, key2, value2, ...] pairs.
            for pair in entries.chunks(2) {
                let [key, value] = pair else { continue };
                if let Ok(key_bytes) = self.resolve(key).as_str() {
                    if key_bytes == name {
                        return Some(value.clone());
                    }
                }
            }
        }

        let Ok(Object::Array(kids)) = dict.get(b"Kids").map(|k| self.resolve(k)) else {
            return None;
        };
        for kid in kids {
            let kid_dict = self.resolve(kid).as_dict().ok();
            // `/Limits` is [least, greatest] of the keys below this
            // branch; skipping branches that can't contain the name is
            // the whole point of the tree.
            let in_range = kid_dict
                .and_then(|d| d.get(b"Limits").ok())
                .and_then(|l| self.resolve(l).as_array().ok())
                .map(|limits| match (limits.first(), limits.get(1)) {
                    (Some(low), Some(high)) => {
                        let low = self.resolve(low).as_str().unwrap_or(b"");
                        let high = self.resolve(high).as_str().unwrap_or(&[0xFF]);
                        low <= name && name <= high
                    }
                    _ => true,
                })
                .unwrap_or(true);
            if !in_range {
                continue;
            }
            if let Some(found) = self.search_name_tree(kid, name, depth + 1) {
                return Some(found);
            }
        }
        None
    }

    /// Decodes a PDF text string.
    ///
    /// Two encodings are possible and the bytes say which: a UTF-16BE
    /// byte-order mark, or otherwise PDFDocEncoding, whose printable
    /// range matches Latin-1 closely enough that treating it as such is
    /// what every reader effectively does. Getting this wrong shows a
    /// document's table of contents as `\0T\0i\0t\0l\0e`.
    fn decode_pdf_text(&self, object: &Object) -> String {
        let Ok(bytes) = self.resolve(object).as_str() else {
            return String::new();
        };
        if bytes.starts_with(&[0xFE, 0xFF]) {
            let units: Vec<u16> = bytes[2..]
                .as_chunks::<2>()
                .0
                .iter()
                .map(|pair| u16::from_be_bytes(*pair))
                .collect();
            String::from_utf16_lossy(&units)
        } else {
            bytes.iter().map(|b| *b as char).collect()
        }
    }
}

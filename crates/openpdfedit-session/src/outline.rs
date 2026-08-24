//! The document's outline — what readers call bookmarks.
//!
//! Read-only and document-only: it reads the already-parsed object graph
//! in `docs`, never the engine and never the working copy, so it is
//! portable without any `WorkingStore` plumbing (compare
//! [`crate::search`], which is portable for the mirror-image reason —
//! engine-only).
//!
//! The tree is flattened into a depth-tagged list rather than sent as a
//! nested structure: the front-end renders it as an indented list
//! anyway, a tree component for something that is only ever fully
//! expanded is more machinery than the job needs, and flattening once
//! here beats flattening on every render.

use std::collections::HashMap;
use std::sync::Mutex;

use openpdfedit_doc::OutlineItem;
use openpdfedit_engine::DocHandle;
use serde::Serialize;

use crate::{resolve_doc, OpenDoc, SessionError};

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OutlineEntryDto {
    pub title: String,
    /// `None` for an entry whose destination isn't a page in this
    /// document — a link out to a URI, or a name that doesn't resolve.
    /// Still listed, because a heading you can see but not jump to is
    /// better than a table of contents with holes in it.
    pub page_index: Option<u32>,
    /// Nesting level; 0 for a top-level entry.
    pub depth: usize,
    pub has_children: bool,
}

/// The logic behind the desktop's `document_outline_cmd` and the
/// extension's `WasmSession::documentOutline`.
pub fn document_outline_impl(
    docs: &Mutex<HashMap<DocHandle, OpenDoc>>,
    handle: DocHandle,
) -> Result<Vec<OutlineEntryDto>, SessionError> {
    let docs_guard = docs.lock().expect("docs lock poisoned");
    let tree = resolve_doc(&docs_guard, handle)?.doc.outline()?;
    let mut flat = Vec::new();
    flatten(&tree, 0, &mut flat);
    Ok(flat)
}

fn flatten(items: &[OutlineItem], depth: usize, out: &mut Vec<OutlineEntryDto>) {
    for item in items {
        out.push(OutlineEntryDto {
            // A bookmark with no title is legal and would render as an
            // empty, unclickable row; a placeholder keeps it usable.
            title: if item.title.trim().is_empty() {
                "(untitled)".to_string()
            } else {
                item.title.clone()
            },
            page_index: item.page_index,
            depth,
            has_children: !item.children.is_empty(),
        });
        flatten(&item.children, depth + 1, out);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lopdf::{dictionary, Object, Stream};
    use openpdfedit_doc::Document;

    /// Two top-level bookmarks, the first with one child.
    fn outlined_pdf() -> Vec<u8> {
        let mut doc = lopdf::Document::with_version("1.5");
        let pages_id = doc.new_object_id();
        let page_ids: Vec<_> = (0..3)
            .map(|_| {
                let content_id = doc.add_object(Stream::new(dictionary! {}, b"".to_vec()));
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
                "Count" => 3,
            }),
        );
        let outlines_id = doc.new_object_id();
        let child = doc.add_object(dictionary! {
            "Title" => Object::string_literal("Child"),
            "Parent" => outlines_id,
            "Dest" => vec![page_ids[2].into(), "Fit".into()],
        });
        let second = doc.add_object(dictionary! {
            "Title" => Object::string_literal("Second"),
            "Parent" => outlines_id,
            "Dest" => vec![page_ids[1].into(), "Fit".into()],
        });
        let first = doc.add_object(dictionary! {
            "Title" => Object::string_literal("First"),
            "Parent" => outlines_id,
            "Next" => second,
            "First" => child,
            "Last" => child,
            "Dest" => vec![page_ids[0].into(), "Fit".into()],
        });
        doc.objects.insert(
            outlines_id,
            Object::Dictionary(
                dictionary! { "Type" => "Outlines", "First" => first, "Last" => second },
            ),
        );
        let catalog_id = doc.add_object(dictionary! {
            "Type" => "Catalog",
            "Pages" => pages_id,
            "Outlines" => outlines_id,
        });
        doc.trailer.set("Root", catalog_id);
        let mut bytes = Vec::new();
        doc.save_to(&mut bytes).unwrap();
        bytes
    }

    fn docs_with(bytes: &[u8]) -> (Mutex<HashMap<DocHandle, OpenDoc>>, DocHandle) {
        let path = std::path::PathBuf::from("mem-outline-test.pdf");
        let doc = Document::from_bytes(bytes).expect("fixture should parse");
        let docs: Mutex<HashMap<DocHandle, OpenDoc>> = Mutex::new(HashMap::new());
        docs.lock().unwrap().insert(
            1,
            OpenDoc {
                path: path.clone(),
                original_path: path,
                dirty: false,
                doc,
                encryption: None,
            },
        );
        (docs, 1)
    }

    /// No engine and no filesystem anywhere in this test — the whole
    /// point of where this function lives.
    #[test]
    fn flattens_the_tree_depth_first_with_depth_tags() {
        let (docs, handle) = docs_with(&outlined_pdf());
        let flat = document_outline_impl(&docs, handle).expect("outline should succeed");

        let shape: Vec<(&str, usize, Option<u32>, bool)> = flat
            .iter()
            .map(|e| (e.title.as_str(), e.depth, e.page_index, e.has_children))
            .collect();
        assert_eq!(
            shape,
            [
                ("First", 0, Some(0), true),
                ("Child", 1, Some(2), false),
                ("Second", 0, Some(1), false),
            ],
            "a child must follow its parent, one level deeper"
        );
    }

    #[test]
    fn a_document_without_bookmarks_yields_an_empty_list() {
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
            Object::Dictionary(
                dictionary! { "Type" => "Pages", "Kids" => vec![page_id.into()], "Count" => 1 },
            ),
        );
        let catalog_id = doc.add_object(dictionary! { "Type" => "Catalog", "Pages" => pages_id });
        doc.trailer.set("Root", catalog_id);
        let mut bytes = Vec::new();
        doc.save_to(&mut bytes).unwrap();

        let (docs, handle) = docs_with(&bytes);
        assert!(document_outline_impl(&docs, handle).unwrap().is_empty());
    }

    #[test]
    fn an_unknown_handle_is_an_error_not_a_panic() {
        let (docs, _) = docs_with(&outlined_pdf());
        assert!(document_outline_impl(&docs, 999_999).is_err());
    }
}

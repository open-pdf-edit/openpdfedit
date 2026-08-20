//! Cross-document page operations: merge multiple PDFs into one, or
//! extract a subset of one PDF's pages into a new file.
//!
//! Unlike `openpdfedit-doc`'s single-document page ops (rotate, delete,
//! reorder, crop — see its module docs), these are **full-document
//! rewrites**, not incremental updates: merging combines two entirely
//! separate object-number spaces (they'd collide if just concatenated),
//! and extraction produces a genuinely new file, not an edited copy of
//! the source. Neither operation preserves a source document's
//! signature (a merge or extraction is definitionally a new document);
//! `openpdfedit-doc`'s incremental-save guarantee is specifically for
//! edits that keep a document's *identity*, which these don't.
//!
//! `merge`'s object-renumbering pattern (`renumber_objects_with`,
//! collect-then-rebuild-Catalog/Pages) is adapted from lopdf's own
//! `examples/merge.rs` (MIT-licensed, matching this crate) — see that
//! file in the lopdf repository for the canonical version this is based
//! on; ours drops bookmark/outline handling, out of scope here.

use std::collections::BTreeMap;

use lopdf::{Document, Object, ObjectId};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum PagesError {
    #[error("failed to load PDF: {0}")]
    Load(#[from] lopdf::Error),
    #[error("failed to write PDF: {0}")]
    Save(#[from] std::io::Error),
    #[error("merge requires at least one source document")]
    NoSources,
    #[error("extract requires at least one selected page")]
    NoPagesSelected,
    #[error("page index {index} out of range (document has {page_count} pages)")]
    PageOutOfRange { index: u32, page_count: u32 },
    #[error("document has no root Pages object")]
    NoRootPages,
    #[error("no Catalog object found while merging")]
    NoCatalog,
    #[error("no Pages object found while merging")]
    NoPages,
}

/// Merges the given source PDFs (each a full file's bytes, in the order
/// they should appear) into one new document containing every page from
/// every source, in order. Bookmarks/outlines are not carried over
/// (each source's `/Outlines` is dropped) — out of scope for this pass.
pub fn merge(sources: &[&[u8]]) -> Result<Vec<u8>, PagesError> {
    if sources.is_empty() {
        return Err(PagesError::NoSources);
    }

    let mut max_id = 1u32;
    let mut all_pages: BTreeMap<ObjectId, Object> = BTreeMap::new();
    let mut all_objects: BTreeMap<ObjectId, Object> = BTreeMap::new();

    for source in sources {
        let mut doc = Document::load_mem(source)?;
        // Shifts every object id (and every reference to one) in `doc`
        // to start at `max_id`, so merging N documents' object spaces
        // can never collide.
        doc.renumber_objects_with(max_id);
        max_id = doc.max_id + 1;

        for object_id in doc.get_pages().into_values() {
            let object = doc.get_object(object_id)?.to_owned();
            all_pages.insert(object_id, object);
        }
        all_objects.extend(doc.objects);
    }

    let mut merged = Document::with_version("1.5");
    let mut catalog: Option<(ObjectId, Object)> = None;
    let mut pages: Option<(ObjectId, Object)> = None;

    for (object_id, object) in all_objects {
        match object.type_name().unwrap_or(b"") {
            b"Catalog" => {
                let id = catalog.as_ref().map_or(object_id, |(id, _)| *id);
                catalog = Some((id, object));
            }
            b"Pages" => {
                let mut dict = object.as_dict().cloned().unwrap_or_default();
                if let Some((_, existing)) = &pages {
                    if let Ok(existing_dict) = existing.as_dict() {
                        dict.extend(existing_dict);
                    }
                }
                let id = pages.as_ref().map_or(object_id, |(id, _)| *id);
                pages = Some((id, Object::Dictionary(dict)));
            }
            // Pages are collected separately (above) and reinserted with
            // an updated /Parent below; outlines aren't carried over.
            b"Page" | b"Outlines" | b"Outline" => {}
            _ => {
                merged.objects.insert(object_id, object);
            }
        }
    }

    let (pages_id, pages_object) = pages.ok_or(PagesError::NoPages)?;
    let (catalog_id, catalog_object) = catalog.ok_or(PagesError::NoCatalog)?;

    for (page_id, page_object) in &all_pages {
        if let Ok(dict) = page_object.as_dict() {
            let mut dict = dict.clone();
            dict.set("Parent", pages_id);
            merged.objects.insert(*page_id, Object::Dictionary(dict));
        }
    }

    if let Ok(dict) = pages_object.as_dict() {
        let mut dict = dict.clone();
        dict.set("Count", all_pages.len() as u32);
        dict.set(
            "Kids",
            all_pages
                .keys()
                .map(|&id| Object::Reference(id))
                .collect::<Vec<_>>(),
        );
        merged.objects.insert(pages_id, Object::Dictionary(dict));
    }

    if let Ok(dict) = catalog_object.as_dict() {
        let mut dict = dict.clone();
        dict.set("Pages", pages_id);
        dict.remove(b"Outlines");
        merged.objects.insert(catalog_id, Object::Dictionary(dict));
    }

    merged.trailer.set("Root", catalog_id);
    merged.max_id = merged.objects.len() as u32;
    merged.renumber_objects();

    let mut bytes = Vec::new();
    merged.save_to(&mut bytes)?;
    Ok(bytes)
}

/// Extracts `page_indices` (0-based, in the order they should appear in
/// the output) from `source` into a new, standalone document. Every
/// object from `source` is carried over (not just objects reachable from
/// the selected pages) — simpler and strictly correct (no risk of a
/// missing font/image), at the cost of the output carrying some
/// unreferenced objects from pages that weren't selected. A real
/// "prune unreachable objects" pass is a follow-up optimization, not a
/// correctness requirement.
pub fn extract_pages(source: &[u8], page_indices: &[u32]) -> Result<Vec<u8>, PagesError> {
    if page_indices.is_empty() {
        return Err(PagesError::NoPagesSelected);
    }

    let mut doc = Document::load_mem(source)?;
    let pages = doc.get_pages(); // 1-based page number -> id
    let page_count = pages.len() as u32;

    let selected_ids: Vec<ObjectId> = page_indices
        .iter()
        .map(|&index| {
            pages
                .get(&(index + 1))
                .copied()
                .ok_or(PagesError::PageOutOfRange { index, page_count })
        })
        .collect::<Result<_, _>>()?;

    let root_pages_id = root_pages_id(&doc)?;

    for &page_id in &selected_ids {
        let mut page_dict = doc.get_dictionary(page_id)?.clone();
        page_dict.set("Parent", Object::Reference(root_pages_id));
        doc.objects.insert(page_id, Object::Dictionary(page_dict));
    }

    let mut root_dict = doc.get_dictionary(root_pages_id)?.clone();
    root_dict.set(
        "Kids",
        selected_ids
            .into_iter()
            .map(Object::Reference)
            .collect::<Vec<_>>(),
    );
    root_dict.set("Count", page_indices.len() as u32);
    doc.objects
        .insert(root_pages_id, Object::Dictionary(root_dict));

    let mut bytes = Vec::new();
    doc.save_to(&mut bytes)?;
    Ok(bytes)
}

fn root_pages_id(doc: &Document) -> Result<ObjectId, PagesError> {
    let root_id = doc
        .trailer
        .get(b"Root")
        .ok()
        .and_then(|o| o.as_reference().ok())
        .ok_or(PagesError::NoRootPages)?;
    let catalog = doc.get_dictionary(root_id)?;
    catalog
        .get(b"Pages")
        .ok()
        .and_then(|o| o.as_reference().ok())
        .ok_or(PagesError::NoRootPages)
}

#[cfg(test)]
mod tests {
    use super::*;
    use lopdf::content::{Content, Operation};
    use lopdf::{dictionary, Stream};

    fn n_page_pdf_bytes(n: usize, marker_prefix: &str) -> Vec<u8> {
        let mut doc = Document::with_version("1.5");
        let pages_id = doc.new_object_id();
        let page_ids: Vec<_> = (0..n)
            .map(|i| {
                let content = Content {
                    operations: vec![
                        Operation::new("BT", vec![]),
                        Operation::new(
                            "Tj",
                            vec![Object::string_literal(format!("{marker_prefix} {i}"))],
                        ),
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
        doc.save_to(&mut bytes).unwrap();
        bytes
    }

    fn page_marker_text(doc: &Document, page_id: ObjectId) -> String {
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
    fn merge_combines_pages_from_every_source_in_order() {
        let a = n_page_pdf_bytes(2, "a");
        let b = n_page_pdf_bytes(3, "b");

        let merged = merge(&[&a, &b]).expect("merge should succeed");
        let doc = Document::load_mem(&merged).expect("merged output should reparse");
        let pages = doc.get_pages();
        assert_eq!(pages.len(), 5);

        let markers: Vec<String> = (1..=5u32)
            .map(|n| page_marker_text(&doc, pages[&n]))
            .collect();
        assert_eq!(markers, vec!["a 0", "a 1", "b 0", "b 1", "b 2"]);
    }

    #[test]
    fn merge_rejects_empty_source_list() {
        assert!(matches!(merge(&[]), Err(PagesError::NoSources)));
    }

    #[test]
    fn merge_produces_a_document_with_no_object_id_collisions() {
        // Both sources start numbering from (1,0) internally; if
        // renumbering didn't work, the merged doc would silently drop or
        // overwrite one source's objects. Every page from both sources
        // being independently readable is the actual proof.
        let a = n_page_pdf_bytes(2, "a");
        let b = n_page_pdf_bytes(2, "b");
        let merged = merge(&[&a, &b]).expect("merge should succeed");
        let doc = Document::load_mem(&merged).unwrap();
        assert_eq!(doc.get_pages().len(), 4);
        // page_count() succeeding at all (not panicking / not silently
        // truncated) plus the ordered-marker assertion in the test above
        // together prove no collision occurred.
    }

    #[test]
    fn extract_pages_selects_and_reorders_a_subset() {
        let bytes = n_page_pdf_bytes(4, "p");
        let extracted = extract_pages(&bytes, &[2, 0]).expect("extract should succeed");

        let doc = Document::load_mem(&extracted).expect("extracted output should reparse");
        let pages = doc.get_pages();
        assert_eq!(pages.len(), 2);
        let markers: Vec<String> = (1..=2u32)
            .map(|n| page_marker_text(&doc, pages[&n]))
            .collect();
        assert_eq!(markers, vec!["p 2", "p 0"]);
    }

    #[test]
    fn extract_pages_rejects_empty_selection() {
        let bytes = n_page_pdf_bytes(2, "p");
        assert!(matches!(
            extract_pages(&bytes, &[]),
            Err(PagesError::NoPagesSelected)
        ));
    }

    #[test]
    fn extract_pages_rejects_out_of_range_index() {
        let bytes = n_page_pdf_bytes(2, "p");
        let result = extract_pages(&bytes, &[5]);
        assert!(matches!(
            result,
            Err(PagesError::PageOutOfRange {
                index: 5,
                page_count: 2
            })
        ));
    }

    #[test]
    fn extract_pages_can_select_a_single_page() {
        let bytes = n_page_pdf_bytes(5, "p");
        let extracted = extract_pages(&bytes, &[3]).expect("extract should succeed");
        let doc = Document::load_mem(&extracted).unwrap();
        let pages = doc.get_pages();
        assert_eq!(pages.len(), 1);
        assert_eq!(page_marker_text(&doc, pages[&1]), "p 3");
    }
}

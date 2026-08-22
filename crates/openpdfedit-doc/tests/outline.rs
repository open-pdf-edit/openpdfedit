//! Outline (bookmark) extraction.
//!
//! The interesting cases are all about *destinations*, because that's
//! where real documents differ: an explicit array, an action wrapper, a
//! PDF 1.1 named destination, a 1.2+ name tree. Which one a file uses
//! depends on the age of the tool that produced it, so a reader that
//! handles only the obvious form shows a table of contents whose entries
//! do nothing.

use lopdf::{dictionary, Object, Stream};
use openpdfedit_doc::Document;

/// Builds a document with `page_count` pages plus whatever extra
/// catalog entries and objects the caller supplies. Returns the saved
/// bytes and the page object ids, so a test can point a destination at a
/// specific page.
struct Builder {
    doc: lopdf::Document,
    pages_id: (u32, u16),
    page_ids: Vec<(u32, u16)>,
}

impl Builder {
    fn new(page_count: usize) -> Self {
        let mut doc = lopdf::Document::with_version("1.5");
        let pages_id = doc.new_object_id();
        let page_ids: Vec<_> = (0..page_count)
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
                "Count" => page_count as i64,
            }),
        );
        Builder {
            doc,
            pages_id,
            page_ids,
        }
    }

    fn finish(mut self, catalog_extras: lopdf::Dictionary) -> Vec<u8> {
        let mut catalog = dictionary! { "Type" => "Catalog", "Pages" => self.pages_id };
        for (key, value) in catalog_extras.iter() {
            catalog.set(key.clone(), value.clone());
        }
        let catalog_id = self.doc.add_object(catalog);
        self.doc.trailer.set("Root", catalog_id);
        let mut bytes = Vec::new();
        self.doc.save_to(&mut bytes).unwrap();
        bytes
    }
}

#[test]
fn a_document_without_an_outline_returns_an_empty_tree() {
    let bytes = Builder::new(2).finish(dictionary! {});
    let doc = Document::from_bytes(&bytes).expect("should parse");
    assert!(doc.outline().expect("outline should succeed").is_empty());
}

#[test]
fn a_nested_outline_keeps_its_shape_and_order() {
    let mut b = Builder::new(4);
    let outlines_id = b.doc.new_object_id();

    let child = b.doc.add_object(dictionary! {
        "Title" => Object::string_literal("Sub-clause"),
        "Parent" => outlines_id,
        "Dest" => vec![b.page_ids[3].into(), "XYZ".into()],
    });
    let second = b.doc.add_object(dictionary! {
        "Title" => Object::string_literal("Second"),
        "Parent" => outlines_id,
        "Dest" => vec![b.page_ids[2].into(), "Fit".into()],
    });
    let first = b.doc.add_object(dictionary! {
        "Title" => Object::string_literal("First"),
        "Parent" => outlines_id,
        "Next" => second,
        "First" => child,
        "Last" => child,
        "Dest" => vec![b.page_ids[1].into(), "Fit".into()],
    });
    b.doc.objects.insert(
        outlines_id,
        Object::Dictionary(dictionary! {
            "Type" => "Outlines",
            "First" => first,
            "Last" => second,
        }),
    );
    let bytes = b.finish(dictionary! { "Outlines" => outlines_id });

    let doc = Document::from_bytes(&bytes).expect("should parse");
    let outline = doc.outline().expect("outline should succeed");

    assert_eq!(outline.len(), 2, "two top-level entries");
    assert_eq!(outline[0].title, "First");
    assert_eq!(outline[0].page_index, Some(1));
    assert_eq!(outline[1].title, "Second");
    assert_eq!(outline[1].page_index, Some(2));
    assert_eq!(outline[0].children.len(), 1);
    assert_eq!(outline[0].children[0].title, "Sub-clause");
    assert_eq!(outline[0].children[0].page_index, Some(3));
}

/// The action form: a bookmark whose destination lives under `/A` with
/// `/S /GoTo` rather than directly under `/Dest`.
#[test]
fn a_goto_action_resolves_the_same_as_a_direct_destination() {
    let mut b = Builder::new(3);
    let outlines_id = b.doc.new_object_id();
    let item = b.doc.add_object(dictionary! {
        "Title" => Object::string_literal("Via action"),
        "Parent" => outlines_id,
        "A" => dictionary! {
            "S" => "GoTo",
            "D" => vec![b.page_ids[2].into(), "Fit".into()],
        },
    });
    b.doc.objects.insert(
        outlines_id,
        Object::Dictionary(dictionary! { "Type" => "Outlines", "First" => item, "Last" => item }),
    );
    let bytes = b.finish(dictionary! { "Outlines" => outlines_id });

    let doc = Document::from_bytes(&bytes).expect("should parse");
    let outline = doc.outline().expect("outline should succeed");
    assert_eq!(outline[0].page_index, Some(2));
}

/// A URI action has no page. The entry must still appear — a bookmark
/// you can see but not follow beats a table of contents with holes.
#[test]
fn a_non_goto_action_yields_an_entry_with_no_page() {
    let mut b = Builder::new(2);
    let outlines_id = b.doc.new_object_id();
    let item = b.doc.add_object(dictionary! {
        "Title" => Object::string_literal("Our website"),
        "Parent" => outlines_id,
        "A" => dictionary! { "S" => "URI", "URI" => Object::string_literal("https://example.com") },
    });
    b.doc.objects.insert(
        outlines_id,
        Object::Dictionary(dictionary! { "Type" => "Outlines", "First" => item, "Last" => item }),
    );
    let bytes = b.finish(dictionary! { "Outlines" => outlines_id });

    let doc = Document::from_bytes(&bytes).expect("should parse");
    let outline = doc.outline().expect("outline should succeed");
    assert_eq!(outline.len(), 1);
    assert_eq!(outline[0].title, "Our website");
    assert_eq!(outline[0].page_index, None);
}

/// The PDF 1.1 form: a `/Dests` dictionary in the catalog.
#[test]
fn a_pdf_1_1_named_destination_resolves() {
    let mut b = Builder::new(3);
    let outlines_id = b.doc.new_object_id();
    let item = b.doc.add_object(dictionary! {
        "Title" => Object::string_literal("Appendix"),
        "Parent" => outlines_id,
        "Dest" => Object::Name(b"appendix".to_vec()),
    });
    b.doc.objects.insert(
        outlines_id,
        Object::Dictionary(dictionary! { "Type" => "Outlines", "First" => item, "Last" => item }),
    );
    let appendix_dest: Vec<Object> = vec![b.page_ids[2].into(), "Fit".into()];
    let bytes = b.finish(dictionary! {
        "Outlines" => outlines_id,
        "Dests" => dictionary! { "appendix" => appendix_dest },
    });

    let doc = Document::from_bytes(&bytes).expect("should parse");
    assert_eq!(doc.outline().unwrap()[0].page_index, Some(2));
}

/// The PDF 1.2+ form: a `/Names /Dests` name tree, including the
/// `/D`-wrapped dictionary shape.
#[test]
fn a_name_tree_destination_resolves_through_its_kids() {
    let mut b = Builder::new(5);
    let outlines_id = b.doc.new_object_id();
    let item = b.doc.add_object(dictionary! {
        "Title" => Object::string_literal("Schedule B"),
        "Parent" => outlines_id,
        "Dest" => Object::string_literal("schedule-b"),
    });
    b.doc.objects.insert(
        outlines_id,
        Object::Dictionary(dictionary! { "Type" => "Outlines", "First" => item, "Last" => item }),
    );

    // Two branches, so the /Limits pruning is exercised: the name lives
    // in the second one.
    let leaf_a = b.doc.add_object(dictionary! {
        "Limits" => vec![Object::string_literal("aaa"), Object::string_literal("mmm")],
        "Names" => vec![
            Object::string_literal("intro"),
            vec![b.page_ids[0].into(), "Fit".into()].into(),
        ],
    });
    let leaf_b = b.doc.add_object(dictionary! {
        "Limits" => vec![Object::string_literal("nnn"), Object::string_literal("zzz")],
        "Names" => vec![
            Object::string_literal("schedule-b"),
            // The /D-wrapped shape, which is equally legal.
            Object::Dictionary(dictionary! {
                "D" => vec![b.page_ids[4].into(), "Fit".into()],
            }),
        ],
    });
    let tree_root = b
        .doc
        .add_object(dictionary! { "Kids" => vec![leaf_a.into(), leaf_b.into()] });
    let bytes = b.finish(dictionary! {
        "Outlines" => outlines_id,
        "Names" => dictionary! { "Dests" => tree_root },
    });

    let doc = Document::from_bytes(&bytes).expect("should parse");
    assert_eq!(doc.outline().unwrap()[0].page_index, Some(4));
}

/// `/Next` is a raw object reference, so a corrupted file can describe a
/// loop. Walking one naively never returns.
#[test]
fn a_cyclic_next_chain_terminates() {
    let mut b = Builder::new(2);
    let outlines_id = b.doc.new_object_id();
    let first_id = b.doc.new_object_id();
    let second = b.doc.add_object(dictionary! {
        "Title" => Object::string_literal("B"),
        "Parent" => outlines_id,
        "Next" => first_id,
    });
    b.doc.objects.insert(
        first_id,
        Object::Dictionary(dictionary! {
            "Title" => Object::string_literal("A"),
            "Parent" => outlines_id,
            "Next" => second,
        }),
    );
    b.doc.objects.insert(
        outlines_id,
        Object::Dictionary(
            dictionary! { "Type" => "Outlines", "First" => first_id, "Last" => second },
        ),
    );
    let bytes = b.finish(dictionary! { "Outlines" => outlines_id });

    let doc = Document::from_bytes(&bytes).expect("should parse");
    let outline = doc.outline().expect("outline should succeed");
    assert_eq!(
        outline.len(),
        2,
        "each entry appears once, then the loop stops"
    );
}

/// A title stored as UTF-16BE — how any non-ASCII heading arrives. Read
/// as bytes it renders as `\0T\0i\0t\0l\0e`.
#[test]
fn a_utf16_title_is_decoded() {
    let mut b = Builder::new(1);
    let outlines_id = b.doc.new_object_id();
    let mut utf16 = vec![0xFE, 0xFF];
    for unit in "Résumé — 概要".encode_utf16() {
        utf16.extend_from_slice(&unit.to_be_bytes());
    }
    let item = b.doc.add_object(dictionary! {
        "Title" => Object::String(utf16, lopdf::StringFormat::Literal),
        "Parent" => outlines_id,
    });
    b.doc.objects.insert(
        outlines_id,
        Object::Dictionary(dictionary! { "Type" => "Outlines", "First" => item, "Last" => item }),
    );
    let bytes = b.finish(dictionary! { "Outlines" => outlines_id });

    let doc = Document::from_bytes(&bytes).expect("should parse");
    assert_eq!(doc.outline().unwrap()[0].title, "Résumé — 概要");
}

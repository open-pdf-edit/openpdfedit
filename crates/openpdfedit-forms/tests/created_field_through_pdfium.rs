//! Cross-crate validation: creates a text field and a checkbox via
//! `openpdfedit-forms`, saves incrementally, and confirms — through real
//! PDFium's own `PdfiumEngine::list_form_fields` (the same M4 form-*fill*
//! machinery, already independently tested against real AcroForm PDFs) —
//! that both fields are recognized as real, fillable AcroForm fields,
//! not just structurally present according to this crate's own
//! bookkeeping. Then fills them through that same PDFium path and
//! confirms the values round-trip, proving a field created here is
//! genuinely usable by the rest of the app, not a dead end.

use openpdfedit_doc::Document;
use openpdfedit_engine::{Engine, FormFieldKind, PdfiumEngine};
use openpdfedit_forms::{create_field, NewField, NewFieldKind};
use std::collections::HashMap;

fn dev_vendor_lib_dir() -> Option<std::path::PathBuf> {
    let manifest_dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    // crates/openpdfedit-forms -> workspace root
    let workspace_root = manifest_dir.parent()?.parent()?;
    let dir = workspace_root.join(if cfg!(windows) {
        ".vendor/pdfium/bin"
    } else {
        ".vendor/pdfium/lib"
    });
    dir.exists().then_some(dir)
}

fn minimal_pdf_bytes() -> Vec<u8> {
    use lopdf::{dictionary, Object};

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
    let catalog_id = doc.add_object(dictionary! { "Type" => "Catalog", "Pages" => pages_id });
    doc.trailer.set("Root", catalog_id);
    let mut bytes = Vec::new();
    doc.save_to(&mut bytes).unwrap();
    bytes
}

#[test]
fn a_field_created_here_is_recognized_and_fillable_through_real_pdfium() {
    let Ok(engine) = PdfiumEngine::new(dev_vendor_lib_dir().as_deref()) else {
        eprintln!("skipping: PDFium not available (run scripts/fetch-pdfium.sh)");
        return;
    };

    let tmp_path = std::env::temp_dir().join(format!(
        "openpdfedit-forms-create-test-{}.pdf",
        std::process::id()
    ));
    std::fs::write(&tmp_path, minimal_pdf_bytes()).expect("should write temp file");

    let mut doc = Document::open(&tmp_path).expect("doc crate should open the temp file");
    create_field(
        &mut doc,
        NewField {
            page_index: 0,
            name: "full_name".into(),
            rect: [50.0, 700.0, 250.0, 720.0],
            kind: NewFieldKind::Text,
        },
    )
    .expect("create text field should succeed");
    create_field(
        &mut doc,
        NewField {
            page_index: 0,
            name: "agree".into(),
            rect: [50.0, 650.0, 65.0, 665.0],
            kind: NewFieldKind::Checkbox,
        },
    )
    .expect("create checkbox field should succeed");
    let saved = doc.save_incremental().expect("save should succeed");
    std::fs::write(&tmp_path, &saved).expect("should overwrite with the fields added");

    let handle = engine
        .open(&tmp_path)
        .expect("PDFium should open the file with the new fields");

    let fields = engine
        .list_form_fields(handle)
        .expect("list_form_fields should succeed");
    assert_eq!(
        fields.len(),
        2,
        "PDFium should independently see both new fields"
    );

    let text_field = fields
        .iter()
        .find(|f| f.name == "full_name")
        .expect("PDFium should recognize the text field by name");
    assert_eq!(text_field.kind, FormFieldKind::Text);

    let checkbox_field = fields
        .iter()
        .find(|f| f.name == "agree")
        .expect("PDFium should recognize the checkbox field by name");
    assert_eq!(checkbox_field.kind, FormFieldKind::Checkbox);
    assert_eq!(checkbox_field.is_checked, Some(false));

    // Fill both through the *existing*, already-tested M4 fill path —
    // proof a field created here is genuinely usable end-to-end, not
    // just structurally present.
    let mut values = HashMap::new();
    values.insert("full_name".to_string(), "Ada Lovelace".to_string());
    values.insert("agree".to_string(), "true".to_string());
    engine
        .fill_form_fields(handle, &values)
        .expect("filling the newly created fields should succeed");

    let tmp_path2 = std::env::temp_dir().join(format!(
        "openpdfedit-forms-create-test-out-{}.pdf",
        std::process::id()
    ));
    engine
        .save_document(handle, &tmp_path2)
        .expect("save should succeed");
    engine.close(handle);

    let reopened = engine
        .open(&tmp_path2)
        .expect("PDFium should reopen the filled file");
    let after = engine
        .list_form_fields(reopened)
        .expect("list should succeed on the filled file");
    let text_after = after.iter().find(|f| f.name == "full_name").unwrap();
    assert_eq!(text_after.value.as_deref(), Some("Ada Lovelace"));
    let checkbox_after = after.iter().find(|f| f.name == "agree").unwrap();
    assert_eq!(checkbox_after.is_checked, Some(true));

    engine.close(reopened);
    let _ = std::fs::remove_file(&tmp_path);
    let _ = std::fs::remove_file(&tmp_path2);
}

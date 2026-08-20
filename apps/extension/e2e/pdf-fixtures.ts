// Shared embedded PDF fixtures for this package's e2e suite — pulled out
// of wasm-session.spec.ts (Phase 5 final-review fix round, I3) so
// ui-flows.spec.ts can reuse the exact same bytes rather than embedding a
// second copy of either base64 blob. Both specs still each build their own
// Uint8Array from these strings independently (base64 -> bytes is trivial
// and each spec's own execution context — in-page `page.evaluate` for
// wasm-session.spec.ts, this file's Node-side `Buffer` for ui-flows.spec.ts's
// picker-stub seeding — needs its own decode step regardless of where the
// string itself lives).

// A minimal one-page AcroForm PDF with a single Text field named
// "full_name" — hand-built via lopdf, the same construction
// `openpdfedit-session/src/forms.rs`'s own test module uses
// (`acroform_pdf_bytes()`), trimmed to just the text field since this
// suite only needs one field to exercise listFormFields/fillFormFields
// end-to-end. lopdf isn't available in JS, so these bytes were generated
// once with a throwaway Rust binary (not checked in) built from exactly
// this snippet, run from the repo root:
//
//   use lopdf::content::{Content, Operation};
//   use lopdf::{dictionary, Object, Stream};
//   let mut doc = lopdf::Document::with_version("1.7");
//   let pages_id = doc.new_object_id();
//   let content = Content { operations: vec![Operation::new("BT", vec![]), Operation::new("ET", vec![])] };
//   let content_id = doc.add_object(Stream::new(dictionary! {}, content.encode().unwrap()));
//   let text_field_id = doc.add_object(dictionary! {
//       "Type" => "Annot", "Subtype" => "Widget", "FT" => "Tx",
//       "T" => Object::string_literal("full_name"),
//       "Rect" => vec![50.into(), 700.into(), 250.into(), 720.into()],
//       "V" => Object::string_literal(""), "DA" => Object::string_literal("/Helv 0 Tf 0 g"),
//   });
//   let page_id = doc.add_object(dictionary! {
//       "Type" => "Page", "Parent" => pages_id,
//       "MediaBox" => vec![0.into(), 0.into(), 612.into(), 792.into()],
//       "Contents" => content_id, "Resources" => dictionary! {},
//       "Annots" => vec![text_field_id.into()],
//   });
//   doc.objects.insert(pages_id, Object::Dictionary(dictionary! {
//       "Type" => "Pages", "Kids" => vec![page_id.into()], "Count" => 1,
//   }));
//   let acroform_id = doc.add_object(dictionary! { "Fields" => vec![text_field_id.into()] });
//   let catalog_id = doc.add_object(dictionary! {
//       "Type" => "Catalog", "Pages" => pages_id, "AcroForm" => acroform_id,
//   });
//   doc.trailer.set("Root", catalog_id);
//   let mut bytes = Vec::new();
//   doc.save_to(&mut bytes).unwrap();
//   // then: stdout the bytes, `| base64 | tr -d '\n'`
export const FORM_PDF_BASE64 =
  "JVBERi0xLjcKJbutwN4KMSAwIG9iago8PC9UeXBlL1BhZ2VzL0tpZHNbNCAwIFJdL0NvdW50IDE+PgplbmRvYmoKMiAwIG9iago8PC9MZW5ndGggNT4+c3RyZWFtCkJUCkVUCmVuZHN0cmVhbSAKZW5kb2JqCjMgMCBvYmoKPDwvVHlwZS9Bbm5vdC9TdWJ0eXBlL1dpZGdldC9GVC9UeC9UKGZ1bGxfbmFtZSkvUmVjdFs1MCA3MDAgMjUwIDcyMF0vVigpL0RBKC9IZWx2IDAgVGYgMCBnKT4+CmVuZG9iago0IDAgb2JqCjw8L1R5cGUvUGFnZS9QYXJlbnQgMSAwIFIvTWVkaWFCb3hbMCAwIDYxMiA3OTJdL0NvbnRlbnRzIDIgMCBSL1Jlc291cmNlczw8Pj4vQW5ub3RzWzMgMCBSXT4+CmVuZG9iago1IDAgb2JqCjw8L0ZpZWxkc1szIDAgUl0+PgplbmRvYmoKNiAwIG9iago8PC9UeXBlL0NhdGFsb2cvUGFnZXMgMSAwIFIvQWNyb0Zvcm0gNSAwIFI+PgplbmRvYmoKNyAwIG9iago8PC9Sb290IDYgMCBSL1R5cGUvWFJlZi9TaXplIDgvV1sxIDQgMl0vSW5kZXhbMSA3XS9MZW5ndGggNDk+PnN0cmVhbQoBAAAADwAAAQAAAEIAAAEAAAB2AAABAAAA4wAAAQAAAU8AAAEAAAFxAAABAAABrQAACmVuZHN0cmVhbSAKZW5kb2JqCgpzdGFydHhyZWYKNDI5CiUlRU9G";

// A minimal one-page PDF with a single, real page-content text run
// ("CONFIDENTIAL" at PDF-point (50, 50), 24pt Helvetica/WinAnsiEncoding)
// — needed because FORM_PDF above is a minimal AcroForm PDF whose one
// page has an *empty* content stream (`BT ET`, no `Tj`): its form field
// lives entirely in an annotation appearance, not in page content, so
// `listTextRuns`/`editTextRun`/`redactPage` (which all operate on page
// *content*, not annotation appearances — see openpdfedit-textedit's and
// openpdfedit-redact's module docs) have nothing to exercise against it.
// ui-flows.spec.ts also relies on this one being a real, drag-selectable
// piece of page text: it's what the highlight-drag gesture needs
// `textSelectionQuads` to actually find something under.
// Hand-built via lopdf, identical construction to
// `openpdfedit-session/src/redact.rs`'s own `text_page_pdf_bytes` test
// helper (`text_page_pdf_bytes("CONFIDENTIAL", 50.0, 50.0, 24.0)`) — the
// bytes were generated once by temporarily adding a `#[test]` to that
// file's test module that wrote the helper's output to
// `/tmp/openpdfedit-e2e-text-fixture.pdf` via `std::fs::write`, running
// it with `cargo test -p openpdfedit-session scratch_dump_text_fixture`,
// then `base64 -i /tmp/openpdfedit-e2e-text-fixture.pdf | tr -d '\n'`
// (the temporary test was removed before committing; not part of the
// shipped suite).
export const TEXT_PDF_BASE64 =
  "JVBERi0xLjUKJbutwN4KMSAwIG9iago8PC9UeXBlL1BhZ2VzL0tpZHNbNCAwIFJdL0NvdW50IDE+PgplbmRvYmoKMiAwIG9iago8PC9UeXBlL0ZvbnQvU3VidHlwZS9UeXBlMS9CYXNlRm9udC9IZWx2ZXRpY2EvRW5jb2RpbmcvV2luQW5zaUVuY29kaW5nPj4KZW5kb2JqCjMgMCBvYmoKPDwvTGVuZ3RoIDQyPj5zdHJlYW0KQlQKL0YxIDI0IFRmCjUwIDUwIFRkCihDT05GSURFTlRJQUwpIFRqCkVUCmVuZHN0cmVhbSAKZW5kb2JqCjQgMCBvYmoKPDwvVHlwZS9QYWdlL1BhcmVudCAxIDAgUi9NZWRpYUJveFswIDAgNjEyIDc5Ml0vQ29udGVudHMgMyAwIFIvUmVzb3VyY2VzPDwvRm9udDw8L0YxIDIgMCBSPj4+Pj4+CmVuZG9iago1IDAgb2JqCjw8L1R5cGUvQ2F0YWxvZy9QYWdlcyAxIDAgUj4+CmVuZG9iago2IDAgb2JqCjw8L1Jvb3QgNSAwIFIvVHlwZS9YUmVmL1NpemUgNy9XWzEgNCAyXS9JbmRleFsxIDZdL0xlbmd0aCA0Mj4+c3RyZWFtCgEAAAAPAAABAAAAQgAAAQAAAJoAAAEAAAD0AAABAAABZAAAAQAAAZEAAAplbmRzdHJlYW0gCmVuZG9iagoKc3RhcnR4cmVmCjQwMQolJUVPRg==";

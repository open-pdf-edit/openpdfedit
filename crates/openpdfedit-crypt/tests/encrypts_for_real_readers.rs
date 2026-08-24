//! The only test that means anything here: does a *third-party* reader
//! accept what we wrote?
//!
//! Our own code agreeing with our own code proves nothing about
//! encryption — a wrong key schedule produces a self-consistent file
//! that no other reader can open. So every assertion below goes through
//! PDFium, which implements AES-256 independently and is the same engine
//! Chrome uses.
//!
//! The properties under test: the result refuses the wrong password,
//! accepts the right one (user *and* owner), and renders pixel-identical
//! to the document that went in.

use std::sync::OnceLock;

use openpdfedit_crypt::{encrypt_document, is_encrypted, Permissions};
use openpdfedit_engine::{EngineHandle, RenderedTile};

fn dev_vendor_lib_dir() -> Option<std::path::PathBuf> {
    let manifest_dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let workspace_root = manifest_dir.parent()?.parent()?;
    let dir = workspace_root.join(if cfg!(windows) {
        ".vendor/pdfium/bin"
    } else {
        ".vendor/pdfium/lib"
    });
    dir.exists().then_some(dir)
}

/// One engine per process. Reached through `EngineHandle` rather than
/// the raw bindings, because nothing outside `openpdfedit-engine` may
/// name a `pdfium-render` type — the boundary that keeps a future
/// non-PDFium backend a swap rather than a rewrite.
fn pdfium() -> Option<&'static EngineHandle> {
    static ENGINE: OnceLock<Option<EngineHandle>> = OnceLock::new();
    ENGINE
        .get_or_init(|| EngineHandle::spawn(dev_vendor_lib_dir()).ok())
        .as_ref()
}

/// A page with visible content, so "renders the same" has something to
/// compare.
fn source_pdf() -> Vec<u8> {
    use lopdf::{dictionary, Object, Stream};
    let mut doc = lopdf::Document::with_version("1.7");
    let pages_id = doc.new_object_id();
    let font_id = doc.add_object(dictionary! {
        "Type" => "Font", "Subtype" => "Type1",
        "BaseFont" => "Helvetica", "Encoding" => "WinAnsiEncoding",
    });
    let content_id = doc.add_object(Stream::new(
        dictionary! {},
        b"1 0 0 rg 100 500 300 200 re f\nBT /F1 36 Tf 100 300 Td (Confidential) Tj ET\n".to_vec(),
    ));
    let page_id = doc.add_object(dictionary! {
        "Type" => "Page",
        "Parent" => pages_id,
        "MediaBox" => vec![0.into(), 0.into(), 612.into(), 792.into()],
        "Contents" => content_id,
        "Resources" => dictionary! { "Font" => dictionary! { "F1" => font_id } },
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
    bytes
}

/// Opens `bytes` (optionally with a password) and returns page 1's
/// pixels, or `None` if it wouldn't open at all.
fn render(engine: &EngineHandle, bytes: &[u8], password: Option<&str>) -> Option<Vec<u8>> {
    let handle = open(engine, bytes, password)?;
    let tile: std::sync::Arc<RenderedTile> = engine.render_page(handle, 0, 300).ok()?;
    engine.close(handle);
    Some(tile.rgba.clone())
}

fn open(engine: &EngineHandle, bytes: &[u8], password: Option<&str>) -> Option<u64> {
    match password {
        Some(pw) => engine.open_bytes_with_password(bytes.to_vec(), pw).ok(),
        None => engine.open_bytes(bytes.to_vec()).ok(),
    }
}

/// Whether the document opens at all with the given password.
fn opens(engine: &EngineHandle, bytes: &[u8], password: Option<&str>) -> bool {
    match open(engine, bytes, password) {
        Some(handle) => {
            engine.close(handle);
            true
        }
        None => false,
    }
}

#[test]
fn an_encrypted_document_needs_its_password_and_then_looks_the_same() {
    let Some(pdfium) = pdfium() else {
        eprintln!("skipping: PDFium not available — run scripts/fetch-pdfium.sh");
        return;
    };

    let plain = source_pdf();
    let before = render(pdfium, &plain, None).expect("the source should render");

    let encrypted = encrypt_document(&plain, "s3cret", "0wner", Permissions::default())
        .expect("encryption should succeed");
    assert!(is_encrypted(&encrypted));
    assert!(!is_encrypted(&plain));

    // Refuses without, and with the wrong password.
    assert!(
        !opens(pdfium, &encrypted, None),
        "opened with no password at all"
    );
    assert!(
        !opens(pdfium, &encrypted, Some("wrong")),
        "opened with the wrong password"
    );

    // Accepts the user password, and the content survived intact.
    let after = render(pdfium, &encrypted, Some("s3cret"))
        .expect("PDFium should open it with the user password");
    assert_eq!(
        after, before,
        "the page renders differently after encryption"
    );

    // ...and the owner password, which is a separate derivation path
    // (Algorithm 9 hashes the whole 48-byte /U as well).
    let as_owner = render(pdfium, &encrypted, Some("0wner"))
        .expect("PDFium should open it with the owner password");
    assert_eq!(as_owner, before);
}

/// The common case: one password for both roles.
#[test]
fn the_same_password_can_serve_as_user_and_owner() {
    let Some(pdfium) = pdfium() else { return };
    let encrypted = encrypt_document(&source_pdf(), "same", "same", Permissions::default())
        .expect("encryption should succeed");
    assert!(opens(pdfium, &encrypted, Some("same")));
}

/// An empty owner password means "reuse the user password", so the
/// document must not end up with an empty owner password that opens it.
#[test]
fn an_omitted_owner_password_falls_back_to_the_user_password() {
    let Some(pdfium) = pdfium() else { return };
    let encrypted = encrypt_document(&source_pdf(), "onlyone", "", Permissions::default())
        .expect("encryption should succeed");
    assert!(opens(pdfium, &encrypted, Some("onlyone")));
    assert!(
        !opens(pdfium, &encrypted, Some("")),
        "an empty password opened the document"
    );
}

/// Non-ASCII passwords are ordinary outside English-speaking offices,
/// and are where a naive byte-vs-char mistake shows up.
#[test]
fn a_unicode_password_round_trips() {
    let Some(pdfium) = pdfium() else { return };
    let password = "contraseña-密码-🔐";
    let encrypted = encrypt_document(&source_pdf(), password, password, Permissions::default())
        .expect("encryption should succeed");
    assert!(
        opens(pdfium, &encrypted, Some(password)),
        "a unicode password didn't open its own document"
    );
}

/// Permissions ride in /P and are echoed in the tamper-check /Perms
/// block; a mismatch makes conforming readers reject the document
/// outright, so this is really a check that Algorithm 10 agrees with /P.
#[test]
fn restricted_permissions_still_produce_a_readable_document() {
    let Some(pdfium) = pdfium() else { return };
    let locked_down = Permissions {
        print: false,
        modify: false,
        copy: false,
        annotate: false,
        fill_forms: false,
        extract_for_accessibility: false,
        assemble: false,
        print_high_resolution: false,
    };
    let encrypted = encrypt_document(&source_pdf(), "pw", "pw", locked_down)
        .expect("encryption should succeed");
    assert!(
        opens(pdfium, &encrypted, Some("pw")),
        "PDFium rejected the document, which means /Perms disagrees with /P"
    );
}

#[test]
fn encrypting_twice_is_refused_rather_than_producing_an_unopenable_file() {
    let encrypted = encrypt_document(&source_pdf(), "pw", "pw", Permissions::default()).unwrap();
    assert!(encrypt_document(&encrypted, "pw2", "pw2", Permissions::default()).is_err());
}

#[test]
fn an_empty_password_is_refused() {
    assert!(encrypt_document(&source_pdf(), "", "", Permissions::default()).is_err());
}

/// The synthetic fixture above is a page this crate built itself. Real
/// documents carry compressed streams, embedded fonts, object streams
/// and text in encodings nobody chose deliberately — so the claim "any
/// PDF" is checked against the real-world corpus rather than only
/// against something shaped conveniently.
#[test]
fn real_world_documents_from_the_corpus_encrypt_and_still_render() {
    let Some(engine) = pdfium() else { return };
    let corpus = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("testdata/corpus");
    if !corpus.exists() {
        eprintln!("skipping: corpus not fetched — run scripts/fetch-test-corpus.sh");
        return;
    }

    let mut checked = 0;
    for entry in std::fs::read_dir(&corpus).expect("corpus should be readable") {
        let path = entry.expect("dir entry").path();
        if path.extension().map(|e| e != "pdf").unwrap_or(true) {
            continue;
        }
        let Ok(plain) = std::fs::read(&path) else {
            continue;
        };
        // Only documents PDFium can render to begin with — the corpus
        // deliberately includes fuzzed and damaged files, and this test
        // is about encryption, not about parsing broken input.
        let Some(before) = render(engine, &plain, None) else {
            continue;
        };

        let name = path.file_name().unwrap().to_string_lossy().to_string();
        let encrypted =
            match encrypt_document(&plain, "corpus-pw", "corpus-pw", Permissions::default()) {
                Ok(bytes) => bytes,
                // A document lopdf itself can't re-serialize is a separate
                // known problem, not an encryption failure.
                Err(_) => continue,
            };

        assert!(
            !opens(engine, &encrypted, None),
            "{name}: encrypted copy opened with no password"
        );
        let after = render(engine, &encrypted, Some("corpus-pw"))
            .unwrap_or_else(|| panic!("{name}: encrypted copy wouldn't open with its password"));
        assert_eq!(
            after, before,
            "{name}: renders differently after encryption"
        );
        checked += 1;
    }

    assert!(
        checked >= 5,
        "expected several corpus documents to be exercised, only managed {checked}"
    );
    eprintln!("encrypted and re-rendered {checked} real-world documents");
}

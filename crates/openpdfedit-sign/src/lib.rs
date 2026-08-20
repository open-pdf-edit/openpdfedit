//! Digital-signature **inspection**, not verification.
//!
//! ## Why cryptographic verification is NOT implemented here
//!
//! Real signature verification needs: CMS/PKCS#7 `SignedData` ASN.1
//! parsing, RSA/ECDSA signature verification against the embedded
//! certificate's public key, X.509 certificate-chain building, and —
//! for anything beyond "is this signature internally self-consistent" —
//! validating the signer's certificate against a trust anchor (AATL/EUTL)
//! plus OCSP/CRL revocation checking. That's a genuinely large,
//! security-sensitive undertaking, and unlike most of this codebase's
//! other scope decisions, the failure mode of getting it subtly wrong
//! isn't "a feature works less well than hoped" — it's "this app tells a
//! user a tampered or forged document is validly signed." Investigated
//! the crypto side before starting on this milestone: RustCrypto's `cms`
//! crate (the natural choice for `SignedData` parsing) was still
//! pre-release (`0.3.0-pre.2`) as of this writing, which is itself a
//! signal this isn't yet a "just wire it up" undertaking even with a
//! mature ecosystem crate underneath it. Rather than rush a partial or
//! under-tested implementation of something this consequential — the
//! same call already made for encryption in `openpdfedit-doc`, for the
//! same underlying reason — this pass implements only the *structural*
//! layer below: finding every signature in a document and reporting what
//! the PDF itself declares about it, plus one purely mechanical
//! structural sanity check. **None of this proves a signature is
//! cryptographically valid, trusted, or even genuinely tied to the bytes
//! it claims to cover** — it answers "what does this document claim,"
//! not "is that claim true." A UI surfacing this data must label it
//! accordingly (see `apps/desktop`'s signature display).
//!
//! Real verification, when it's built, needs at minimum: `cms` (or
//! equivalent SignedData parsing — re-check for a stable release first),
//! `x509-cert`, `rsa`/`p256`/`p384` for the signature math, `sha1`/`sha2`
//! for digest algorithms PDF signatures actually use, and — for trust,
//! not just self-consistency — a bundled or fetched AATL/EUTL trust
//! anchor list. Tracked as a standing TODO for its own dedicated,
//! carefully-tested pass; do not bolt cryptographic verification onto
//! this module without that dedicated effort.
//!
//! ## What this module actually does
//!
//! [`find_signatures`] scans a PDF's raw object graph (via `lopdf`, no
//! PDFium involved — everything here is plain structural parsing) for
//! every `/Type /Sig` or `/Type /DocTimeStamp` dictionary and extracts
//! what it declares: `/SubFilter` (the signature format), signer-supplied
//! `/Reason`/`/Name`/`/M` (signing time — reported as the raw PDF date
//! string, unparsed), `/ByteRange`, and the length of the `/Contents`
//! placeholder. It also checks one purely structural property: does
//! `/ByteRange` have the shape `[0, a, b, len-b]` that a well-formed
//! signature must (the range covering everything in the file *except*
//! the `/Contents` placeholder itself) — see
//! [`SignatureInfo::byte_range_is_structurally_sound`]. A `false` there
//! is a strong signal something is wrong (a malformed or hand-tampered
//! signature dictionary); a `true` is not evidence of anything beyond
//! "this specific structural shape looks right."

use lopdf::{Dictionary, Object};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum SignError {
    #[error("failed to load PDF: {0}")]
    Load(#[from] lopdf::Error),
}

/// What a PDF declares about one signature — structural facts read
/// straight from the document, not a cryptographic verdict. See this
/// crate's module doc.
#[derive(Debug, Clone, PartialEq)]
pub struct SignatureInfo {
    /// e.g. `"adbe.pkcs7.detached"`, `"ETSI.CAdES.detached"`.
    pub sub_filter: Option<String>,
    pub reason: Option<String>,
    pub name: Option<String>,
    /// Raw PDF date string (e.g. `"D:20260801120000+00'00'"`), unparsed.
    pub signing_time: Option<String>,
    pub byte_range: Option<[i64; 4]>,
    /// Length in bytes of the `/Contents` placeholder (the raw signature
    /// bytes, DER-encoded PKCS#1 or PKCS#7).
    pub contents_len_bytes: usize,
    /// See this struct's doc and the crate module doc: a structural
    /// sanity check only, not a cryptographic one.
    pub byte_range_is_structurally_sound: bool,
}

/// Finds every signature (`/Type /Sig` or `/Type /DocTimeStamp`
/// dictionary) in `pdf_bytes` and reports what the document declares
/// about each. See this crate's module doc for what this does and
/// (importantly) does not prove.
pub fn find_signatures(pdf_bytes: &[u8]) -> Result<Vec<SignatureInfo>, SignError> {
    let doc = lopdf::Document::load_mem(pdf_bytes)?;
    let mut signatures = Vec::new();

    for object in doc.objects.values() {
        let Ok(dict) = object.as_dict() else {
            continue;
        };
        let is_signature = matches!(
            dict.get(b"Type").and_then(|o| o.as_name()),
            Ok(t) if t == b"Sig" || t == b"DocTimeStamp"
        );
        if !is_signature {
            continue;
        }

        let sub_filter = dict
            .get(b"SubFilter")
            .and_then(|o| o.as_name())
            .ok()
            .map(|n| String::from_utf8_lossy(n).into_owned());
        let reason = string_field(dict, b"Reason");
        let name = string_field(dict, b"Name");
        let signing_time = string_field(dict, b"M");

        let byte_range = dict
            .get(b"ByteRange")
            .and_then(|o| o.as_array())
            .ok()
            .and_then(|arr| parse_byte_range(arr));

        let contents_len_bytes = dict
            .get(b"Contents")
            .ok()
            .and_then(|o| match o {
                Object::String(bytes, _) => Some(bytes.len()),
                _ => None,
            })
            .unwrap_or(0);

        let byte_range_is_structurally_sound = byte_range
            .map(|br| is_byte_range_structurally_sound(&br, pdf_bytes.len()))
            .unwrap_or(false);

        signatures.push(SignatureInfo {
            sub_filter,
            reason,
            name,
            signing_time,
            byte_range,
            contents_len_bytes,
            byte_range_is_structurally_sound,
        });
    }

    Ok(signatures)
}

fn string_field(dict: &Dictionary, key: &[u8]) -> Option<String> {
    dict.get(key)
        .and_then(|o| o.as_str())
        .ok()
        .map(|bytes| String::from_utf8_lossy(bytes).into_owned())
}

fn parse_byte_range(array: &[Object]) -> Option<[i64; 4]> {
    if array.len() != 4 {
        return None;
    }
    let mut out = [0i64; 4];
    for (i, value) in array.iter().enumerate() {
        out[i] = value.as_i64().ok()?;
    }
    Some(out)
}

/// A well-formed signature's `/ByteRange` is `[0, a, b, len-b]`: it
/// starts at the beginning of the file, covers everything up to where
/// the `/Contents` placeholder begins, skips the placeholder, then covers
/// everything after it through the end of the file. That's the shape
/// this checks — not that the covered bytes hash/verify to anything (see
/// module doc).
fn is_byte_range_structurally_sound(byte_range: &[i64; 4], file_len: usize) -> bool {
    let [start1, len1, start2, len2] = *byte_range;
    if start1 < 0 || len1 < 0 || start2 < 0 || len2 < 0 {
        return false;
    }
    let Ok(file_len) = i64::try_from(file_len) else {
        return false;
    };
    start1 == 0 && start1 + len1 <= start2 && start2 + len2 == file_len
}

#[cfg(test)]
mod tests {
    use super::*;
    use lopdf::{dictionary, Object};

    fn pdf_with_signature(byte_range: Option<[i64; 4]>, contents_hex_len: usize) -> Vec<u8> {
        let mut doc = lopdf::Document::with_version("1.7");
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

        let mut sig_dict = dictionary! {
            "Type" => "Sig",
            "SubFilter" => "adbe.pkcs7.detached",
            "Reason" => Object::string_literal("Approval"),
            "Name" => Object::string_literal("Ada Lovelace"),
            "M" => Object::string_literal("D:20260801120000+00'00'"),
            "Contents" => Object::String(vec![0u8; contents_hex_len], lopdf::StringFormat::Hexadecimal),
        };
        if let Some(br) = byte_range {
            sig_dict.set(
                "ByteRange",
                br.iter().map(|&n| n.into()).collect::<Vec<Object>>(),
            );
        }
        doc.add_object(dictionary! {
            "Type" => "Sig",
        }); // placeholder to bump ids, harmless
        let sig_id = doc.add_object(Object::Dictionary(sig_dict));
        let catalog_id = doc.add_object(dictionary! {
            "Type" => "Catalog",
            "Pages" => pages_id,
            "SigRef" => sig_id, // keep the signature reachable, not otherwise meaningful
        });
        doc.trailer.set("Root", catalog_id);

        let mut bytes = Vec::new();
        doc.save_to(&mut bytes).unwrap();
        bytes
    }

    #[test]
    fn finds_a_signature_and_reports_its_declared_fields() {
        let bytes = pdf_with_signature(Some([0, 100, 200, 50]), 32);
        let signatures = find_signatures(&bytes).expect("should parse");
        // Two Sig-typed objects exist (the placeholder + the real one);
        // both are found — find_signatures doesn't try to guess which
        // one is "the real" AcroForm-registered signature, only that a
        // /Type /Sig dictionary exists, which is deliberately permissive.
        let with_reason: Vec<_> = signatures.iter().filter(|s| s.reason.is_some()).collect();
        assert_eq!(with_reason.len(), 1);
        let sig = with_reason[0];
        assert_eq!(sig.sub_filter.as_deref(), Some("adbe.pkcs7.detached"));
        assert_eq!(sig.reason.as_deref(), Some("Approval"));
        assert_eq!(sig.name.as_deref(), Some("Ada Lovelace"));
        assert_eq!(sig.signing_time.as_deref(), Some("D:20260801120000+00'00'"));
        assert_eq!(sig.contents_len_bytes, 32);
    }

    #[test]
    fn no_signatures_returns_an_empty_list_not_an_error() {
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

        let signatures = find_signatures(&bytes).expect("should parse");
        assert!(signatures.is_empty());
    }

    #[test]
    fn malformed_bytes_return_an_error_not_a_panic() {
        assert!(find_signatures(b"not a pdf at all").is_err());
    }

    #[test]
    fn byte_range_structural_check_accepts_a_well_formed_range() {
        // [0, 100, 200, 50] on a 250-byte file: covers 0..100, skips
        // 100..200 (the Contents placeholder), covers 200..250 — exactly
        // the whole file minus the placeholder.
        assert!(is_byte_range_structurally_sound(&[0, 100, 200, 50], 250));
    }

    #[test]
    fn byte_range_structural_check_rejects_a_gap_or_overlap() {
        // A gap between the two covered ranges (100..200 vs declared
        // second-range start of 250) is not what a real signature's
        // ByteRange looks like.
        assert!(!is_byte_range_structurally_sound(&[0, 100, 250, 50], 250));
        // Doesn't start at 0.
        assert!(!is_byte_range_structurally_sound(&[10, 90, 200, 50], 250));
        // Second range runs past the actual file length.
        assert!(!is_byte_range_structurally_sound(&[0, 100, 200, 100], 250));
        // Negative values (malformed input) must not panic via overflow.
        assert!(!is_byte_range_structurally_sound(&[0, -1, 200, 50], 250));
    }

    #[test]
    fn signature_with_missing_byte_range_is_reported_as_structurally_unsound() {
        let bytes = pdf_with_signature(None, 32);
        let signatures = find_signatures(&bytes).expect("should parse");
        let sig = signatures.iter().find(|s| s.reason.is_some()).unwrap();
        assert!(sig.byte_range.is_none());
        assert!(!sig.byte_range_is_structurally_sound);
    }
}

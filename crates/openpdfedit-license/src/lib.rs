//! Offline license verification scaffold.
//!
//! Per PLAN.md decision log (2026-08-01, items #2/#3), feature tiering and
//! pricing are deferred: every feature ships ungated while the product is
//! built out. This crate exists so that turning gating on later is a flip,
//! not a refactor — nothing in the desktop app calls `verify` to block a
//! feature yet.
//!
//! A license is a small signed record issued by the OpenApps platform
//! (Ed25519, matching the platform's existing JWKS key discipline — see
//! `docs/architecture.md` in the platform workspace). Verification is
//! entirely local: no network call is required to keep a purchased license
//! working, which is the "yours forever" promise in PLAN.md §2.

use ed25519_dalek::{Signature, Signer, SigningKey, Verifier, VerifyingKey};
use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum LicenseError {
    #[error("malformed license payload: {0}")]
    Malformed(#[from] serde_json::Error),
    #[error("invalid signature encoding")]
    BadSignatureEncoding,
    #[error("invalid public key encoding")]
    BadKeyEncoding,
    #[error("signature does not verify against the given key")]
    SignatureInvalid,
}

/// The signed fields of a license, as issued by the OpenApps platform.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct LicensePayload {
    pub license_id: String,
    /// Ties the license to a purchase receipt rather than a user account,
    /// so "no account required" extends to paying customers (PLAN.md §4).
    pub purchase_id: String,
    pub tier: String,
    /// RFC 3339 timestamps. Kept as strings so this crate has no opinion
    /// about clock sources; the caller decides how to compare `updates_until`
    /// against "now".
    pub issued_at: String,
    pub updates_until: String,
}

/// A license file: the payload plus its detached signature, both stored
/// on disk as the fields below (typically serialized as one JSON document).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LicenseFile {
    pub payload: LicensePayload,
    /// Base64 (standard, no padding stripped) Ed25519 signature over the
    /// canonical JSON encoding of `payload`.
    pub signature: String,
}

/// Verifies a license file against the platform's public key, returning
/// the payload on success. Does not interpret `updates_until` — that's a
/// policy decision for the caller once tiering (PLAN.md §11 item 2) lands.
pub fn verify(
    license: &LicenseFile,
    public_key_bytes: &[u8; 32],
) -> Result<LicensePayload, LicenseError> {
    let verifying_key =
        VerifyingKey::from_bytes(public_key_bytes).map_err(|_| LicenseError::BadKeyEncoding)?;

    let sig_bytes = base64_decode(&license.signature).ok_or(LicenseError::BadSignatureEncoding)?;
    let sig_array: [u8; 64] = sig_bytes
        .as_slice()
        .try_into()
        .map_err(|_| LicenseError::BadSignatureEncoding)?;
    let signature = Signature::from_bytes(&sig_array);

    let canonical = serde_json::to_vec(&license.payload)?;
    verifying_key
        .verify(&canonical, &signature)
        .map_err(|_| LicenseError::SignatureInvalid)?;

    Ok(license.payload.clone())
}

/// Signs `payload` with `signing_key_bytes`, producing a [`LicenseFile`].
///
/// **This is the OpenApps platform's job in production** — the issuing
/// side, run when a purchase completes, holding the private key this app
/// never sees. It's exposed here purely for local dev/test tooling (e.g.
/// minting a throwaway license to exercise the import/verify flow before
/// any real checkout integration exists) — see this crate's module doc
/// for why that integration isn't built yet.
pub fn sign(payload: LicensePayload, signing_key_bytes: &[u8; 32]) -> LicenseFile {
    let signing_key = SigningKey::from_bytes(signing_key_bytes);
    let canonical = serde_json::to_vec(&payload).expect("LicensePayload always serializes");
    let signature = signing_key.sign(&canonical);
    LicenseFile {
        payload,
        signature: base64_encode(&signature.to_bytes()),
    }
}

fn base64_encode(bytes: &[u8]) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::new();
    for chunk in bytes.chunks(3) {
        let b0 = chunk[0] as u32;
        let b1 = *chunk.get(1).unwrap_or(&0) as u32;
        let b2 = *chunk.get(2).unwrap_or(&0) as u32;
        let n = (b0 << 16) | (b1 << 8) | b2;
        out.push(ALPHABET[(n >> 18 & 0x3F) as usize] as char);
        out.push(ALPHABET[(n >> 12 & 0x3F) as usize] as char);
        out.push(if chunk.len() > 1 {
            ALPHABET[(n >> 6 & 0x3F) as usize] as char
        } else {
            '='
        });
        out.push(if chunk.len() > 2 {
            ALPHABET[(n & 0x3F) as usize] as char
        } else {
            '='
        });
    }
    out
}

// Minimal base64 (standard alphabet) decoder so this crate doesn't pull in
// a whole base64 dependency for one call site. Swap for the `base64` crate
// if a second call site appears.
fn base64_decode(input: &str) -> Option<Vec<u8>> {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut table = [255u8; 256];
    for (i, &c) in ALPHABET.iter().enumerate() {
        table[c as usize] = i as u8;
    }

    let input = input.trim_end_matches('=');
    let mut out = Vec::with_capacity(input.len() * 3 / 4 + 3);
    let mut buf = 0u32;
    let mut bits = 0u32;
    for c in input.bytes() {
        let val = table[c as usize];
        if val == 255 {
            return None;
        }
        buf = (buf << 6) | val as u32;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((buf >> bits) as u8);
        }
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn signed_test_license() -> (LicenseFile, [u8; 32]) {
        let key_bytes = [7u8; 32];
        let payload = LicensePayload {
            license_id: "lic_test_0001".into(),
            purchase_id: "purchase_test_0001".into(),
            tier: "pro".into(),
            issued_at: "2026-08-01T00:00:00Z".into(),
            updates_until: "2028-08-01T00:00:00Z".into(),
        };
        let license = sign(payload, &key_bytes);
        let public_key = SigningKey::from_bytes(&key_bytes)
            .verifying_key()
            .to_bytes();
        (license, public_key)
    }

    #[test]
    fn valid_signature_verifies() {
        let (license, public_key) = signed_test_license();
        let payload = verify(&license, &public_key).expect("signature should verify");
        assert_eq!(payload.tier, "pro");
    }

    #[test]
    fn tampered_payload_fails_verification() {
        let (mut license, public_key) = signed_test_license();
        license.payload.tier = "premium".into();
        let result = verify(&license, &public_key);
        assert!(matches!(result, Err(LicenseError::SignatureInvalid)));
    }

    #[test]
    fn wrong_key_fails_verification() {
        let (license, _) = signed_test_license();
        let wrong_key = SigningKey::from_bytes(&[9u8; 32])
            .verifying_key()
            .to_bytes();
        let result = verify(&license, &wrong_key);
        assert!(matches!(result, Err(LicenseError::SignatureInvalid)));
    }

    #[test]
    fn sign_then_verify_round_trips_with_a_freshly_generated_keypair() {
        // Distinct from `signed_test_license`'s fixed seed key: a real
        // random keypair, generated the same way an actual issuing key
        // would be, exercising `sign()` and `verify()` as a genuine pair
        // rather than only against one fixed known-good fixture.
        let key_bytes: [u8; 32] = rand::random();
        let signing_key = SigningKey::from_bytes(&key_bytes);
        let payload = LicensePayload {
            license_id: "lic_random_0001".into(),
            purchase_id: "purchase_random_0001".into(),
            tier: "free".into(),
            issued_at: "2026-08-01T00:00:00Z".into(),
            updates_until: "2027-08-01T00:00:00Z".into(),
        };
        let license = sign(payload.clone(), &key_bytes);
        let verified = verify(&license, &signing_key.verifying_key().to_bytes())
            .expect("a signature from a freshly generated key must verify");
        assert_eq!(verified, payload);
    }

    #[test]
    fn json_round_trip_through_serialization_preserves_verifiability() {
        // A LicenseFile is meant to be persisted to disk as JSON and
        // re-parsed on the next app launch — confirm that round trip
        // doesn't somehow invalidate the signature (e.g. via field
        // reordering changing the canonical bytes hashed).
        let (license, public_key) = signed_test_license();
        let json = serde_json::to_string(&license).expect("should serialize");
        let reloaded: LicenseFile = serde_json::from_str(&json).expect("should deserialize");
        let verified =
            verify(&reloaded, &public_key).expect("should still verify after JSON round trip");
        assert_eq!(verified.tier, "pro");
    }
}

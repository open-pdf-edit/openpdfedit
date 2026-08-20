//! Dev-only tool: mints a license file signed by a **fixed, publicly
//! known, throwaway keypair** — not the real OpenApps platform key, which
//! doesn't exist yet (see this crate's module doc). Its sole purpose is
//! letting the desktop app's license import/verify flow be exercised
//! end-to-end before any real checkout integration lands. The
//! corresponding public key is hardcoded in
//! `apps/desktop/src-tauri/src/license.rs` as `DEV_OPENAPPS_PUBLIC_KEY`,
//! clearly labeled the same way.
//!
//! Usage: `cargo run -p openpdfedit-license --example mint_dev_license -- <output-path> [tier]`
//! (`tier` defaults to `"pro"`.)

use openpdfedit_license::{sign, LicensePayload};

/// Fixed dev seed — NEVER a real private key. Anyone can derive the
/// matching public key from this, which is exactly why it must never be
/// used once real licenses are issued by an actual platform key.
const DEV_SIGNING_KEY_SEED: [u8; 32] = [42u8; 32];

fn main() {
    let mut args = std::env::args().skip(1);
    let output_path = args.next().unwrap_or_else(|| {
        eprintln!("usage: mint_dev_license <output-path> [tier]");
        std::process::exit(1);
    });
    let tier = args.next().unwrap_or_else(|| "pro".to_string());

    let payload = LicensePayload {
        license_id: format!("lic_dev_{}", std::process::id()),
        purchase_id: format!("purchase_dev_{}", std::process::id()),
        tier,
        issued_at: "2026-08-01T00:00:00Z".to_string(),
        updates_until: "2028-08-01T00:00:00Z".to_string(),
    };

    let license = sign(payload, &DEV_SIGNING_KEY_SEED);
    let json = serde_json::to_string_pretty(&license).expect("license file always serializes");
    std::fs::write(&output_path, json).expect("should write license file");
    eprintln!("wrote dev license to {output_path}");
}

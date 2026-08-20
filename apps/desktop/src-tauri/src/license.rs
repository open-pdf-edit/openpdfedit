//! License import/status commands (PLAN.md M6: "license plumbing
//! scaffold, activation deferred"). Per the decision log, no feature is
//! gated by license status yet — this only lets the app import, persist,
//! and display a license, so gating can be flipped on later without
//! rework.
//!
//! `DEV_OPENAPPS_PUBLIC_KEY` is a **placeholder**: the real OpenApps
//! platform checkout/entitlement-issuance integration (PLAN.md's other
//! M6 bullet) isn't built — there's no live purchasing backend to
//! integrate with from this environment, and fabricating one would mean
//! shipping a fake trust root. The matching private key
//! (`openpdfedit-license`'s `examples/mint_dev_license.rs`) is
//! intentionally public and clearly labeled dev-only; swapping in a real
//! platform-issued public key is the only change needed once that
//! integration exists.

use openpdfedit_license::{LicenseFile, LicensePayload};
use serde::Serialize;
use tauri::{AppHandle, Manager};

use crate::CommandError;

/// See this module's doc — a throwaway keypair, not a real trust root.
const DEV_OPENAPPS_PUBLIC_KEY: [u8; 32] = [
    25, 127, 107, 35, 225, 108, 133, 50, 198, 171, 200, 56, 250, 205, 94, 167, 137, 190, 12, 118,
    178, 146, 3, 52, 3, 155, 250, 139, 61, 54, 141, 97,
];

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LicenseStatusDto {
    license_id: String,
    tier: String,
    issued_at: String,
    updates_until: String,
}

impl From<LicensePayload> for LicenseStatusDto {
    fn from(p: LicensePayload) -> Self {
        LicenseStatusDto {
            license_id: p.license_id,
            tier: p.tier,
            issued_at: p.issued_at,
            updates_until: p.updates_until,
        }
    }
}

fn persisted_license_path(app: &AppHandle) -> Result<std::path::PathBuf, CommandError> {
    let dir = app
        .path()
        .app_data_dir()
        .map_err(|e| CommandError::Io(e.to_string()))?;
    std::fs::create_dir_all(&dir)?;
    Ok(dir.join("license.json"))
}

/// Imports a license file from `source_path`, verifies it, and — only on
/// successful verification — persists a copy into the app's own data
/// directory so it survives restarts without depending on the original
/// file staying where the user picked it from.
#[tauri::command]
pub fn import_license_cmd(
    app: AppHandle,
    source_path: String,
) -> Result<LicenseStatusDto, CommandError> {
    let bytes = std::fs::read(&source_path)?;
    let license: LicenseFile =
        serde_json::from_slice(&bytes).map_err(|e| CommandError::Doc(e.to_string()))?;
    let payload = openpdfedit_license::verify(&license, &DEV_OPENAPPS_PUBLIC_KEY)
        .map_err(|e| CommandError::Doc(e.to_string()))?;

    let dest = persisted_license_path(&app)?;
    std::fs::write(&dest, &bytes)?;

    Ok(payload.into())
}

/// Returns the currently persisted license's status, if any and if it
/// still verifies (re-verified on every call rather than trusted from a
/// prior import — cheap, and correctly reflects a license file that
/// somehow got corrupted or hand-edited on disk since import).
#[tauri::command]
pub fn get_license_status_cmd(app: AppHandle) -> Result<Option<LicenseStatusDto>, CommandError> {
    let path = persisted_license_path(&app)?;
    if !path.exists() {
        return Ok(None);
    }
    let bytes = std::fs::read(&path)?;
    let license: LicenseFile =
        serde_json::from_slice(&bytes).map_err(|e| CommandError::Doc(e.to_string()))?;
    match openpdfedit_license::verify(&license, &DEV_OPENAPPS_PUBLIC_KEY) {
        Ok(payload) => Ok(Some(payload.into())),
        // A present-but-invalid license file (corrupted, tampered, or
        // signed by a key that's no longer trusted) is reported as "no
        // license," not an error — the free tier is always a safe
        // fallback, and there's nothing actionable a caller can do with
        // "your on-disk license file is broken" beyond re-importing.
        Err(_) => Ok(None),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dev_signed_license_bytes(tier: &str) -> Vec<u8> {
        const DEV_SIGNING_KEY_SEED: [u8; 32] = [42u8; 32];
        let payload = LicensePayload {
            license_id: "lic_test".into(),
            purchase_id: "purchase_test".into(),
            tier: tier.into(),
            issued_at: "2026-08-01T00:00:00Z".into(),
            updates_until: "2028-08-01T00:00:00Z".into(),
        };
        let license = openpdfedit_license::sign(payload, &DEV_SIGNING_KEY_SEED);
        serde_json::to_vec(&license).unwrap()
    }

    #[test]
    fn dev_public_key_matches_the_dev_signing_key_used_for_test_fixtures() {
        // If these ever drift out of sync (e.g. someone changes the seed
        // in mint_dev_license.rs without updating this constant), every
        // license-import test would fail for a confusing reason — this
        // test exists to fail with an obvious one instead.
        let bytes = dev_signed_license_bytes("pro");
        let license: LicenseFile = serde_json::from_slice(&bytes).unwrap();
        assert!(openpdfedit_license::verify(&license, &DEV_OPENAPPS_PUBLIC_KEY).is_ok());
    }

    #[test]
    fn tampered_license_bytes_fail_verification() {
        let mut license: LicenseFile =
            serde_json::from_slice(&dev_signed_license_bytes("pro")).unwrap();
        license.payload.tier = "free".into();
        assert!(openpdfedit_license::verify(&license, &DEV_OPENAPPS_PUBLIC_KEY).is_err());
    }
}

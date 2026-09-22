//! What only the Mac App Store build does: sell credits through StoreKit,
//! and reopen Recent files inside the sandbox.
//!
//! Both are Swift, in `appstore-native/`, linked only when the `appstore`
//! feature is on (see build.rs). The commands exist in every build so the
//! invoke handler is the same everywhere; outside the store build they
//! answer that StoreKit is not here, and the page never asks, because it
//! only installs its StoreKit bridge in that build.

#[cfg(all(feature = "appstore", target_os = "macos"))]
mod native {
    use std::collections::HashMap;
    use std::ffi::{c_char, CStr, CString};
    use std::path::PathBuf;
    use std::sync::{Mutex, OnceLock};

    use tauri::{AppHandle, Emitter, Manager};

    extern "C" {
        fn oa_sk_start(on_receipt: extern "C" fn(*const c_char));
        fn oa_sk_products() -> *mut c_char;
        fn oa_sk_purchase(product_id: *const c_char) -> *mut c_char;
        fn oa_sk_outstanding() -> *mut c_char;
        fn oa_sk_finish(transaction_id: *const c_char) -> *mut c_char;
        fn oa_sk_free(pointer: *mut c_char);
        fn oa_bm_create(path: *const c_char) -> *mut c_char;
        fn oa_bm_resolve(bookmark: *const c_char) -> *mut c_char;
    }

    /// Copies a string the Swift side returned, and frees it there.
    fn take(pointer: *mut c_char) -> String {
        let value = unsafe { CStr::from_ptr(pointer) }.to_string_lossy().into_owned();
        unsafe { oa_sk_free(pointer) };
        value
    }

    fn c(value: &str) -> CString {
        CString::new(value).unwrap_or_default()
    }

    // --- StoreKit ------------------------------------------------------

    static APP: OnceLock<AppHandle> = OnceLock::new();

    /// A transaction StoreKit delivered on its own — an interrupted
    /// purchase completing, an Ask to Buy approval — handed to the page
    /// as the same "receipt" the iOS shell sends.
    extern "C" fn on_receipt(json: *const c_char) {
        let receipt = unsafe { CStr::from_ptr(json) }.to_string_lossy().into_owned();
        if let (Some(app), Ok(value)) = (APP.get(), serde_json::from_str::<serde_json::Value>(&receipt)) {
            let _ = app.emit("storekit-receipt", value);
        }
    }

    pub fn start(app: &AppHandle) {
        if APP.set(app.clone()).is_ok() {
            unsafe { oa_sk_start(on_receipt) };
        }
    }

    /// Runs a blocking StoreKit call off the main thread — which is also
    /// what leaves the main thread free to show the purchase sheet — and
    /// turns `{"error": …}` into an error.
    async fn call(work: impl FnOnce() -> String + Send + 'static) -> Result<serde_json::Value, String> {
        let raw = tauri::async_runtime::spawn_blocking(work).await.map_err(|e| e.to_string())?;
        let value: serde_json::Value = serde_json::from_str(&raw).map_err(|e| e.to_string())?;
        match value.get("error").and_then(|e| e.as_str()) {
            Some(message) => Err(message.to_string()),
            None => Ok(value),
        }
    }

    pub async fn products() -> Result<serde_json::Value, String> {
        call(|| take(unsafe { oa_sk_products() })).await
    }

    pub async fn purchase(product_id: String) -> Result<serde_json::Value, String> {
        call(move || take(unsafe { oa_sk_purchase(c(&product_id).as_ptr()) })).await
    }

    pub async fn outstanding() -> Result<serde_json::Value, String> {
        call(|| take(unsafe { oa_sk_outstanding() })).await
    }

    pub async fn finish(transaction_id: String) -> Result<serde_json::Value, String> {
        call(move || take(unsafe { oa_sk_finish(c(&transaction_id).as_ptr()) })).await
    }

    // --- Recent files in the sandbox -------------------------------------

    /// path → security-scoped bookmark, kept in the app's container.
    static BOOKMARKS: Mutex<Option<HashMap<String, String>>> = Mutex::new(None);

    fn store_path(app: &AppHandle) -> Option<PathBuf> {
        app.path().app_data_dir().ok().map(|dir| dir.join("bookmarks.json"))
    }

    fn with_bookmarks<T>(app: &AppHandle, f: impl FnOnce(&mut HashMap<String, String>) -> T) -> T {
        let mut guard = BOOKMARKS.lock().expect("bookmarks lock poisoned");
        let map = guard.get_or_insert_with(|| {
            store_path(app)
                .and_then(|p| std::fs::read(p).ok())
                .and_then(|b| serde_json::from_slice(&b).ok())
                .unwrap_or_default()
        });
        f(map)
    }

    fn save(app: &AppHandle, map: &HashMap<String, String>) {
        if let Some(path) = store_path(app) {
            let _ = std::fs::create_dir_all(path.parent().unwrap());
            let _ = std::fs::write(path, serde_json::to_vec(map).unwrap_or_default());
        }
    }

    /// Records a bookmark for a file the app has access to right now —
    /// just opened from a panel, or just saved to a new place.
    pub fn remember(app: &AppHandle, path: &str) {
        let bookmark = take(unsafe { oa_bm_create(c(path).as_ptr()) });
        if bookmark.starts_with('{') {
            return; // `{"error": …}`: nothing to keep, and nothing to break.
        }
        with_bookmarks(app, |map| {
            map.insert(path.to_string(), bookmark);
            save(app, map);
        });
    }

    /// Before opening a path the app may no longer be allowed to read —
    /// a recent, after a relaunch — asks macOS for access again through
    /// the bookmark kept for it. Returns the path to open, which differs
    /// when the file has since been moved on the same volume.
    pub fn regain(app: &AppHandle, path: &str) -> String {
        let Some(bookmark) = with_bookmarks(app, |map| map.get(path).cloned()) else {
            return path.to_string();
        };
        let resolved = take(unsafe { oa_bm_resolve(c(&bookmark).as_ptr()) });
        let Ok(value) = serde_json::from_str::<serde_json::Value>(&resolved) else {
            return path.to_string();
        };
        let Some(current) = value.get("path").and_then(|p| p.as_str()).map(str::to_string) else {
            return path.to_string();
        };
        if value.get("stale").and_then(|s| s.as_bool()) == Some(true) || current != path {
            // Moved: refresh the bookmark while access is live again.
            with_bookmarks(app, |map| {
                map.remove(path);
            });
            remember(app, &current);
        }
        current
    }
}

// --- the commands, in every build ------------------------------------------

#[cfg(all(feature = "appstore", target_os = "macos"))]
pub use native::{regain, remember, start};

/// No sandbox outside the store build, so nothing to regain or remember.
#[cfg(not(all(feature = "appstore", target_os = "macos")))]
pub fn regain(_app: &tauri::AppHandle, path: &str) -> String {
    path.to_string()
}
#[cfg(not(all(feature = "appstore", target_os = "macos")))]
pub fn remember(_app: &tauri::AppHandle, _path: &str) {}
#[cfg(not(all(feature = "appstore", target_os = "macos")))]
pub fn start(_app: &tauri::AppHandle) {}

#[cfg(not(all(feature = "appstore", target_os = "macos")))]
const ABSENT: &str = "StoreKit is only in the Mac App Store build";

#[tauri::command]
pub async fn storekit_products() -> Result<serde_json::Value, String> {
    #[cfg(all(feature = "appstore", target_os = "macos"))]
    return native::products().await;
    #[cfg(not(all(feature = "appstore", target_os = "macos")))]
    Err(ABSENT.into())
}

#[tauri::command]
pub async fn storekit_purchase(product_id: String) -> Result<serde_json::Value, String> {
    #[cfg(all(feature = "appstore", target_os = "macos"))]
    return native::purchase(product_id).await;
    #[cfg(not(all(feature = "appstore", target_os = "macos")))]
    {
        let _ = product_id;
        Err(ABSENT.into())
    }
}

#[tauri::command]
pub async fn storekit_outstanding() -> Result<serde_json::Value, String> {
    #[cfg(all(feature = "appstore", target_os = "macos"))]
    return native::outstanding().await;
    #[cfg(not(all(feature = "appstore", target_os = "macos")))]
    Err(ABSENT.into())
}

#[tauri::command]
pub async fn storekit_finish(transaction_id: String) -> Result<serde_json::Value, String> {
    #[cfg(all(feature = "appstore", target_os = "macos"))]
    return native::finish(transaction_id).await;
    #[cfg(not(all(feature = "appstore", target_os = "macos")))]
    {
        let _ = transaction_id;
        Err(ABSENT.into())
    }
}

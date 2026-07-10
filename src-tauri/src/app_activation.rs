//! macOS app-activation observer. Fires the per-app auto-mode rule whenever the
//! frontmost application changes, so the active cleanup mode tracks focus rather
//! than only updating at dictation start. macOS only; other platforms rely on
//! the dictation-start path in [`crate::per_app_mode`].
//!
//! The observer is registered against `NSWorkspace`'s notification center, whose
//! callbacks are delivered on the main thread — so the handler reads the
//! frontmost app and writes settings inline without a cross-thread hop. The
//! returned observer token is leaked so it lives for the process lifetime;
//! dropping it would deregister the observer (and `Retained` is neither `Send`
//! nor `Sync`, so it cannot be parked in a global or Tauri state anyway).

use block2::RcBlock;
use objc2::rc::Retained;
use objc2::runtime::AnyObject;
use objc2::{class, msg_send};
use objc2_foundation::NSString;
use tauri::AppHandle;

/// Reads the current frontmost application's bundle id via `NSWorkspace`.
/// Caller must be on the main thread (the observer callback is).
unsafe fn frontmost_bundle_id() -> Option<String> {
    let workspace: *mut AnyObject = msg_send![class!(NSWorkspace), sharedWorkspace];
    if workspace.is_null() {
        return None;
    }
    let running_app: *mut AnyObject = msg_send![workspace, frontmostApplication];
    if running_app.is_null() {
        return None;
    }
    let bundle: *mut NSString = msg_send![running_app, bundleIdentifier];
    if bundle.is_null() {
        return None;
    }
    Some((*bundle).to_string())
}

/// Registers the activation observer. The block and its captured `AppHandle`
/// are copied by the notification center; the returned token is leaked so the
/// observer stays registered for the process lifetime.
pub fn register(app: &AppHandle) {
    let app_handle = app.clone();

    // Delivered on the main thread; the notification argument is unused because
    // we read the (now-)frontmost app directly.
    let block = RcBlock::new(move |_notification: *mut AnyObject| {
        let bundle_id = unsafe { frontmost_bundle_id() };
        if let Some(bundle_id) = bundle_id {
            crate::per_app_mode::on_app_switched(&app_handle, &bundle_id);
        }
    });

    unsafe {
        let workspace: *mut AnyObject = msg_send![class!(NSWorkspace), sharedWorkspace];
        if workspace.is_null() {
            log::warn!("app-activation observer: NSWorkspace unavailable");
            return;
        }
        let center: *mut AnyObject = msg_send![workspace, notificationCenter];
        if center.is_null() {
            log::warn!("app-activation observer: workspace notificationCenter unavailable");
            return;
        }

        let name = NSString::from_str("NSWorkspaceDidActivateApplicationNotification");
        let token: Retained<AnyObject> = msg_send![
            center,
            addObserverForName: &*name,
            object: std::ptr::null::<AnyObject>(),
            queue: std::ptr::null::<AnyObject>(),
            usingBlock: &*block,
        ];
        // Keep the observer alive for the process lifetime. `Retained` is not
        // `Send`/`Sync`, so it cannot be stored globally; leaking is the
        // simplest way to pin its lifetime to the app.
        std::mem::forget(token);
    }

    log::debug!("app-activation observer registered");
}

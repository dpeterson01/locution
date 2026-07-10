//! Per-app auto mode rules (opt-in, default off). A rule sets the active
//! **mode** — modes are complete goal recipes bundling edit-strength, tone,
//! format, and model tier, so mapping an app to a mode is the whole point.
//!
//! Manual mode switches (the overlay Modes popover, tray, and the cycle-mode
//! hotkey; the Settings-dropdown is a mode editor now and no longer touches the
//! active mode) are **transient** while auto mode is on: the pick sticks for the
//! current app only and is cleared the moment the frontmost app changes, at
//! which point the per-app rule takes over again. With auto mode off there is no
//! rule to defer to, so a manual pick persists (see
//! `set_post_process_selected_prompt`).

use once_cell::sync::Lazy;
use std::sync::Mutex;
use tauri::{AppHandle, Emitter};

use crate::settings;

/// Transient manual override slot. `Some(id)` means the user picked a mode for
/// the current app; it owns `post_process_selected_prompt_id` until the next
/// app switch clears it. `None` means the base per-app rule owns the selection.
static TRANSIENT_MODE_OVERRIDE: Lazy<Mutex<Option<String>>> = Lazy::new(|| Mutex::new(None));

fn transient_override() -> Option<String> {
    TRANSIENT_MODE_OVERRIDE
        .lock()
        .expect("transient override mutex poisoned")
        .clone()
}

/// Clears the transient manual override so the per-app rule resumes ownership.
pub fn clear_transient_override() {
    *TRANSIENT_MODE_OVERRIDE
        .lock()
        .expect("transient override mutex poisoned") = None;
}

/// Records a transient manual override for the current app and writes it as the
/// selected mode (syncing overlay + tray). Dropped on the next app switch.
pub fn set_transient_override(app: &AppHandle, id: &str) {
    *TRANSIENT_MODE_OVERRIDE
        .lock()
        .expect("transient override mutex poisoned") = Some(id.to_string());
    let mut settings = settings::get_settings(app);
    settings.post_process_selected_prompt_id = Some(id.to_string());
    settings::write_settings(app, settings);
    emit_and_sync(app);
}

fn emit_and_sync(app: &AppHandle) {
    let _ = app.emit(
        "settings-changed",
        serde_json::json!({ "setting": "post_process_selected_prompt_id" }),
    );
    crate::tray::update_tray_menu(app, &crate::tray::current_tray_state(app), None);
}

/// Resolves the base (rule-driven) mode id for a bundle id: a mapped mode if one
/// exists, otherwise the durable default. Returns `None` when the resolved mode
/// is empty or no longer exists — a mapped mode could have been deleted since
/// the rule was created, and unlike a missing Style, a missing mode means no
/// cleanup at all, so validate before applying.
fn resolve_base_mode(settings: &settings::AppSettings, bundle_id: &str) -> Option<String> {
    let effective = settings
        .per_app_mode_map
        .get(bundle_id)
        .cloned()
        .unwrap_or_else(|| settings.default_mode_id.clone().unwrap_or_default());
    if effective.is_empty() {
        return None;
    }
    if !settings
        .post_process_prompts
        .iter()
        .any(|p| p.id == effective)
    {
        return None;
    }
    Some(effective)
}

/// Applies the base per-app mode for a known bundle id through the same settings
/// write + event a manual change would use, so the overlay/tray stay in sync.
/// No-ops unless auto mode is on, the map is non-empty, and the resolved mode
/// differs from the current selection. Unmapped apps resolve to
/// `default_mode_id`, not whatever the last mapped app left behind.
fn apply_base_for_bundle(app: &AppHandle, bundle_id: &str) {
    let settings = settings::get_settings(app);
    if !settings.per_app_auto_mode_enabled || settings.per_app_mode_map.is_empty() {
        return;
    }
    let Some(effective) = resolve_base_mode(&settings, bundle_id) else {
        return;
    };
    if Some(&effective) == settings.post_process_selected_prompt_id.as_ref() {
        return;
    }

    let mut updated = settings;
    updated.post_process_selected_prompt_id = Some(effective);
    settings::write_settings(app, updated);
    emit_and_sync(app);
}

/// App-switch handler — called by the macOS activation observer on the main
/// thread with the newly-frontmost app's bundle id. Ignores activations of our
/// own windows (so the mode keeps reflecting the last external app), clears any
/// transient manual override, then applies the base per-app rule.
pub fn on_app_switched(app: &AppHandle, bundle_id: &str) {
    if bundle_id == app.config().identifier {
        return;
    }
    clear_transient_override();
    apply_base_for_bundle(app, bundle_id);
}

/// Dictation-start handler (all platforms; runs off the main thread). Honors an
/// active transient override; otherwise applies the base rule for the frontmost
/// app. On macOS the activation observer normally keeps the selection current,
/// but Windows and the very first dictation rely on this path.
pub fn apply_rule_for_frontmost_app(app: &AppHandle) {
    let settings = settings::get_settings(app);
    if !settings.per_app_auto_mode_enabled || settings.per_app_mode_map.is_empty() {
        return;
    }
    if transient_override().is_some() {
        return;
    }

    let Some((_, bundle_id)) = crate::context_capture::frontmost_app_info(app) else {
        return;
    };
    apply_base_for_bundle(app, &bundle_id);
}

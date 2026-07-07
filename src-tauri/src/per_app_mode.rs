//! Per-app auto mode rules (opt-in, default off). A rule sets the active
//! **mode** — modes are complete goal recipes bundling edit-strength, tone,
//! format, and model tier, so mapping an app to a mode is the whole point,
//! not a layered tone-only override the way the old Style axis was. A manual
//! mode switch (Settings dropdown, tray, overlay Modes popover, or the
//! cycle-mode hotkey — all of which converge on
//! `set_post_process_selected_prompt`) sticks for the rest of the app run so
//! a rule can't silently flip output underneath the user later.

use once_cell::sync::Lazy;
use std::sync::atomic::{AtomicBool, Ordering};
use tauri::{AppHandle, Emitter};

use crate::settings;

/// In-memory, process-lifetime flag — "the session" is this app run. Reset
/// only by relaunching the app, not by any settings toggle.
static MANUAL_MODE_OVERRIDE: Lazy<AtomicBool> = Lazy::new(|| AtomicBool::new(false));

/// Marks that the user manually picked a mode this run. Per-app rules stop
/// moving `post_process_selected_prompt_id` until relaunch.
pub fn mark_manual_mode_override() {
    MANUAL_MODE_OVERRIDE.store(true, Ordering::SeqCst);
}

/// Best-effort: look up the frontmost app's bundle id against the per-app
/// mode map and, if it selects a different mode than the one currently
/// active, apply it — through the exact same settings write + event a manual
/// change would use, so the overlay/tray stay in sync and nothing is silent.
/// No-ops unless the feature is enabled, the map is non-empty, and no manual
/// override is active yet. Unmapped apps resolve to `default_mode_id`, not
/// whatever the last mapped app left behind.
pub fn apply_rule_for_frontmost_app(app: &AppHandle) {
    let settings = settings::get_settings(app);
    if !settings.per_app_auto_mode_enabled || settings.per_app_mode_map.is_empty() {
        return;
    }
    if MANUAL_MODE_OVERRIDE.load(Ordering::SeqCst) {
        return;
    }

    let Some((_, bundle_id)) = crate::context_capture::frontmost_app_info(app) else {
        return;
    };

    let effective = settings
        .per_app_mode_map
        .get(&bundle_id)
        .cloned()
        .unwrap_or_else(|| settings.default_mode_id.clone().unwrap_or_default());

    if effective.is_empty() || Some(&effective) == settings.post_process_selected_prompt_id.as_ref()
    {
        return;
    }

    // A mapped mode could have been deleted since the rule was created —
    // unlike a missing Style (which just meant "no tone"), a missing mode
    // means no cleanup at all, so validate before applying.
    if !settings
        .post_process_prompts
        .iter()
        .any(|p| p.id == effective)
    {
        return;
    }

    let mut updated = settings;
    updated.post_process_selected_prompt_id = Some(effective);
    settings::write_settings(app, updated);
    let _ = app.emit(
        "settings-changed",
        serde_json::json!({ "setting": "post_process_selected_prompt_id" }),
    );
    crate::tray::update_tray_menu(app, &crate::tray::current_tray_state(app), None);
}

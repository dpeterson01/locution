//! First-run "move to Applications" nudge (macOS only).
//!
//! Encourages installing the app into `/Applications` so the auto-updater can
//! replace it in place and macOS keeps its Accessibility / Input Monitoring
//! grants stable across launches. Safe by design: a self-install is attempted
//! only on the happy path (running from a writable, non-translocated location
//! — a mounted DMG or `~/Downloads`). When Gatekeeper has app-translocated the
//! bundle, the real source can't be resolved without private APIs, so the app
//! shows a plain drag-me instruction instead and never attempts a move that
//! could strand itself.

#[cfg(not(target_os = "macos"))]
pub fn maybe_prompt(_app: &tauri::AppHandle) {}

#[cfg(target_os = "macos")]
pub fn maybe_prompt(app: &tauri::AppHandle) {
    use std::path::Path;
    use tauri_plugin_dialog::{DialogExt, MessageDialogButtons};

    const DEST: &str = "/Applications/Locution.app";

    let Ok(exe) = std::env::current_exe() else {
        return;
    };
    // `.../Locution.app/Contents/MacOS/<bin>` — the bundle is three parents up.
    let Some(bundle) = exe
        .parent()
        .and_then(Path::parent)
        .and_then(Path::parent)
        .map(Path::to_path_buf)
    else {
        return;
    };
    // In dev the binary runs bare (not inside an `.app`), so there is nothing
    // to install — bail unless we are actually inside a bundle.
    if bundle.extension().and_then(|e| e.to_str()) != Some("app") {
        return;
    }
    let bundle_str = bundle.to_string_lossy().to_string();

    // Already installed — nothing to do.
    if bundle_str.starts_with("/Applications/") {
        return;
    }

    let translocated = bundle_str.contains("/AppTranslocation/");
    let from_dmg = bundle_str.starts_with("/Volumes/");
    let from_downloads = std::env::var("HOME")
        .map(|home| bundle_str.starts_with(&format!("{home}/Downloads/")))
        .unwrap_or(false);

    // Only nudge from locations that mean "not installed yet". Any other path
    // (a dev build under `target/`, a deliberate custom location) is left alone
    // so power users are never nagged.
    if !(translocated || from_dmg || from_downloads) {
        return;
    }

    let app = app.clone();
    std::thread::spawn(move || {
        // Translocated: `current_exe()` points at a read-only random copy and
        // the real source can't be resolved without private SecTranslocate
        // APIs, so a self-move isn't safe. Instruct the user instead.
        if translocated {
            app.dialog()
                .message(
                    "Locution is running from a temporary copy. To finish installing, \
                     drag Locution into your Applications folder, then open it from there.",
                )
                .title("Move Locution to Applications")
                .blocking_show();
            return;
        }

        let confirmed = app
            .dialog()
            .message(
                "Locution works best from your Applications folder — that keeps automatic \
                 updates and macOS permissions working across launches. Move it there now?",
            )
            .title("Move to Applications?")
            .buttons(MessageDialogButtons::OkCancelCustom(
                "Move to Applications".into(),
                "Not Now".into(),
            ))
            .blocking_show();

        if !confirmed {
            return;
        }

        if let Err(reason) = install_to_applications(&bundle, DEST, from_dmg) {
            app.dialog()
                .message(format!(
                    "Couldn't move Locution automatically ({reason}). You can drag it into \
                     your Applications folder manually, then reopen it."
                ))
                .title("Move to Applications")
                .blocking_show();
            return;
        }

        // Launch the freshly installed copy, then quit this one.
        let _ = std::process::Command::new("/usr/bin/open")
            .arg(DEST)
            .spawn();
        app.exit(0);
    });
}

/// Copy the running bundle into `/Applications` with `ditto` (faithful across
/// volumes, unlike `fs::rename` off a read-only DMG), replacing any existing
/// install. A `~/Downloads` source is removed afterward so two copies don't
/// linger; a DMG source is read-only and left for the user to eject.
#[cfg(target_os = "macos")]
fn install_to_applications(
    bundle: &std::path::Path,
    dest: &str,
    from_dmg: bool,
) -> Result<(), String> {
    use std::path::Path;

    if Path::new(dest).exists() {
        std::fs::remove_dir_all(dest).map_err(|e| format!("removing the old copy failed: {e}"))?;
    }

    let status = std::process::Command::new("/usr/bin/ditto")
        .arg(bundle)
        .arg(dest)
        .status()
        .map_err(|e| format!("ditto failed to run: {e}"))?;
    if !status.success() {
        return Err("the copy did not complete".to_string());
    }

    if !from_dmg {
        let _ = std::fs::remove_dir_all(bundle);
    }
    Ok(())
}

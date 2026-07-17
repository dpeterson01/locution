use crate::input;
use crate::settings;
use crate::settings::{OverlayPosition, OverlayStyle};
use std::sync::atomic::{AtomicBool, Ordering};
use tauri::{AppHandle, Emitter, Manager, PhysicalPosition, PhysicalSize};

#[cfg(not(target_os = "macos"))]
use log::debug;

#[cfg(not(target_os = "macos"))]
use tauri::WebviewWindowBuilder;

#[cfg(target_os = "macos")]
use tauri::WebviewUrl;

#[cfg(target_os = "macos")]
use tauri_nspanel::{tauri_panel, CollectionBehavior, PanelBuilder, PanelLevel, StyleMask};

#[cfg(target_os = "linux")]
use gtk_layer_shell::{Edge, KeyboardMode, Layer, LayerShell};

#[cfg(target_os = "linux")]
use std::env;

#[cfg(target_os = "macos")]
tauri_panel! {
    panel!(RecordingOverlayPanel {
        config: {
            can_become_key_window: false,
            is_floating_panel: true
        }
        // A non-activating panel is never key, so AppKit won't send it
        // mouseMoved events — which kills CSS :hover in the webview. An
        // always-active tracking area restores hover for the overlay's
        // controls without stealing focus from the frontmost app.
        with: {
            tracking_area: {
                options: tauri_nspanel::objc2_app_kit::NSTrackingAreaOptions::ActiveAlways
                    | tauri_nspanel::objc2_app_kit::NSTrackingAreaOptions::MouseEnteredAndExited
                    | tauri_nspanel::objc2_app_kit::NSTrackingAreaOptions::MouseMoved,
                auto_resize: true,
            }
        }
    })
}

// Native overlay window sizes (logical points). One window is reused for every
// state and resized in `show_overlay_state`; each size need only be at least as
// large as the card it hosts (the `--ov-*` vars in RecordingOverlay.css). The
// card is CSS-anchored flush to the screen edge, so window height doesn't move
// where the card sits — only OVERLAY_TOP_OFFSET / OVERLAY_BOTTOM_OFFSET do. Keep
// these in sync with the CSS card geometry.
//
// Compact overlay (Minimal / transcribing / processing): the 40h pill animates
// width from 172 (--ov-rest-w) to 216 (--ov-work-w) and expands from center, so
// the window must fit the widest state plus a little slack.
const OVERLAY_WIDTH: f64 = 256.0;
const OVERLAY_HEIGHT: f64 = 46.0;

// Actual is 394x118, just a little extra
const OVERLAY_STREAM_WIDTH: f64 = 400.0;
const OVERLAY_STREAM_HEIGHT: f64 = 120.0;

// Idle dash pill (always-show). Kept as small as possible: a transparent
// window still swallows clicks over its whole rect, so the idle footprint must
// hug the visible dash. This is the idle *minimum/reset* size — with per-app
// auto-mode on, the webview grows the width via `resize_overlay_window` to fit
// the at-rest mode label (measured pill width; see RecordingOverlay.tsx), just
// as hover growth (controls row, mode popover) resizes on demand.
const OVERLAY_IDLE_WIDTH: f64 = 96.0;
const OVERLAY_IDLE_HEIGHT: f64 = 26.0;

/// Overlay window size (logical) for a given UI state.
fn overlay_dimensions(state: &str) -> (f64, f64) {
    match state {
        "streaming" => (OVERLAY_STREAM_WIDTH, OVERLAY_STREAM_HEIGHT),
        "idle" => (OVERLAY_IDLE_WIDTH, OVERLAY_IDLE_HEIGHT),
        _ => (OVERLAY_WIDTH, OVERLAY_HEIGHT),
    }
}

// Gaps from the monitor *work area* edges (which already exclude the menu
// bar, Dock, and taskbar — see calculate_overlay_position), so these are just
// breathing room, not reserved-UI allowances.
#[cfg(target_os = "macos")]
const OVERLAY_TOP_OFFSET: f64 = 8.0;
#[cfg(any(target_os = "windows", target_os = "linux"))]
const OVERLAY_TOP_OFFSET: f64 = 4.0;

#[cfg(target_os = "macos")]
const OVERLAY_BOTTOM_OFFSET: f64 = 10.0;

#[cfg(any(target_os = "windows", target_os = "linux"))]
const OVERLAY_BOTTOM_OFFSET: f64 = 10.0;

#[cfg(target_os = "linux")]
fn update_gtk_layer_shell_anchors(overlay_window: &tauri::webview::WebviewWindow) {
    let window_clone = overlay_window.clone();
    let _ = overlay_window.run_on_main_thread(move || {
        // Try to get the GTK window from the Tauri webview
        if let Ok(gtk_window) = window_clone.gtk_window() {
            let settings = settings::get_settings(window_clone.app_handle());
            match settings.overlay_position {
                OverlayPosition::Top => {
                    gtk_window.set_anchor(Edge::Top, true);
                    gtk_window.set_anchor(Edge::Bottom, false);
                }
                OverlayPosition::Bottom => {
                    gtk_window.set_anchor(Edge::Bottom, true);
                    gtk_window.set_anchor(Edge::Top, false);
                }
            }
        }
    });
}

/// Returns true when the environment variable is set to a truthy value
/// (e.g. "1", "true", "yes", "on").
/// "0", "false", "no", "off" and empty string are treated as falsy (case-insensitive).
/// Returns false when the variable is not set.
#[cfg(target_os = "linux")]
fn env_flag_enabled(name: &str) -> bool {
    match env::var(name) {
        Ok(v) => !matches!(
            v.trim().to_ascii_lowercase().as_str(),
            "" | "0" | "false" | "no" | "off"
        ),
        Err(_) => false,
    }
}

/// Initializes GTK layer shell for Linux overlay window
/// Returns true if layer shell was successfully initialized, false otherwise
#[cfg(target_os = "linux")]
fn init_gtk_layer_shell(overlay_window: &tauri::webview::WebviewWindow) -> bool {
    if env_flag_enabled("HANDY_NO_GTK_LAYER_SHELL") {
        debug!("Skipping GTK layer shell init (HANDY_NO_GTK_LAYER_SHELL is enabled)");
        return false;
    }

    if !gtk_layer_shell::is_supported() {
        return false;
    }

    // Try to get the GTK window from the Tauri webview
    if let Ok(gtk_window) = overlay_window.gtk_window() {
        // Initialize layer shell
        gtk_window.init_layer_shell();
        gtk_window.set_layer(Layer::Overlay);
        gtk_window.set_keyboard_mode(KeyboardMode::None);
        gtk_window.set_exclusive_zone(0);

        update_gtk_layer_shell_anchors(overlay_window);

        return true;
    }
    false
}

/// Forces a window to be topmost using Win32 API (Windows only)
/// This is more reliable than Tauri's set_always_on_top which can be overridden
#[cfg(target_os = "windows")]
fn force_overlay_topmost(overlay_window: &tauri::webview::WebviewWindow) {
    use windows::Win32::UI::WindowsAndMessaging::{
        SetWindowPos, HWND_TOPMOST, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE, SWP_SHOWWINDOW,
    };

    // Clone because run_on_main_thread takes 'static
    let overlay_clone = overlay_window.clone();

    // Make sure the Win32 call happens on the UI thread
    let _ = overlay_clone.clone().run_on_main_thread(move || {
        if let Ok(hwnd) = overlay_clone.hwnd() {
            unsafe {
                // Force Z-order: make this window topmost without changing size/pos or stealing focus
                let _ = SetWindowPos(
                    hwnd,
                    Some(HWND_TOPMOST),
                    0,
                    0,
                    0,
                    0,
                    SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE | SWP_SHOWWINDOW,
                );
            }
        }
    });
}

fn get_monitor_with_cursor(app_handle: &AppHandle) -> Option<tauri::Monitor> {
    if let Some(mouse_location) = input::get_cursor_position(app_handle) {
        if let Ok(monitors) = app_handle.available_monitors() {
            for monitor in monitors {
                // Tauri's monitor position/size are physical pixels, but enigo
                // may return logical coordinates (confirmed on macOS via
                // NSEvent::mouseLocation; on Windows, GetCursorPos behavior
                // depends on the process DPI-awareness context). Dividing by
                // scale_factor normalizes to logical, which is safe regardless:
                // if enigo returns logical it matches directly, and if it returns
                // physical on a scale=1 monitor the division is a no-op.
                let scale = monitor.scale_factor();
                let pos = PhysicalPosition::new(
                    (monitor.position().x as f64 / scale) as i32,
                    (monitor.position().y as f64 / scale) as i32,
                );
                let size = PhysicalSize::new(
                    (monitor.size().width as f64 / scale) as u32,
                    (monitor.size().height as f64 / scale) as u32,
                );
                if is_mouse_within_monitor(mouse_location, &pos, &size) {
                    return Some(monitor);
                }
            }
        }
    }

    app_handle.primary_monitor().ok().flatten()
}

fn is_mouse_within_monitor(
    mouse_pos: (i32, i32),
    monitor_pos: &PhysicalPosition<i32>,
    monitor_size: &PhysicalSize<u32>,
) -> bool {
    let (mouse_x, mouse_y) = mouse_pos;
    let PhysicalPosition {
        x: monitor_x,
        y: monitor_y,
    } = *monitor_pos;
    let PhysicalSize {
        width: monitor_width,
        height: monitor_height,
    } = *monitor_size;

    mouse_x >= monitor_x
        && mouse_x < (monitor_x + monitor_width as i32)
        && mouse_y >= monitor_y
        && mouse_y < (monitor_y + monitor_height as i32)
}

/// Returns overlay position in logical coordinates (points on macOS).
///
/// Uses monitor position/size directly rather than work_area(), which can
/// return incorrect coordinates on macOS for monitors with negative positions.
/// The per-platform OVERLAY_TOP_OFFSET / OVERLAY_BOTTOM_OFFSET constants
/// already account for system chrome (menu bar, taskbar).
///
/// We must use LogicalPosition (not PhysicalPosition) because Tauri/tao
/// converts PhysicalPosition using the scale factor of the monitor the window
/// is *currently* on, which is wrong when moving cross-monitor.
fn calculate_overlay_position(
    app_handle: &AppHandle,
    width: f64,
    height: f64,
) -> Option<(f64, f64)> {
    let monitor = get_monitor_with_cursor(app_handle)?;
    let scale = monitor.scale_factor();
    // Anchor to the monitor's work area (excludes the Dock / taskbar and the
    // menu bar) so a bottom overlay floats above the Dock instead of over it.
    let area = monitor.work_area();
    let area_x = area.position.x as f64 / scale;
    let area_y = area.position.y as f64 / scale;
    let area_width = area.size.width as f64 / scale;
    let area_height = area.size.height as f64 / scale;

    let settings = settings::get_settings(app_handle);

    let x = area_x + (area_width - width) / 2.0;
    let y = match settings.overlay_position {
        OverlayPosition::Top => area_y + OVERLAY_TOP_OFFSET,
        OverlayPosition::Bottom => area_y + area_height - height - OVERLAY_BOTTOM_OFFSET,
    };

    Some((x, y))
}

/// Current overlay window size in logical units (points), for repositioning
/// without assuming a fixed size (compact vs. streaming).
fn current_overlay_logical_size(window: &tauri::webview::WebviewWindow) -> Option<(f64, f64)> {
    let size = window.inner_size().ok()?;
    let scale = window.scale_factor().ok()?;
    Some((size.width as f64 / scale, size.height as f64 / scale))
}

/// Creates the recording overlay window and keeps it hidden by default
#[cfg(not(target_os = "macos"))]
pub fn create_recording_overlay(app_handle: &AppHandle) {
    // On Linux (Wayland), monitor detection often fails, but we don't need exact coordinates
    // for Layer Shell as we use anchors. On other platforms, we require a monitor.
    #[cfg(not(target_os = "linux"))]
    {
        let position = calculate_overlay_position(app_handle, OVERLAY_WIDTH, OVERLAY_HEIGHT);
        if position.is_none() {
            debug!("Failed to determine overlay position, not creating overlay window");
            return;
        }
    }

    // Position starts unset — update_overlay_position() sets the correct
    // LogicalPosition before the overlay is shown.
    let mut builder = WebviewWindowBuilder::new(
        app_handle,
        "recording_overlay",
        tauri::WebviewUrl::App("src/overlay/index.html".into()),
    )
    .title("Recording")
    .resizable(false)
    .inner_size(OVERLAY_WIDTH, OVERLAY_HEIGHT)
    .shadow(false)
    .maximizable(false)
    .minimizable(false)
    .closable(false)
    .accept_first_mouse(true)
    .decorations(false)
    .always_on_top(true)
    .skip_taskbar(true)
    .transparent(true)
    .focusable(false)
    .focused(false)
    .visible(false);

    if let Some(data_dir) = crate::portable::data_dir() {
        builder = builder.data_directory(data_dir.join("webview"));
    }

    #[allow(unused_variables)]
    match builder.build() {
        Ok(window) => {
            #[cfg(target_os = "linux")]
            {
                // Try to initialize GTK layer shell, ignore errors if compositor doesn't support it
                if init_gtk_layer_shell(&window) {
                    debug!("GTK layer shell initialized for overlay window");
                } else {
                    debug!("GTK layer shell not available, falling back to regular window");
                }
            }

            debug!("Recording overlay window created successfully (hidden)");
        }
        Err(e) => {
            debug!("Failed to create recording overlay window: {}", e);
        }
    }
}

/// Creates the recording overlay panel and keeps it hidden by default (macOS)
#[cfg(target_os = "macos")]
pub fn create_recording_overlay(app_handle: &AppHandle) {
    if let Some((x, y)) = calculate_overlay_position(app_handle, OVERLAY_WIDTH, OVERLAY_HEIGHT) {
        // PanelBuilder creates a Tauri window then converts it to NSPanel.
        // The window remains registered, so get_webview_window() still works.
        match PanelBuilder::<_, RecordingOverlayPanel>::new(app_handle, "recording_overlay")
            .url(WebviewUrl::App("src/overlay/index.html".into()))
            .title("Recording")
            .position(tauri::Position::Logical(tauri::LogicalPosition { x, y }))
            .level(PanelLevel::Status)
            .size(tauri::Size::Logical(tauri::LogicalSize {
                width: OVERLAY_WIDTH,
                height: OVERLAY_HEIGHT,
            }))
            .has_shadow(false)
            .transparent(true)
            .no_activate(true)
            .corner_radius(0.0)
            .style_mask(StyleMask::empty().borderless().nonactivating_panel())
            .with_window(|w| w.decorations(false).transparent(true).focusable(false))
            .collection_behavior(
                CollectionBehavior::new()
                    .can_join_all_spaces()
                    .full_screen_auxiliary(),
            )
            // Complements the panel's always-active tracking area (see the
            // tauri_panel! definition above) so hover reaches the webview.
            .accepts_mouse_moved_events(true)
            .build()
        {
            Ok(panel) => {
                panel.hide();
            }
            Err(e) => {
                log::error!("Failed to create recording overlay panel: {}", e);
            }
        }
    }
}

fn show_overlay_state(app_handle: &AppHandle, state: &str) {
    // Whether the overlay shows at all is governed by overlay_style; position
    // only chooses Top vs Bottom placement.
    let settings = settings::get_settings(app_handle);
    if settings.overlay_style == OverlayStyle::None {
        return;
    }

    // Size the overlay for this state (compact vs. streaming), then position it.
    let (width, height) = overlay_dimensions(state);
    if let Some(overlay_window) = app_handle.get_webview_window("recording_overlay") {
        #[cfg(target_os = "linux")]
        update_gtk_layer_shell_anchors(&overlay_window);

        let _ = overlay_window.set_size(tauri::Size::Logical(tauri::LogicalSize { width, height }));
        if let Some((x, y)) = calculate_overlay_position(app_handle, width, height) {
            let _ = overlay_window
                .set_position(tauri::Position::Logical(tauri::LogicalPosition { x, y }));
        }

        let _ = overlay_window.show();

        // On Windows, aggressively re-assert "topmost" in the native Z-order after showing
        #[cfg(target_os = "windows")]
        force_overlay_topmost(&overlay_window);

        let _ = overlay_window.emit("show-overlay", state);

        // Self-heal placement. At cold start (and after display or wake
        // changes) the monitor work-area metrics can still be unsettled when
        // the overlay first shows, leaving it mispositioned — and nothing
        // re-checks until the user toggles the position setting. Re-apply the
        // position shortly after showing so a bad initial placement corrects
        // itself. When the placement was already correct this is a no-op
        // (set_position to the same coordinates).
        let app_clone = app_handle.clone();
        std::thread::spawn(move || {
            std::thread::sleep(std::time::Duration::from_millis(300));
            update_overlay_position(&app_clone);
        });
    }
}

/// Shows the recording overlay window with fade-in animation
pub fn show_recording_overlay(app_handle: &AppHandle) {
    show_overlay_state(app_handle, "recording");
}

/// Shows the larger streaming overlay that displays live transcription text
pub fn show_streaming_overlay(app_handle: &AppHandle) {
    show_overlay_state(app_handle, "streaming");
}

/// Shows the transcribing overlay window
pub fn show_transcribing_overlay(app_handle: &AppHandle) {
    show_overlay_state(app_handle, "transcribing");
}

/// Shows the processing overlay window
pub fn show_processing_overlay(app_handle: &AppHandle) {
    show_overlay_state(app_handle, "processing");
}

/// Shows a short-lived terminal state on the compact overlay after processing
/// finishes: `"cleaned"`, `"transcribed"`, or `"failed"`. Reuses the compact
/// window at its default size; the webview swaps the spinner for a checkmark
/// (cleaned/transcribed) or an error glyph (failed). The caller is responsible
/// for hiding the overlay after a brief delay.
pub fn show_terminal_overlay(app_handle: &AppHandle, kind: &str) {
    show_overlay_state(app_handle, kind);
}

/// Shows the small idle dash pill (always-show mode). No-op when the user
/// disabled always-show, the overlay style is None, or a recording is in
/// flight — settings changed mid-recording (e.g. the overlay's Expand button)
/// must not swap the recording overlay for the idle dash.
pub fn show_idle_overlay(app_handle: &AppHandle) {
    let recording = app_handle
        .try_state::<std::sync::Arc<crate::managers::audio::AudioRecordingManager>>()
        .map(|rm| rm.active_recording_binding().is_some())
        .unwrap_or(false);
    if recording {
        log::debug!("show_idle_overlay skipped: recording in flight");
        return;
    }
    let settings = settings::get_settings(app_handle);
    if !settings.overlay_always_show || settings.overlay_style == OverlayStyle::None {
        return;
    }
    show_overlay_state(app_handle, "idle");
}

/// Called by the overlay webview once its event listeners are mounted. Shows
/// the resting idle pill at that moment — a fixed startup delay races the
/// webview load and can emit show-overlay before anyone is listening, leaving
/// an invisible (but click-swallowing) window until the next state change.
#[tauri::command]
#[specta::specta]
pub fn overlay_ready(app: AppHandle) {
    // show_idle_overlay itself skips when a recording is in flight.
    show_idle_overlay(&app);
}

/// Cursor position in the overlay window's logical coordinate space, or None
/// when the cursor is outside the window. Drives the overlay's hover UI: the
/// non-activating panel never becomes key, so AppKit doesn't deliver the
/// mouseMoved stream WebKit needs for CSS :hover — the frontend polls this
/// instead and applies hover classes itself.
#[tauri::command]
#[specta::specta]
pub fn overlay_cursor_position(app: AppHandle) -> Option<(f64, f64)> {
    let window = app.get_webview_window("recording_overlay")?;
    if !window.is_visible().ok()? {
        return None;
    }
    // NOTE(windows, deferred): enigo's location() returns LOGICAL points on
    // macOS (NSEvent.mouseLocation) — which this math assumes — but PHYSICAL
    // pixels on Windows, so hover targeting would be offset on scaled Windows
    // displays. If a Windows pass ever happens, add:
    //   #[cfg(target_os = "windows")]
    //   let (cx, cy) = (cx / scale, cy / scale);
    // (mind per-monitor DPI). macOS is the only supported target today.
    let (cx, cy) = input::get_cursor_position(&app)?;
    let scale = window.scale_factor().ok()?;
    let pos = window.outer_position().ok()?;
    let size = window.outer_size().ok()?;
    let x = cx as f64 - pos.x as f64 / scale;
    let y = cy as f64 - pos.y as f64 / scale;
    let w = size.width as f64 / scale;
    let h = size.height as f64 / scale;
    if x < 0.0 || y < 0.0 || x >= w || y >= h {
        return None;
    }
    Some((x, y))
}

/// Resize the overlay window for its interaction states (hover controls, mode
/// popover) and re-center it on the configured screen edge. Called from the
/// overlay webview via the `resize_overlay` command.
pub fn resize_overlay_window(app_handle: &AppHandle, width: f64, height: f64) {
    // Clamp to sane bounds so a misbehaving webview can't cover the screen.
    let width = width.clamp(OVERLAY_IDLE_WIDTH, 480.0);
    let height = height.clamp(OVERLAY_IDLE_HEIGHT, 360.0);
    if let Some(overlay_window) = app_handle.get_webview_window("recording_overlay") {
        let _ = overlay_window.set_size(tauri::Size::Logical(tauri::LogicalSize { width, height }));
        if let Some((x, y)) = calculate_overlay_position(app_handle, width, height) {
            let _ = overlay_window
                .set_position(tauri::Position::Logical(tauri::LogicalPosition { x, y }));
        }
    }
}

/// Updates the overlay window position based on current settings
pub fn update_overlay_position(app_handle: &AppHandle) {
    if let Some(overlay_window) = app_handle.get_webview_window("recording_overlay") {
        #[cfg(target_os = "linux")]
        {
            update_gtk_layer_shell_anchors(&overlay_window);
        }

        // Use the window's current size so centering stays correct whether the
        // overlay is in compact or streaming layout.
        let (width, height) = current_overlay_logical_size(&overlay_window)
            .unwrap_or((OVERLAY_WIDTH, OVERLAY_HEIGHT));
        if let Some((x, y)) = calculate_overlay_position(app_handle, width, height) {
            let _ = overlay_window
                .set_position(tauri::Position::Logical(tauri::LogicalPosition { x, y }));
        }
    }
}

/// Hides the recording overlay window with fade-out animation. When
/// always-show is enabled the window returns to the idle dash pill after the
/// fade instead of disappearing.
pub fn hide_recording_overlay(app_handle: &AppHandle) {
    // Always run the fade regardless of settings - if setting was changed while
    // recording, we still want to leave the recording state properly
    if let Some(overlay_window) = app_handle.get_webview_window("recording_overlay") {
        let settings = settings::get_settings(app_handle);
        let return_to_idle =
            settings.overlay_always_show && settings.overlay_style != OverlayStyle::None;

        // Emit event to trigger fade-out animation
        let _ = overlay_window.emit("hide-overlay", ());
        // Hide (or swap to idle) after a short delay so the animation completes.
        // The idle swap goes through show_idle_overlay so a recording that was
        // re-triggered within the fade window keeps its recording overlay.
        let app_clone = app_handle.clone();
        let window_clone = overlay_window.clone();
        std::thread::spawn(move || {
            std::thread::sleep(std::time::Duration::from_millis(300));
            if return_to_idle {
                show_idle_overlay(&app_clone);
            } else {
                let _ = window_clone.hide();
            }
        });
    }
}

// Cached "overlay is enabled" flag, kept in sync with overlay_style. Avoids
// reading the Tauri store on every audio callback (~24 Hz during recording).
// Defaults to false so the audio path doesn't emit until lib.rs::setup
// populates the cache from initial settings.
static OVERLAY_ENABLED: AtomicBool = AtomicBool::new(false);

/// Update the cached overlay-enabled flag. Called from `lib.rs` at
/// startup after settings load, and from `change_overlay_style_setting`
/// whenever the user changes whether the overlay is shown.
pub fn update_overlay_enabled_cache(enabled: bool) {
    OVERLAY_ENABLED.store(enabled, Ordering::Relaxed);
}

pub fn emit_levels(app_handle: &AppHandle, levels: &[f32]) {
    // Skip emission when the overlay is disabled. The recording_overlay
    // window is created at boot regardless of overlay_style, so without this
    // guard a hidden overlay's WebKit subprocess still
    // processes every event. Each event drives some kind of WebKit
    // C++ allocation that accumulates without bound (mechanism not
    // directly characterized; see issue #1279 for the investigation).
    // For users with `overlay_style: none` (the Linux default) this skip
    // eliminates the upstream driver of that accumulation.
    if !OVERLAY_ENABLED.load(Ordering::Relaxed) {
        return;
    }

    // Target only the overlay window. In Tauri 2 both `AppHandle::emit`
    // and `WebviewWindow::emit` broadcast to all webviews; Tauri's
    // listener filter then skips webviews with no registered listener
    // for the event, so the settings webview never received `mic-level`.
    // But the previous dual-call pattern still produced two `eval_script`
    // calls to the overlay per audio callback (one from each .emit()).
    // `emit_to` with the overlay's window label produces a single
    // eval_script call per callback, cutting the per-callback WebKit
    // dispatch work in half.
    let _ = app_handle.emit_to("recording_overlay", "mic-level", levels);
}

//! Screen-context capture for the Context cleanup mode (Phase 7).
//!
//! Best-effort, privacy-gated capture of the frontmost app, the focused
//! element's selected text (Accessibility API), and the clipboard. Callers
//! gate on the global `context_capture_enabled` switch, the selected mode's
//! `use_context`, and the local Custom/Ollama provider BEFORE calling
//! [`capture`] — nothing here is read unless all gates pass. Only the shape
//! of captured content (booleans/lengths) is ever logged.

use tauri::AppHandle;

/// What was on screen when dictation stopped. All fields best-effort.
#[derive(Debug, Clone, Default)]
pub struct ContextSnapshot {
    pub app_name: Option<String>,
    pub bundle_id: Option<String>,
    pub selected_text: Option<String>,
    /// Full text already in the focused input box (AXValue), captured only
    /// when it is small enough to be a compose field rather than a document
    /// (see `sanitize_field_text`). Lets cleanup match spellings the user
    /// already typed instead of overwriting them.
    pub field_text: Option<String>,
    /// Best-effort "who is this addressed to" for the supported messaging
    /// apps (Teams, Outlook, Messages). See `macos::recipients_for_app`.
    pub recipients: Option<String>,
    pub clipboard: Option<String>,
}

impl ContextSnapshot {
    pub fn is_empty(&self) -> bool {
        self.app_name.is_none()
            && self.bundle_id.is_none()
            && self.selected_text.is_none()
            && self.field_text.is_none()
            && self.recipients.is_none()
            && self.clipboard.is_none()
    }
}

/// Character-count caps keep the local model's context sane and bound what a
/// stray mega-clipboard can inject.
const MAX_SELECTION_CHARS: usize = 2000;
const MAX_CLIPBOARD_CHARS: usize = 1000;
const MAX_APP_META_CHARS: usize = 200;
/// Field text above this length is treated as a document, not a compose box,
/// and dropped entirely (see `sanitize_field_text`).
const MAX_FIELD_CHARS: usize = 2000;
/// Recipient strings are short (a name or a few names); cap well below the
/// content channels.
const MAX_RECIPIENTS_CHARS: usize = 300;

/// Trim, drop empties, neutralize a literal `${output}` (the legacy prompt
/// path replaces every occurrence, so captured content must never smuggle the
/// placeholder in), and truncate on a char boundary.
fn sanitize(raw: String, max_chars: usize) -> Option<String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return None;
    }
    let cleaned = trimmed.replace("${output}", "[output]");
    Some(cleaned.chars().take(max_chars).collect())
}

/// Field text is a spelling reference for a compose box, not a document. If
/// the focused field holds more than `MAX_FIELD_CHARS`, it is an editor or a
/// long document, so DROP it rather than inject a truncated slab that would
/// mislead cleanup. Otherwise trim and neutralize `${output}` like `sanitize`.
fn sanitize_field_text(raw: String) -> Option<String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() || trimmed.chars().count() > MAX_FIELD_CHARS {
        return None;
    }
    Some(trimmed.replace("${output}", "[output]"))
}

#[cfg(target_os = "macos")]
mod macos {
    use std::ffi::c_void;

    use tauri_nspanel::objc2::runtime::AnyObject;
    use tauri_nspanel::objc2_app_kit::NSWorkspace;
    use tauri_nspanel::objc2_foundation::NSString;

    type CFTypeRef = *const c_void;
    type CFStringRef = *const c_void; // toll-free bridged NSString
    type AXUIElementRef = CFTypeRef; // opaque CF type
    type AXError = i32; // kAXErrorSuccess == 0

    #[link(name = "ApplicationServices", kind = "framework")]
    extern "C" {
        fn AXUIElementCreateSystemWide() -> AXUIElementRef;
        fn AXUIElementCopyAttributeValue(
            element: AXUIElementRef,
            attribute: CFStringRef,
            value: *mut CFTypeRef,
        ) -> AXError;
        fn AXUIElementSetMessagingTimeout(element: AXUIElementRef, timeout_seconds: f32)
            -> AXError;
        fn AXUIElementCreateApplication(pid: i32) -> AXUIElementRef;
    }

    #[link(name = "CoreFoundation", kind = "framework")]
    extern "C" {
        fn CFRelease(cf: CFTypeRef);
    }

    /// Password fields report role `AXTextField` with SUBROLE
    /// `AXSecureTextField` — the subrole, not the role, carries the secure
    /// marker, so the guard must query `AXSubrole`.
    pub(super) const SECURE_FIELD_ATTRIBUTE: &str = "AXSubrole";
    const SECURE_FIELD_SUBROLE: &str = "AXSecureTextField";

    pub(super) fn is_secure_field(subrole: Option<&str>) -> bool {
        subrole == Some(SECURE_FIELD_SUBROLE)
    }

    /// Frontmost app name + bundle id via NSWorkspace. Main thread only
    /// (dispatched by capture()).
    pub(super) fn frontmost_app() -> (Option<String>, Option<String>) {
        let workspace = NSWorkspace::sharedWorkspace();
        match workspace.frontmostApplication() {
            Some(app) => (
                app.localizedName().map(|s| s.to_string()),
                app.bundleIdentifier().map(|s| s.to_string()),
            ),
            None => (None, None),
        }
    }

    /// Read an AX attribute, downcasting the result to a string. Non-string
    /// values (AXValue, attributed strings, elements) return None unless the
    /// caller asked for the raw pointer via `copy_raw`.
    unsafe fn copy_string_attribute(element: AXUIElementRef, attribute: &str) -> Option<String> {
        let value = copy_raw(element, attribute)?;
        // Toll-free bridging: a CFString result is an NSString subclass at
        // runtime, so isKindOfClass-based downcast accepts exactly strings.
        let obj = &*(value as *const AnyObject);
        let text = obj.downcast_ref::<NSString>().map(|s| s.to_string());
        CFRelease(value);
        text
    }

    /// Copy an AX attribute value (caller owns the result — Copy rule).
    unsafe fn copy_raw(element: AXUIElementRef, attribute: &str) -> Option<CFTypeRef> {
        let name = NSString::from_str(attribute);
        let name_ref = &*name as *const NSString as CFStringRef;
        let mut value: CFTypeRef = std::ptr::null();
        let err = AXUIElementCopyAttributeValue(element, name_ref, &mut value);
        // Nonzero AXError (NoValue -25212, AttributeUnsupported -25205,
        // APIDisabled -25211, CannotComplete -25204, ...) all mean "absent"
        // for this best-effort capture.
        if err != 0 || value.is_null() {
            if err != 0 && err != -25212 && err != -25205 {
                log::debug!("AX attribute '{}' unavailable (AXError {})", attribute, err);
            }
            return None;
        }
        Some(value)
    }

    /// Reads from the focused element (selected text + full field value) plus
    /// per-app recipients, gathered in one main-thread pass.
    #[derive(Default)]
    pub(super) struct FocusedReads {
        pub(super) selected_text: Option<String>,
        pub(super) field_text: Option<String>,
        pub(super) recipients: Option<String>,
    }

    /// Frontmost app pid via NSWorkspace, needed to build a per-app
    /// AXUIElement for the recipient read. Main thread only.
    fn frontmost_pid() -> Option<i32> {
        let workspace = NSWorkspace::sharedWorkspace();
        workspace
            .frontmostApplication()
            .map(|app| app.processIdentifier())
    }

    /// Best-effort "who is this addressed to" for the three supported
    /// messaging apps. Reads the app's focused-window title, which is the
    /// conversation/recipient name in Messages and the chat title in Teams;
    /// for Outlook it is the compose subject, a weaker signal. A precise
    /// To/participants field read needs per-app AX-tree paths validated with
    /// Accessibility Inspector — this window-title read is the reliable floor.
    /// Scoped to these bundle ids only; no recipient capture in any other app.
    fn recipients_for_app(bundle_id: Option<&str>) -> Option<String> {
        let bundle = bundle_id?;
        if !matches!(
            bundle,
            "com.microsoft.teams2" | "com.microsoft.Outlook" | "com.apple.MobileSMS"
        ) {
            return None;
        }
        let pid = frontmost_pid()?;
        unsafe {
            let app = AXUIElementCreateApplication(pid);
            if app.is_null() {
                return None;
            }
            let _ = AXUIElementSetMessagingTimeout(app, 0.05);
            let title = match copy_raw(app, "AXFocusedWindow") {
                Some(window) => {
                    let t = copy_string_attribute(window, "AXTitle");
                    CFRelease(window);
                    t
                }
                None => None,
            };
            CFRelease(app);
            title
        }
    }

    /// Selected text AND full value of the focused UI element (skipping secure
    /// fields), plus per-app recipients — one main-thread pass. Bounded by the
    /// AX messaging timeout. AX reads are synchronous IPC into the target app;
    /// the per-message 50ms timeout keeps a hung target from blocking the main
    /// thread past capture()'s budget across the ~6 messages this can send
    /// (focused element, subrole, selected text, value, focused window, title).
    pub(super) fn focused_reads(bundle_id: Option<&str>) -> FocusedReads {
        let recipients = recipients_for_app(bundle_id);
        unsafe {
            let system = AXUIElementCreateSystemWide();
            if system.is_null() {
                return FocusedReads {
                    recipients,
                    ..Default::default()
                };
            }
            let _ = AXUIElementSetMessagingTimeout(system, 0.05);

            let focused = match copy_raw(system, "AXFocusedUIElement") {
                Some(f) => f,
                None => {
                    CFRelease(system);
                    return FocusedReads {
                        recipients,
                        ..Default::default()
                    };
                }
            };

            // Defense-in-depth: macOS already refuses secure-field content via
            // AX, but skip explicitly so intent is documented. The same guard
            // covers both the selection and the full field value.
            let is_secure =
                is_secure_field(copy_string_attribute(focused, SECURE_FIELD_ATTRIBUTE).as_deref());
            let (selected_text, field_text) = if is_secure {
                (None, None)
            } else {
                (
                    copy_string_attribute(focused, "AXSelectedText"),
                    copy_string_attribute(focused, "AXValue"),
                )
            };

            CFRelease(focused);
            CFRelease(system);

            FocusedReads {
                selected_text,
                field_text,
                recipients,
            }
        }
    }

    /// Whether the currently-focused UI element is a secure (password) field.
    /// Standalone version of the check inlined in `focused_reads` — usable
    /// right before a paste, without the rest of `capture()`'s
    /// clipboard/app-metadata work. Main-thread only, like the other AX
    /// reads in this module.
    pub(super) fn is_focused_element_secure() -> bool {
        unsafe {
            let system = AXUIElementCreateSystemWide();
            if system.is_null() {
                return false;
            }
            let _ = AXUIElementSetMessagingTimeout(system, 0.1);

            let focused = match copy_raw(system, "AXFocusedUIElement") {
                Some(f) => f,
                None => {
                    CFRelease(system);
                    return false;
                }
            };

            let secure =
                is_secure_field(copy_string_attribute(focused, SECURE_FIELD_ATTRIBUTE).as_deref());

            CFRelease(focused);
            CFRelease(system);
            secure
        }
    }
}

/// Capture the current screen context. Must be called OFF the main thread
/// (the coordinator thread qualifies) — it dispatches the NSWorkspace/AX
/// reads to the main thread and waits with a timeout, so calling it from the
/// main thread would deadlock until the timeout.
#[cfg(target_os = "macos")]
pub fn capture(app: &AppHandle) -> Option<ContextSnapshot> {
    use std::sync::mpsc;
    use std::time::Duration;
    use tauri_plugin_clipboard_manager::ClipboardExt;

    let clipboard = app
        .clipboard()
        .read_text()
        .ok()
        .and_then(|s| sanitize(s, MAX_CLIPBOARD_CHARS));

    let (tx, rx) = mpsc::channel();
    let dispatched = app
        .run_on_main_thread(move || {
            // A late send after recv_timeout fails silently — fine.
            let app = macos::frontmost_app();
            let reads = macos::focused_reads(app.1.as_deref());
            let _ = tx.send((app, reads));
        })
        .is_ok();

    let ((app_name, bundle_id), reads) = if dispatched {
        rx.recv_timeout(Duration::from_millis(400))
            .unwrap_or(((None, None), macos::FocusedReads::default()))
    } else {
        ((None, None), macos::FocusedReads::default())
    };

    // App metadata is sanitized like the content channels: a localized app
    // name is still foreign input to the prompt template.
    let snapshot = ContextSnapshot {
        app_name: app_name.and_then(|s| sanitize(s, MAX_APP_META_CHARS)),
        bundle_id: bundle_id.and_then(|s| sanitize(s, MAX_APP_META_CHARS)),
        selected_text: reads.selected_text.and_then(|s| sanitize(s, MAX_SELECTION_CHARS)),
        field_text: reads.field_text.and_then(sanitize_field_text),
        recipients: reads
            .recipients
            .and_then(|s| sanitize(s, MAX_RECIPIENTS_CHARS)),
        clipboard,
    };

    // Shape only — never content.
    log::debug!(
        "Context capture: app={} selection_chars={} field_chars={} recipients={} clipboard_chars={}",
        snapshot.app_name.is_some(),
        snapshot
            .selected_text
            .as_deref()
            .map_or(0, |s| s.chars().count()),
        snapshot
            .field_text
            .as_deref()
            .map_or(0, |s| s.chars().count()),
        snapshot.recipients.is_some(),
        snapshot
            .clipboard
            .as_deref()
            .map_or(0, |s| s.chars().count()),
    );

    if snapshot.is_empty() {
        None
    } else {
        Some(snapshot)
    }
}

#[cfg(not(target_os = "macos"))]
pub fn capture(_app: &AppHandle) -> Option<ContextSnapshot> {
    None
}

/// Lightweight frontmost-app lookup (name, bundle id) for Phase 6 per-app
/// Style rules and the Settings "use current app" detect button — a
/// standalone, shorter main-thread dispatch than [`capture`], which also
/// does AX/clipboard work this doesn't need.
#[cfg(target_os = "macos")]
pub fn frontmost_app_info(app: &AppHandle) -> Option<(String, String)> {
    use std::sync::mpsc;
    use std::time::Duration;

    let (tx, rx) = mpsc::channel();
    let dispatched = app
        .run_on_main_thread(move || {
            let _ = tx.send(macos::frontmost_app());
        })
        .is_ok();

    if !dispatched {
        return None;
    }

    let (name, bundle_id) = rx.recv_timeout(Duration::from_millis(150)).ok()?;
    Some((name?, bundle_id?))
}

/// Windows: frontmost app via Win32. The owning process exe basename (e.g.
/// `Code.exe`) is the Windows analog of the macOS bundle id and is what the
/// per-app mode map keys on. These Win32 calls are thread-agnostic
/// (`GetForegroundWindow` returns the system-wide foreground window), so
/// unlike the macOS path no main-thread dispatch is needed.
#[cfg(target_os = "windows")]
mod windows_impl {
    use windows::core::PWSTR;
    use windows::Win32::Foundation::{CloseHandle, HWND};
    use windows::Win32::System::Threading::{
        OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_WIN32,
        PROCESS_QUERY_LIMITED_INFORMATION,
    };
    use windows::Win32::UI::WindowsAndMessaging::{
        GetForegroundWindow, GetWindowTextLengthW, GetWindowTextW, GetWindowThreadProcessId,
    };

    /// Comfortably covers Win32 exe paths (classic MAX_PATH is 260); a longer
    /// path just yields ERROR_INSUFFICIENT_BUFFER, which we treat as "absent".
    const PATH_BUF_LEN: usize = 1024;

    /// (window title, process exe basename), both best-effort.
    pub(super) fn frontmost_app() -> (Option<String>, Option<String>) {
        unsafe {
            let hwnd = GetForegroundWindow();
            if hwnd.0.is_null() {
                return (None, None);
            }
            (window_title(hwnd), process_exe_basename(hwnd))
        }
    }

    unsafe fn window_title(hwnd: HWND) -> Option<String> {
        let len = GetWindowTextLengthW(hwnd);
        if len <= 0 {
            return None;
        }
        // +1 for the null terminator GetWindowTextW writes.
        let mut buf = vec![0u16; len as usize + 1];
        let copied = GetWindowTextW(hwnd, &mut buf);
        if copied <= 0 {
            return None;
        }
        let title = String::from_utf16_lossy(&buf[..copied as usize]);
        let title = title.trim();
        (!title.is_empty()).then(|| title.to_string())
    }

    unsafe fn process_exe_basename(hwnd: HWND) -> Option<String> {
        let mut pid: u32 = 0;
        GetWindowThreadProcessId(hwnd, Some(&mut pid as *mut u32));
        if pid == 0 {
            return None;
        }
        // PROCESS_QUERY_LIMITED_INFORMATION needs no elevation for most apps.
        let handle = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid).ok()?;

        let mut buf = vec![0u16; PATH_BUF_LEN];
        let mut size = buf.len() as u32;
        let query = QueryFullProcessImageNameW(
            handle,
            PROCESS_NAME_WIN32,
            PWSTR(buf.as_mut_ptr()),
            &mut size,
        );
        let _ = CloseHandle(handle);

        query.ok()?;
        if size == 0 {
            return None;
        }
        let full_path = String::from_utf16_lossy(&buf[..size as usize]);
        // Basename after the last path separator; the exe name is the map key.
        let base = full_path
            .rsplit(|c| c == '\\' || c == '/')
            .next()
            .unwrap_or(&full_path)
            .trim();
        (!base.is_empty()).then(|| base.to_string())
    }
}

#[cfg(target_os = "windows")]
pub fn frontmost_app_info(_app: &AppHandle) -> Option<(String, String)> {
    let (name, bundle_id) = windows_impl::frontmost_app();
    let bundle_id = bundle_id?;
    // The exe name is the essential map key; fall back to it for the display
    // name so per-app auto-mode still works on title-less windows.
    let name = name.unwrap_or_else(|| bundle_id.clone());
    Some((name, bundle_id))
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
pub fn frontmost_app_info(_app: &AppHandle) -> Option<(String, String)> {
    None
}

/// Best-effort check of whether the currently-focused element (e.g. right
/// before a paste) is a secure/password field. Shape signal only — used to
/// record that a paste landed on what looks like a secure field, never to
/// inspect its content.
#[cfg(target_os = "macos")]
pub fn is_focused_element_secure() -> bool {
    macos::is_focused_element_secure()
}

#[cfg(not(target_os = "macos"))]
pub fn is_focused_element_secure() -> bool {
    false
}

#[cfg(all(test, target_os = "macos"))]
mod tests {
    use super::macos::{is_secure_field, SECURE_FIELD_ATTRIBUTE};

    /// Structural regression guard: the secure-field check must key off the
    /// SUBROLE attribute. Password fields are role `AXTextField` + subrole
    /// `AXSecureTextField`; the original implementation compared `AXRole`
    /// against the subrole value and therefore never matched.
    #[test]
    fn secure_field_guard_reads_subrole_not_role() {
        assert_eq!(SECURE_FIELD_ATTRIBUTE, "AXSubrole");
        assert!(is_secure_field(Some("AXSecureTextField")));
        assert!(!is_secure_field(Some("AXTextField"))); // the ROLE of a password field
        assert!(!is_secure_field(Some("AXStandardWindow")));
        assert!(!is_secure_field(None));
    }

    #[test]
    fn sanitize_neutralizes_placeholder_and_truncates_on_char_boundary() {
        assert_eq!(
            super::sanitize("  hi ${output} there  ".to_string(), 100),
            Some("hi [output] there".to_string())
        );
        assert_eq!(super::sanitize("   ".to_string(), 100), None);
        // Multibyte chars: take() counts chars, never splits a code point.
        assert_eq!(
            super::sanitize("héllo".to_string(), 2),
            Some("hé".to_string())
        );
    }
}

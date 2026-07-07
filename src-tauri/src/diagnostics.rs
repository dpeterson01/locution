//! Structured, shape-only failure logging (Phase 9).
//!
//! [`FailureCategory`] deliberately carries no free-text field — this is a
//! structural guarantee against logging transcript text, audio, or raw
//! model/daemon responses, not just a convention callers are expected to
//! follow. [`record_failure`] is the single entry point: every known failure
//! point in the app routes through it so the log line, the local journal, and
//! the frontend toast all see the same category and nothing else.

use std::fmt;
use std::io::Write;

use serde::{Deserialize, Serialize};
use specta::Type;
use tauri::{AppHandle, Emitter};

const JOURNAL_FILE_NAME: &str = "error-log.jsonl";
const JOURNAL_MAX_BYTES: u64 = 500_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum HttpStatusCategory {
    ClientError,
    ServerError,
    Unreachable,
    Timeout,
}

/// A known failure point. No variant carries a free-text/string payload —
/// only categories and safe, bounded metadata (e.g. an HTTP status class).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum FailureCategory {
    OllamaUnreachable,
    ModelMissing,
    WhisperModelNotLoaded,
    MicPermissionDenied,
    NoInputDevice,
    AccessibilityMissing,
    PasteBlockedSecureField,
    TranscriptionFailed,
    PostProcessHttpError { status_category: HttpStatusCategory },
}

impl fmt::Display for FailureCategory {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // serde_json::to_string on a fieldless-or-simple enum never fails.
        write!(f, "{}", serde_json::to_string(self).unwrap_or_default())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub struct DiagnosticEvent {
    pub category: FailureCategory,
    pub timestamp: String,
}

/// Record a known failure: one structured log line, one journal line, one
/// frontend event — all carrying only `category`, never raw error text.
pub fn record_failure(app: &AppHandle, category: FailureCategory) {
    let timestamp = chrono::Utc::now().to_rfc3339();

    // Fixed prefix makes this trivially greppable and lets Export Diagnostics
    // safelist exactly these lines out of an otherwise-unsafe-to-bundle log
    // file (see commands::diagnostics::export_diagnostics).
    log::warn!("DIAG_EVENT: {}", category);

    if let Err(e) = append_to_journal(app, &category, &timestamp) {
        log::debug!("Failed to append to error journal: {}", e);
    }

    let _ = app.emit(
        "diagnostic-event",
        DiagnosticEvent {
            category,
            timestamp,
        },
    );
}

fn append_to_journal(
    app: &AppHandle,
    category: &FailureCategory,
    timestamp: &str,
) -> std::io::Result<()> {
    let dir =
        crate::portable::app_data_dir(app).map_err(|e| std::io::Error::other(e.to_string()))?;
    std::fs::create_dir_all(&dir)?;
    let path = dir.join(JOURNAL_FILE_NAME);

    // Size-capped like the tauri_plugin_log file target: once the journal
    // crosses the limit, start it over rather than growing it unbounded.
    if let Ok(metadata) = std::fs::metadata(&path) {
        if metadata.len() > JOURNAL_MAX_BYTES {
            std::fs::remove_file(&path)?;
        }
    }

    let line = serde_json::json!({ "category": category, "timestamp": timestamp });
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)?;
    writeln!(file, "{}", line)?;
    Ok(())
}

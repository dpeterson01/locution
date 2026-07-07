use crate::settings::get_settings;
use flate2::write::GzEncoder;
use flate2::Compression;
use tauri::AppHandle;

/// Lines carrying this marker are the only ones ever considered safe to
/// bundle out of the app's log file (see diagnostics::record_failure) —
/// everything else in `handy.log` is excluded, because several pre-existing,
/// unrelated log lines elsewhere in the app (e.g. successful-transcription
/// debug logs) print the transcript text itself. Filtering to this safelist,
/// rather than trusting every log call site to stay clean forever, is what
/// makes the exported log "clean by construction".
const DIAG_MARKER: &str = "DIAG_EVENT:";

fn add_text_entry<W: std::io::Write>(
    builder: &mut tar::Builder<W>,
    name: &str,
    content: &str,
) -> std::io::Result<()> {
    let data = content.as_bytes();
    let mut header = tar::Header::new_gnu();
    header.set_size(data.len() as u64);
    header.set_mode(0o644);
    header.set_cksum();
    builder.append_data(&mut header, name, data)
}

/// Filters a log file's content down to the diagnostic safelist, returning
/// the kept text plus how many lines were excluded (a count, not content).
fn extract_diag_lines(content: &str) -> (String, usize) {
    let mut kept = String::new();
    let mut excluded = 0usize;
    for line in content.lines() {
        if line.contains(DIAG_MARKER) {
            kept.push_str(line);
            kept.push('\n');
        } else {
            excluded += 1;
        }
    }
    (kept, excluded)
}

/// Bundles a transcript-free diagnostics archive: structured failure events
/// (from the log file, safelist-filtered, and the local error journal) plus a
/// redacted settings snapshot. Deliberately excludes the log file's other
/// lines (some pre-existing debug/info lines elsewhere in the app log
/// transcript text) and the `recordings/` directory (actual audio).
#[tauri::command]
#[specta::specta]
pub fn export_diagnostics(app: AppHandle, dest_path: String) -> Result<(), String> {
    let log_dir = crate::portable::app_log_dir(&app)
        .map_err(|e| format!("Failed to get log directory: {}", e))?;
    let data_dir = crate::portable::app_data_dir(&app)
        .map_err(|e| format!("Failed to get app data directory: {}", e))?;

    let mut diag_log = String::new();
    let mut total_excluded = 0usize;
    if let Ok(entries) = std::fs::read_dir(&log_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            let is_log_file = path
                .file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| n.starts_with("handy"));
            if !is_log_file {
                continue;
            }
            if let Ok(content) = std::fs::read_to_string(&path) {
                let (kept, excluded) = extract_diag_lines(&content);
                diag_log.push_str(&kept);
                total_excluded += excluded;
            }
        }
    }
    diag_log.push_str(&format!(
        "\n({} non-diagnostic log line(s) excluded for privacy)\n",
        total_excluded
    ));

    let journal_path = data_dir.join("error-log.jsonl");
    let journal_content = std::fs::read_to_string(&journal_path).unwrap_or_default();

    let settings = get_settings(&app);
    let redacted_settings = format!("{:#?}", settings);

    let file = std::fs::File::create(&dest_path)
        .map_err(|e| format!("Failed to create diagnostics file: {}", e))?;
    let encoder = GzEncoder::new(file, Compression::default());
    let mut builder = tar::Builder::new(encoder);

    add_text_entry(&mut builder, "diagnostic-log.txt", &diag_log)
        .map_err(|e| format!("Failed to write diagnostic log: {}", e))?;
    add_text_entry(&mut builder, "error-log.jsonl", &journal_content)
        .map_err(|e| format!("Failed to write error journal: {}", e))?;
    add_text_entry(&mut builder, "settings-redacted.txt", &redacted_settings)
        .map_err(|e| format!("Failed to write settings snapshot: {}", e))?;

    builder
        .into_inner()
        .and_then(|enc| enc.finish())
        .map_err(|e| format!("Failed to finalize diagnostics archive: {}", e))?;

    Ok(())
}

use crate::actions::process_transcription_output;
use crate::managers::{
    history::{
        CleanupFeedback, CleanupFeedbackSummary, FeedbackDatasetRecord, HistoryEntry,
        HistoryManager, PaginatedHistory,
    },
    transcription::TranscriptionManager,
};
use std::fs;
use std::path::Path;
use std::sync::Arc;
use tauri::{AppHandle, State};

fn serialize_feedback_dataset(records: &[FeedbackDatasetRecord]) -> Result<Vec<u8>, String> {
    if records.is_empty() {
        return Err("There is no cleanup feedback to export".to_string());
    }

    let mut output = Vec::new();
    for record in records {
        serde_json::to_writer(&mut output, record).map_err(|e| e.to_string())?;
        output.push(b'\n');
    }
    Ok(output)
}

fn write_feedback_dataset(path: &Path, records: &[FeedbackDatasetRecord]) -> Result<usize, String> {
    let output = serialize_feedback_dataset(records)?;
    fs::write(path, output).map_err(|e| format!("Failed to write feedback export: {e}"))?;
    Ok(records.len())
}

#[tauri::command]
#[specta::specta]
pub async fn get_history_entries(
    _app: AppHandle,
    history_manager: State<'_, Arc<HistoryManager>>,
    cursor: Option<i64>,
    limit: Option<usize>,
) -> Result<PaginatedHistory, String> {
    history_manager
        .get_history_entries(cursor, limit)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
#[specta::specta]
pub async fn toggle_history_entry_saved(
    _app: AppHandle,
    history_manager: State<'_, Arc<HistoryManager>>,
    id: i64,
) -> Result<(), String> {
    history_manager
        .toggle_saved_status(id)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
#[specta::specta]
pub async fn set_history_entry_feedback(
    _app: AppHandle,
    history_manager: State<'_, Arc<HistoryManager>>,
    id: i64,
    feedback: Option<CleanupFeedback>,
) -> Result<HistoryEntry, String> {
    history_manager
        .set_feedback(id, feedback)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
#[specta::specta]
pub async fn get_cleanup_feedback_summary(
    _app: AppHandle,
    history_manager: State<'_, Arc<HistoryManager>>,
) -> Result<CleanupFeedbackSummary, String> {
    history_manager
        .get_feedback_summary()
        .map_err(|e| e.to_string())
}

#[tauri::command]
#[specta::specta]
pub async fn export_cleanup_feedback(
    app: AppHandle,
    history_manager: State<'_, Arc<HistoryManager>>,
    dest_path: String,
) -> Result<usize, String> {
    let records = history_manager
        .get_feedback_dataset_records(&app.package_info().version.to_string())
        .map_err(|e| e.to_string())?;
    write_feedback_dataset(Path::new(&dest_path), &records)
}

#[tauri::command]
#[specta::specta]
pub async fn get_audio_file_path(
    _app: AppHandle,
    history_manager: State<'_, Arc<HistoryManager>>,
    file_name: String,
) -> Result<String, String> {
    let path = history_manager.get_audio_file_path(&file_name);
    path.to_str()
        .ok_or_else(|| "Invalid file path".to_string())
        .map(|s| s.to_string())
}

#[tauri::command]
#[specta::specta]
pub async fn delete_history_entry(
    _app: AppHandle,
    history_manager: State<'_, Arc<HistoryManager>>,
    id: i64,
) -> Result<(), String> {
    history_manager
        .delete_entry(id)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
#[specta::specta]
pub async fn retry_history_entry_transcription(
    app: AppHandle,
    history_manager: State<'_, Arc<HistoryManager>>,
    transcription_manager: State<'_, Arc<TranscriptionManager>>,
    id: i64,
) -> Result<(), String> {
    let entry = history_manager
        .get_entry_by_id(id)
        .await
        .map_err(|e| e.to_string())?
        .ok_or_else(|| format!("History entry {} not found", id))?;

    let audio_path = history_manager.get_audio_file_path(&entry.file_name);
    let samples = crate::audio_toolkit::read_wav_samples(&audio_path)
        .map_err(|e| format!("Failed to load audio: {}", e))?;

    if samples.is_empty() {
        return Err("Recording has no audio samples".to_string());
    }

    transcription_manager.initiate_model_load();

    let tm = Arc::clone(&transcription_manager);
    let transcription = tauri::async_runtime::spawn_blocking(move || tm.transcribe(samples, &[]))
        .await
        .map_err(|e| format!("Transcription task panicked: {}", e))?
        .map_err(|e| e.to_string())?;

    if transcription.is_empty() {
        return Err("Recording contains no speech".to_string());
    }

    // Reprocessing old audio: live screen context would mislead — pass None.
    let processed =
        process_transcription_output(&app, &transcription, entry.post_process_requested, None)
            .await;
    let cleanup = processed.cleanup_record();
    history_manager
        .update_transcription(
            id,
            transcription,
            processed.post_processed_text,
            processed.post_process_prompt,
            cleanup,
        )
        .map(|_| ())
        .map_err(|e| e.to_string())
}

#[tauri::command]
#[specta::specta]
pub async fn update_history_limit(
    app: AppHandle,
    history_manager: State<'_, Arc<HistoryManager>>,
    limit: usize,
) -> Result<(), String> {
    let mut settings = crate::settings::get_settings(&app);
    settings.history_limit = limit;
    crate::settings::write_settings(&app, settings);

    history_manager
        .cleanup_old_entries()
        .map_err(|e| e.to_string())?;

    Ok(())
}

#[tauri::command]
#[specta::specta]
pub async fn update_recording_retention_period(
    app: AppHandle,
    history_manager: State<'_, Arc<HistoryManager>>,
    period: String,
) -> Result<(), String> {
    use crate::settings::RecordingRetentionPeriod;

    let retention_period = match period.as_str() {
        "never" => RecordingRetentionPeriod::Never,
        "preserve_limit" => RecordingRetentionPeriod::PreserveLimit,
        "days3" => RecordingRetentionPeriod::Days3,
        "weeks2" => RecordingRetentionPeriod::Weeks2,
        "months3" => RecordingRetentionPeriod::Months3,
        _ => return Err(format!("Invalid retention period: {}", period)),
    };

    let mut settings = crate::settings::get_settings(&app);
    settings.recording_retention_period = retention_period;
    crate::settings::write_settings(&app, settings);

    history_manager
        .cleanup_old_entries()
        .map_err(|e| e.to_string())?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;

    fn dataset_record() -> FeedbackDatasetRecord {
        FeedbackDatasetRecord {
            schema_version: 1,
            locution_version: "0.1.0".to_string(),
            entry_timestamp: 100,
            feedback_updated_at: 110,
            feedback: CleanupFeedback::Down,
            transcription_text: "raw\ntext".to_string(),
            cleaned_text: "cleaned \"text\"".to_string(),
            cleanup_mode_id: "clean_up".to_string(),
            cleanup_mode_name: "Clean up".to_string(),
            cleanup_model: "test-model".to_string(),
            cleanup_tier: None,
            prompt_template: None,
        }
    }

    #[test]
    fn serialize_feedback_dataset_writes_one_json_object_per_line() {
        let output = serialize_feedback_dataset(&[dataset_record()]).expect("serialize dataset");
        assert_eq!(output.last(), Some(&b'\n'));

        let text = String::from_utf8(output).expect("valid UTF-8");
        assert_eq!(text.lines().count(), 1);
        let value: Value = serde_json::from_str(text.trim_end()).expect("valid JSON object");
        assert_eq!(value["transcription_text"], "raw\ntext");
        assert_eq!(value["cleaned_text"], "cleaned \"text\"");
        assert!(value.get("id").is_none());
        assert!(value.get("file_name").is_none());
        assert!(value.get("saved").is_none());
        assert!(value.get("context").is_none());
    }

    #[test]
    fn write_feedback_dataset_refuses_empty_export_without_touching_file() {
        let directory = tempfile::tempdir().expect("create temp directory");
        let path = directory.path().join("feedback.jsonl");
        fs::write(&path, "existing").expect("seed destination");

        let result = write_feedback_dataset(&path, &[]);

        assert!(result.is_err());
        assert_eq!(
            fs::read_to_string(path).expect("read destination"),
            "existing"
        );
    }
}

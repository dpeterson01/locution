export interface ModelStateEvent {
  event_type: string;
  model_id?: string;
  model_name?: string;
  error?: string;
}

export interface RecordingErrorEvent {
  error_type: string;
  detail?: string;
}

export type HttpStatusCategory =
  | "client_error"
  | "server_error"
  | "unreachable"
  | "timeout";

// Mirrors src-tauri/src/diagnostics.rs::FailureCategory. Every variant is a
// category tag only, never a free-text/error-message field.
export type FailureCategory =
  | "ollama_unreachable"
  | "model_missing"
  | "whisper_model_not_loaded"
  | "mic_permission_denied"
  | "no_input_device"
  | "accessibility_missing"
  | "paste_blocked_secure_field"
  | "transcription_failed"
  | { post_process_http_error: { status_category: HttpStatusCategory } };

export interface DiagnosticEvent {
  category: FailureCategory;
  timestamp: string;
}

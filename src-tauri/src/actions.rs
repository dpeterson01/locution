use crate::audio_feedback::{play_feedback_sound, play_feedback_sound_blocking, SoundType};
use crate::audio_toolkit::{is_microphone_access_denied, is_no_input_device_error, VadPolicy};
use crate::diagnostics::{self, FailureCategory, HttpStatusCategory};
use crate::llm_client::LlmError;
use crate::managers::audio::AudioRecordingManager;
use crate::managers::history::HistoryManager;
use crate::managers::model::ModelManager;
use crate::managers::transcription::StreamWorkKind;
use crate::managers::transcription::TranscriptionManager;
use crate::settings::{get_settings, AppSettings, OverlayStyle};
use crate::shortcut;
use crate::tray::{change_tray_icon, TrayIconState};
use crate::utils::{
    self, show_processing_overlay, show_recording_overlay, show_transcribing_overlay,
};
use crate::TranscriptionCoordinator;
use ferrous_opencc::{config::BuiltinConfig, OpenCC};
use log::{debug, error, warn};
use once_cell::sync::Lazy;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;
use tauri::Manager;
use tauri::{AppHandle, Emitter};

#[derive(Clone, serde::Serialize)]
struct RecordingErrorEvent {
    error_type: String,
    detail: Option<String>,
}

/// Maps a post-process HTTP failure to a diagnostic category. `provider_id`
/// disambiguates "unreachable"/"missing" for the local Ollama provider
/// ("custom") from the same failure on a cloud provider, where the toast
/// hint ("Open Ollama...") wouldn't make sense.
fn categorize_llm_error(provider_id: &str, e: LlmError) -> FailureCategory {
    let is_local = provider_id == "custom";
    match e {
        LlmError::Unreachable if is_local => FailureCategory::OllamaUnreachable,
        LlmError::Unreachable => FailureCategory::PostProcessHttpError {
            status_category: HttpStatusCategory::Unreachable,
        },
        LlmError::Timeout => FailureCategory::PostProcessHttpError {
            status_category: HttpStatusCategory::Timeout,
        },
        LlmError::HttpStatus(404) if is_local => FailureCategory::ModelMissing,
        LlmError::HttpStatus(code) if code >= 500 => FailureCategory::PostProcessHttpError {
            status_category: HttpStatusCategory::ServerError,
        },
        LlmError::HttpStatus(_) | LlmError::ParseError | LlmError::ClientError => {
            FailureCategory::PostProcessHttpError {
                status_category: HttpStatusCategory::ClientError,
            }
        }
    }
}

/// Drop guard that notifies the [`TranscriptionCoordinator`] when the
/// transcription pipeline finishes — whether it completes normally or panics.
struct FinishGuard(AppHandle);
impl Drop for FinishGuard {
    fn drop(&mut self) {
        if let Some(c) = self.0.try_state::<TranscriptionCoordinator>() {
            c.notify_processing_finished();
        }
    }
}

// Shortcut Action Trait
pub trait ShortcutAction: Send + Sync {
    fn start(&self, app: &AppHandle, binding_id: &str, shortcut_str: &str);
    fn stop(&self, app: &AppHandle, binding_id: &str, shortcut_str: &str);
}

/// How a [`TranscribeAction`] decides whether to run cleanup. The single
/// dictation hotkey resolves this from the `post_process_enabled` toggle at stop
/// time (`FromSettings`); an explicit always-clean binding forces it (`Force`).
#[derive(Clone, Copy)]
enum PostProcessMode {
    /// Read `settings.post_process_enabled` when the recording stops.
    FromSettings,
    /// Ignore the toggle and use this fixed value.
    Force(bool),
}

// Transcribe Action
struct TranscribeAction {
    post_process: PostProcessMode,
}

/// Field name for structured output JSON schema
const TRANSCRIPTION_FIELD: &str = "transcription";

/// Strip invisible Unicode characters that some LLMs may insert
fn strip_invisible_chars(s: &str) -> String {
    s.replace(['\u{200B}', '\u{200C}', '\u{200D}', '\u{FEFF}'], "")
}

/// Build a system prompt from the user's prompt template.
/// Removes `${output}` placeholder since the transcription is sent as the user message.
fn build_system_prompt(prompt_template: &str) -> String {
    prompt_template.replace("${output}", "").trim().to_string()
}

/// Returns `true` when a transcription has no meaningful content to
/// post-process (empty or whitespace-only). Used to skip the post-processing
/// LLM call when nothing was actually transcribed, which would otherwise make
/// the model reply with an error message such as "you need to provide the
/// transcription".
fn is_blank_transcription(transcription: &str) -> bool {
    transcription.trim().is_empty()
}

/// Length-based routing tier for adaptive cleanup (Ollama/custom provider only).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CleanupTier {
    /// Below `skip_llm_under_chars`: skip the LLM entirely; the built-in
    /// punctuation/filler filters already ran upstream.
    SkipLlm,
    /// At or below `short_threshold_chars`: fast model, no polishing overlay.
    Short,
    /// Above the threshold: thorough model with the polishing overlay.
    Long,
}

/// Metadata about the cleanup mode that actually ran, surfaced to the overlay
/// terminal state and the history badge (mode name · tier · model).
#[derive(Debug, Clone)]
pub(crate) struct CleanupModeInfo {
    pub id: String,
    pub name: String,
    pub model: String,
    pub tier: Option<CleanupTier>,
}

/// Outcome of [`post_process_transcription`]. Replaces a bare `Option<String>`
/// so callers can tell apart a real cleanup (with mode metadata) from a
/// deliberate skip (blank input, no provider, SkipLlm tier, empty response)
/// and a hard failure (the LLM errored — raw transcript is kept).
pub(crate) enum CleanupOutcome {
    Cleaned {
        text: String,
        mode_id: String,
        mode_name: String,
        model: String,
        tier: Option<CleanupTier>,
    },
    Skipped,
    Failed(FailureCategory),
}

/// Serialize a [`CleanupTier`] for history persistence. Kept in sync with the
/// history UI's tier label mapping.
pub(crate) fn cleanup_tier_key(tier: CleanupTier) -> &'static str {
    match tier {
        CleanupTier::SkipLlm => "skip_llm",
        CleanupTier::Short => "short",
        CleanupTier::Long => "long",
    }
}

/// Stable string key for a cleanup [`FailureCategory`], matching the frontend
/// `categoryKey` in App.tsx so the history UI can reuse the diagnostic i18n
/// strings.
pub(crate) fn cleanup_failure_key(category: &FailureCategory) -> String {
    match category {
        FailureCategory::PostProcessHttpError { status_category } => {
            let sc = match status_category {
                HttpStatusCategory::ClientError => "client_error",
                HttpStatusCategory::ServerError => "server_error",
                HttpStatusCategory::Unreachable => "unreachable",
                HttpStatusCategory::Timeout => "timeout",
            };
            format!("post_process_http_error:{sc}")
        }
        other => serde_json::to_string(other)
            .unwrap_or_default()
            .trim_matches('"')
            .to_string(),
    }
}

/// True when the currently selected cleanup mode opted into screen context.
fn selected_mode_uses_context(settings: &AppSettings) -> bool {
    settings
        .post_process_selected_prompt_id
        .as_deref()
        .and_then(|id| settings.post_process_prompts.iter().find(|p| p.id == id))
        .is_some_and(|p| p.use_context)
}

/// Labeled context block prepended above the prompt template. Absent items
/// are omitted; captured content was already sanitized (trimmed, truncated,
/// `${output}` neutralized) by context_capture.
fn build_context_block(ctx: &crate::context_capture::ContextSnapshot) -> String {
    let mut block = String::from("Context (reference only — do not copy it into the output):");
    if let Some(name) = &ctx.app_name {
        match &ctx.bundle_id {
            Some(bundle) => block.push_str(&format!("\nActive app: {} ({})", name, bundle)),
            None => block.push_str(&format!("\nActive app: {}", name)),
        }
    }
    if let Some(sel) = &ctx.selected_text {
        block.push_str(&format!("\nSelected text:\n{}", sel));
    }
    if let Some(clip) = &ctx.clipboard {
        block.push_str(&format!("\nClipboard:\n{}", clip));
    }
    block
}

/// Fixed guardrail against the cleanup step upgrading plain speech into
/// AI-sounding press-release voice. Always part of the style guidance block
/// when `style_card_enabled` is on — not user-editable, unlike `style_card`.
const NEGATIVE_STYLE_CONSTRAINTS: &str = "Do not introduce em dashes as connectors or for emphasis. Avoid rule-of-three padding (grouping things into exactly three for rhythm). Avoid vocabulary like \"vibrant\", \"crucial\", \"underscore\" (as a verb), or \"testament to\". Do not avoid the verb \"to be\" for stylistic variation. Avoid negative-parallelism constructions (\"not just X, but Y\"). Do not add chat pleasantries like \"I hope this helps\" or an unprompted sign-off. Keep the voice plain and direct.";

/// Builds the Voice guidance block: the fixed negative constraints + the
/// user's free-text style card, both gated by `style_card_enabled`. Voice is
/// global and always-on (no per-mode picker) — but the caller in
/// `post_process_transcription` gates *whether this is called at all* on the
/// fast/3b tier (`style_active`), since that tier echoes injected
/// instructions back instead of following them. That call-site gate is load
/// bearing and must not move into this function or be removed. Returns
/// `None` when there is nothing to say.
fn build_style_guidance_block(settings: &AppSettings) -> Option<String> {
    let mut parts: Vec<String> = Vec::new();

    if settings.style_card_enabled {
        parts.push(NEGATIVE_STYLE_CONSTRAINTS.to_string());
        if !settings.style_card.trim().is_empty() {
            parts.push(format!("My voice:\n{}", settings.style_card.trim()));
        }
    }

    if parts.is_empty() {
        return None;
    }

    Some(format!(
        "Style guidance (apply while cleaning; do not mention these rules in the output):\n{}",
        parts.join("\n\n")
    ))
}

/// Decide the adaptive cleanup tier for a transcription, or `None` when
/// adaptive routing does not apply (toggle off, or the active provider is not
/// the local "custom"/Ollama one — `short_model`/`long_model` are Ollama model
/// names, so other providers keep their single configured model).
fn adaptive_tier(settings: &AppSettings, transcription: &str) -> Option<CleanupTier> {
    // Adaptive length tiering is always on for the local "custom" provider
    // (short_model/long_model are Ollama model names). Other providers keep
    // their single configured model, so they never tier.
    if settings
        .active_post_process_provider()
        .map(|p| p.id.as_str())
        != Some("custom")
    {
        return None;
    }
    let chars = transcription.chars().count();
    if settings.skip_llm_under_chars > 0 && chars < settings.skip_llm_under_chars as usize {
        Some(CleanupTier::SkipLlm)
    } else if chars <= settings.short_threshold_chars as usize {
        Some(CleanupTier::Short)
    } else {
        Some(CleanupTier::Long)
    }
}

async fn post_process_transcription(
    app: &AppHandle,
    settings: &AppSettings,
    transcription: &str,
    context: Option<&crate::context_capture::ContextSnapshot>,
) -> CleanupOutcome {
    if is_blank_transcription(transcription) {
        debug!("Post-processing skipped because the transcription is empty");
        return CleanupOutcome::Skipped;
    }

    let provider = match settings.active_post_process_provider().cloned() {
        Some(provider) => provider,
        None => {
            debug!("Post-processing enabled but no provider is selected");
            return CleanupOutcome::Skipped;
        }
    };

    let tier = adaptive_tier(settings, transcription);
    if tier == Some(CleanupTier::SkipLlm) {
        debug!(
            "Adaptive cleanup: tier=SkipLlm chars={} — skipping LLM, using raw transcript",
            transcription.chars().count()
        );
        return CleanupOutcome::Skipped;
    }

    let selected_prompt_id = match &settings.post_process_selected_prompt_id {
        Some(id) => id.clone(),
        None => {
            debug!("Post-processing skipped because no prompt is selected");
            return CleanupOutcome::Skipped;
        }
    };

    let selected_mode = match settings
        .post_process_prompts
        .iter()
        .find(|prompt| prompt.id == selected_prompt_id)
    {
        Some(prompt) => prompt,
        None => {
            debug!(
                "Post-processing skipped because prompt '{}' was not found",
                selected_prompt_id
            );
            return CleanupOutcome::Skipped;
        }
    };
    let mode_id = selected_prompt_id.clone();
    let mode_name = selected_mode.name.clone();
    let mut prompt = selected_mode.prompt.clone();

    // Model selection: the mode's pinned model wins. Otherwise fall back to
    // the length tier (short input -> short_model, long input -> long_model);
    // the provider's primary model is the last resort for non-tiered (hosted)
    // providers.
    let mode_model = selected_mode
        .model
        .as_ref()
        .filter(|m| !m.trim().is_empty());
    let tier_fallback_model = match tier {
        Some(CleanupTier::Short) => Some(settings.short_model.trim().to_string()),
        Some(CleanupTier::Long) => Some(settings.long_model.trim().to_string()),
        _ => None,
    }
    .filter(|m| !m.is_empty());
    let model = mode_model.cloned().unwrap_or_else(|| {
        tier_fallback_model.unwrap_or_else(|| {
            settings
                .post_process_models
                .get(&provider.id)
                .cloned()
                .unwrap_or_default()
        })
    });

    // The length tier swaps the prompt ONLY when the active mode is one of the
    // protected Dictation tiers — the default length-based path. A per-app
    // category mode overrides short/long regardless of length, so it is used
    // verbatim (no tier prompt swap).
    let mut effective_use_context = selected_mode.use_context;
    let active_is_tier_mode =
        crate::settings::PROTECTED_MODE_IDS.contains(&selected_prompt_id.as_str());
    let tier_prompt_id = if active_is_tier_mode {
        match tier {
            Some(CleanupTier::Short) => settings.short_prompt_id.as_ref(),
            Some(CleanupTier::Long) => settings.long_prompt_id.as_ref(),
            _ => None,
        }
    } else {
        None
    };
    if let Some(id) = tier_prompt_id {
        if let Some(tier_prompt) = settings.post_process_prompts.iter().find(|p| &p.id == id) {
            prompt = tier_prompt.prompt.clone();
            effective_use_context = tier_prompt.use_context;
        }
    }

    // Whether this run will inject a context snapshot into the prompt.
    let context_active = effective_use_context
        && settings.context_capture_enabled
        && provider.id == "custom"
        && context.is_some_and(|c| !c.is_empty());

    if model.trim().is_empty() {
        debug!(
            "Post-processing skipped because provider '{}' has no model configured",
            provider.id
        );
        return CleanupOutcome::Skipped;
    }

    if let Some(tier) = tier {
        debug!(
            "Adaptive cleanup: tier={:?} chars={} mode={} model={}",
            tier,
            transcription.chars().count(),
            selected_prompt_id,
            model
        );
    }

    if prompt.trim().is_empty() {
        debug!("Post-processing skipped because the selected prompt is empty");
        return CleanupOutcome::Skipped;
    }

    // Prelude blocks stacked above the prompt template: style guidance (Phase
    // 6) first, then screen context (Phase 7). Both share the same
    // model-tier discipline — never on the fast/3b tier, which echoes
    // injected instructions back instead of following them (the llama3.2:3b
    // gotcha in CLAUDE.md) — and the same placement/scaffolding fix: the
    // block(s) above the whole template, an explicit label marking which
    // text is the dictation, and a hard output constraint AFTER the
    // transcript. Plain cleanup with neither active keeps the user's
    // template untouched, exactly as before this block existed.
    let mut prelude_blocks: Vec<String> = Vec::new();

    // Voice guidance: skip on the fast/3b tier (Clean up/Message/Verbatim are
    // pinned there) — 3b echoes preambles instead of following them, so
    // injecting Voice guidance would just get read back as if it were the
    // dictation. Comparing against `short_model` covers both the mode's own
    // pin and an adaptive downgrade; cloud providers never match it. This
    // gate is load bearing — see the doc comment on build_style_guidance_block.
    let style_active = model.trim() != settings.short_model.trim();
    if style_active {
        if let Some(style_block) = build_style_guidance_block(settings) {
            debug!(
                "Voice guidance injected: card_enabled={} card_chars={}",
                settings.style_card_enabled,
                settings.style_card.chars().count()
            );
            prelude_blocks.push(style_block);
        }
    }

    // Screen-context injection (Phase 7). Capture was gated on the *selected*
    // mode (tier unknown at capture time); injection is gated on the
    // *effective* mode after the tier override — a tier swap to a non-context
    // prompt deliberately drops the snapshot. Custom/Ollama only: context
    // never reaches a cloud provider.
    if context_active {
        if let Some(ctx) = context.filter(|c| !c.is_empty()) {
            prelude_blocks.push(build_context_block(ctx));
            debug!(
                "Context injected: mode={} app={} selection_chars={} clipboard_chars={}",
                selected_prompt_id,
                ctx.app_name.is_some(),
                ctx.selected_text
                    .as_deref()
                    .map_or(0, |s| s.chars().count()),
                ctx.clipboard.as_deref().map_or(0, |s| s.chars().count()),
            );
        }
    }

    if !prelude_blocks.is_empty() {
        let joined = prelude_blocks.join("\n\n");
        prompt = format!("{joined}\n\n{prompt}").replace(
            "${output}",
            "Dictation transcript:\n${output}\n\nRespond with nothing but \
             the cleaned dictation text — no preamble, no explanations, no \
             quotation marks, and never copy sentences from the context \
             into your response.",
        );
    }

    debug!(
        "Starting LLM post-processing with provider '{}' (model: {})",
        provider.id, model
    );

    let api_key = settings
        .post_process_api_keys
        .get(&provider.id)
        .cloned()
        .unwrap_or_default();

    // Disable reasoning for providers where post-processing rarely benefits from it.
    // - custom: top-level reasoning_effort (works for local OpenAI-compat servers)
    // - openrouter: nested reasoning object; exclude:true also keeps reasoning text
    //   out of the response so it can't pollute structured-output JSON parsing
    let (reasoning_effort, reasoning) = match provider.id.as_str() {
        "custom" => (Some("none".to_string()), None),
        "openrouter" => (
            None,
            Some(crate::llm_client::ReasoningConfig {
                effort: Some("none".to_string()),
                exclude: Some(true),
            }),
        ),
        _ => (None, None),
    };

    if provider.supports_structured_output {
        debug!("Using structured outputs for provider '{}'", provider.id);

        let system_prompt = build_system_prompt(&prompt);
        let user_content = transcription.to_string();

        // Define JSON schema for transcription output
        let json_schema = serde_json::json!({
            "type": "object",
            "properties": {
                (TRANSCRIPTION_FIELD): {
                    "type": "string",
                    "description": "The cleaned and processed transcription text"
                }
            },
            "required": [TRANSCRIPTION_FIELD],
            "additionalProperties": false
        });

        match crate::llm_client::send_chat_completion_with_schema(
            &provider,
            api_key.clone(),
            &model,
            user_content,
            Some(system_prompt),
            Some(json_schema),
            reasoning_effort.clone(),
            reasoning.clone(),
        )
        .await
        {
            Ok(Some(content)) => {
                // Parse the JSON response to extract the transcription field
                match serde_json::from_str::<serde_json::Value>(&content) {
                    Ok(json) => {
                        if let Some(transcription_value) =
                            json.get(TRANSCRIPTION_FIELD).and_then(|t| t.as_str())
                        {
                            let result = strip_invisible_chars(transcription_value);
                            debug!(
                                "Structured output post-processing succeeded for provider '{}'. Output length: {} chars",
                                provider.id,
                                result.len()
                            );
                            return CleanupOutcome::Cleaned {
                                text: result,
                                mode_id: mode_id.clone(),
                                mode_name: mode_name.clone(),
                                model: model.clone(),
                                tier,
                            };
                        } else {
                            error!("Structured output response missing 'transcription' field");
                            return CleanupOutcome::Cleaned {
                                text: strip_invisible_chars(&content),
                                mode_id: mode_id.clone(),
                                mode_name: mode_name.clone(),
                                model: model.clone(),
                                tier,
                            };
                        }
                    }
                    Err(e) => {
                        error!(
                            "Failed to parse structured output JSON: {}. Returning raw content.",
                            e
                        );
                        return CleanupOutcome::Cleaned {
                            text: strip_invisible_chars(&content),
                            mode_id: mode_id.clone(),
                            mode_name: mode_name.clone(),
                            model: model.clone(),
                            tier,
                        };
                    }
                }
            }
            Ok(None) => {
                error!("LLM API response has no content");
                return CleanupOutcome::Skipped;
            }
            Err(e) => {
                warn!(
                    "Structured output failed for provider '{}': {}. Falling back to legacy mode.",
                    provider.id, e
                );
                diagnostics::record_failure(app, categorize_llm_error(&provider.id, e));
                // Fall through to legacy mode below
            }
        }
    }

    // Legacy mode: send prompt to the model.
    //
    // For the local Ollama provider (custom), split into a system message
    // (instructions) and a separate user message (the transcription). Small local
    // models — e.g. phi4-mini — echo the instruction text back when it shares the
    // same message as the transcript, producing garbage output. Separating them,
    // and appending an explicit plain-prose constraint, fixes this.
    //
    // For other providers (anthropic, groq) keep the classic single-message
    // format: those services use larger models that do not have the echo problem,
    // and their behaviour with a combined message is already well-tested.
    let (legacy_user_content, legacy_system) = if provider.id == "custom" {
        // Strip ${output} and the trailing label line that introduced it (e.g.
        // "Input text to clean:"). The system message needs to be instructions
        // only; adding a dangling label or an extra constraint in the user message
        // confuses small models like phi4-mini, causing them to echo back the
        // instructions or explain why they are leaving the response empty.
        let instructions = {
            let stripped = prompt.replace("${output}", "");
            let trimmed = stripped.trim_end();
            // If the last non-empty line ends with ':', it was the label that
            // preceded ${output} — drop it so the system prompt ends on an
            // instruction, not a prompt fragment.
            if let Some(pos) = trimmed.rfind('\n') {
                let last_line = trimmed[pos + 1..].trim();
                if last_line.ends_with(':') {
                    trimmed[..pos].trim_end().to_string()
                } else {
                    trimmed.to_string()
                }
            } else {
                trimmed.to_string()
            }
        };
        // User message is the transcript alone — the system message already
        // constrains the output format (e.g. "Output ONLY the finalized text").
        (transcription.to_string(), Some(instructions))
    } else {
        (prompt.replace("${output}", transcription), None)
    };
    debug!("Processed prompt length: {} chars", legacy_user_content.len());

    match crate::llm_client::send_chat_completion_with_schema(
        &provider,
        api_key,
        &model,
        legacy_user_content,
        legacy_system,
        None,
        reasoning_effort,
        reasoning,
    )
    .await
    {
        Ok(Some(content)) => {
            let content = strip_invisible_chars(&content);
            debug!(
                "LLM post-processing succeeded for provider '{}'. Output length: {} chars",
                provider.id,
                content.len()
            );
            CleanupOutcome::Cleaned {
                text: content,
                mode_id,
                mode_name,
                model,
                tier,
            }
        }
        Ok(None) => {
            error!("LLM API response has no content");
            CleanupOutcome::Skipped
        }
        Err(e) => {
            error!(
                "LLM post-processing failed for provider '{}': {}. Falling back to original transcription.",
                provider.id,
                e
            );
            let category = categorize_llm_error(&provider.id, e);
            diagnostics::record_failure(app, category);
            CleanupOutcome::Failed(category)
        }
    }
}

async fn maybe_convert_chinese_variant(
    effective_language: &str,
    transcription: &str,
) -> Option<String> {
    // Gate on the language the model actually transcribed in (the effective
    // language), not the persisted intent. A leftover zh-Hans/zh-Hant intent
    // from a previously selected model must not run OpenCC S2T/T2S over output a
    // non-Chinese model produced — that would silently rewrite any shared CJK
    // characters (e.g. Japanese kanji) in the result.
    let is_simplified = effective_language == "zh-Hans";
    let is_traditional = effective_language == "zh-Hant";

    if !is_simplified && !is_traditional {
        debug!("effective language is not Simplified or Traditional Chinese; skipping conversion");
        return None;
    }

    debug!(
        "Starting Chinese variant conversion using OpenCC for language: {}",
        effective_language
    );

    // Use OpenCC to convert based on selected language
    let config = if is_simplified {
        // Convert Traditional Chinese to Simplified Chinese
        BuiltinConfig::Tw2sp
    } else {
        // Convert Simplified Chinese to Traditional Chinese
        BuiltinConfig::S2tw
    };

    match OpenCC::from_config(config) {
        Ok(converter) => {
            let converted = converter.convert(transcription);
            debug!(
                "OpenCC translation completed. Input length: {}, Output length: {}",
                transcription.len(),
                converted.len()
            );
            Some(converted)
        }
        Err(e) => {
            error!("Failed to initialize OpenCC converter: {}. Falling back to original transcription.", e);
            None
        }
    }
}

pub(crate) struct ProcessedTranscription {
    pub final_text: String,
    pub post_processed_text: Option<String>,
    pub post_process_prompt: Option<String>,
    /// The cleanup mode that ran (mode name · tier · model), when cleanup
    /// actually succeeded. `None` when cleanup was skipped or failed.
    pub cleanup_mode: Option<CleanupModeInfo>,
    /// The failure category when cleanup was requested but the LLM errored —
    /// the raw transcript is kept and pasted. `None` otherwise.
    pub cleanup_failure: Option<FailureCategory>,
}

impl ProcessedTranscription {
    /// Build the history cleanup columns (mode name/model/tier on success, or
    /// the failure category key) from this outcome.
    pub(crate) fn cleanup_record(&self) -> crate::managers::history::CleanupRecord {
        crate::managers::history::CleanupRecord {
            mode_id: self.cleanup_mode.as_ref().map(|m| m.id.clone()),
            mode_name: self.cleanup_mode.as_ref().map(|m| m.name.clone()),
            model: self.cleanup_mode.as_ref().map(|m| m.model.clone()),
            tier: self
                .cleanup_mode
                .as_ref()
                .and_then(|m| m.tier)
                .map(|t| cleanup_tier_key(t).to_string()),
            error: self.cleanup_failure.as_ref().map(cleanup_failure_key),
        }
    }
}

/// Resolve the persisted language *intent* into the language the currently-loaded
/// model will actually use — the same capability-aware coercion the transcription
/// paths apply (see [`crate::managers::model::effective_language`]). Post-processing
/// resolves it independently so it agrees with the language the transcription ran
/// in, without threading a value through the pipeline.
fn resolve_effective_language(app: &AppHandle, settings: &AppSettings) -> String {
    let tm = app.state::<Arc<TranscriptionManager>>();
    let model_manager = app.state::<Arc<ModelManager>>();
    let active_model = tm
        .get_current_model()
        .unwrap_or_else(|| settings.selected_model.clone());
    match model_manager.get_model_info(&active_model) {
        Some(info) => crate::managers::model::effective_language(
            &settings.selected_language,
            &info.supported_languages,
            info.supports_language_detection,
        ),
        None => settings.selected_language.clone(),
    }
}

pub(crate) async fn process_transcription_output(
    app: &AppHandle,
    transcription: &str,
    post_process: bool,
    context: Option<&crate::context_capture::ContextSnapshot>,
) -> ProcessedTranscription {
    let settings = get_settings(app);
    let mut final_text = transcription.to_string();
    let mut post_processed_text: Option<String> = None;
    let mut post_process_prompt: Option<String> = None;
    let mut cleanup_mode: Option<CleanupModeInfo> = None;
    let mut cleanup_failure: Option<FailureCategory> = None;

    // Resolve the language the transcription actually ran in (the persisted
    // intent coerced against the loaded model's capabilities) so OpenCC keys off
    // the effective language rather than a possibly-stale intent.
    let effective_language = resolve_effective_language(app, &settings);
    if let Some(converted_text) =
        maybe_convert_chinese_variant(&effective_language, transcription).await
    {
        final_text = converted_text;
    }

    if post_process {
        match post_process_transcription(app, &settings, &final_text, context).await {
            CleanupOutcome::Cleaned {
                text,
                mode_id,
                mode_name,
                model,
                tier,
            } => {
                post_processed_text = Some(text.clone());
                final_text = text;
                cleanup_mode = Some(CleanupModeInfo {
                    id: mode_id,
                    name: mode_name,
                    model,
                    tier,
                });

                if let Some(prompt_id) = &settings.post_process_selected_prompt_id {
                    if let Some(prompt) = settings
                        .post_process_prompts
                        .iter()
                        .find(|prompt| &prompt.id == prompt_id)
                    {
                        post_process_prompt = Some(prompt.prompt.clone());
                    }
                }
            }
            CleanupOutcome::Skipped => {}
            CleanupOutcome::Failed(category) => {
                cleanup_failure = Some(category);
            }
        }
    } else if final_text != transcription {
        post_processed_text = Some(final_text.clone());
    }

    // Snippets (Phase 6) expand verbatim into the pasted text, after any LLM
    // cleanup — expanding pre-LLM would let Rewrite/Email/Notes reword the
    // boilerplate the user asked to insert exactly. `post_processed_text` /
    // `post_process_prompt` (history/audit fields) intentionally keep showing
    // the pre-expansion cleanup output.
    if !settings.snippets.is_empty() {
        let pairs: Vec<(String, String)> = settings
            .snippets
            .iter()
            .map(|s| (s.trigger.clone(), s.expansion.clone()))
            .collect();
        final_text = crate::audio_toolkit::apply_snippets(&final_text, &pairs);
    }

    ProcessedTranscription {
        final_text,
        post_processed_text,
        post_process_prompt,
        cleanup_mode,
        cleanup_failure,
    }
}

impl ShortcutAction for TranscribeAction {
    fn start(&self, app: &AppHandle, binding_id: &str, _shortcut_str: &str) {
        let start_time = Instant::now();
        debug!("TranscribeAction::start called for binding: {}", binding_id);

        // Per-app auto Style rule (Phase 6), best-effort. Runs before the
        // overlay is shown so its subsequent settings read already sees the
        // (possibly rule-selected) Style — same main-thread-hop precedent as
        // context_capture::capture elsewhere in this file.
        crate::per_app_mode::apply_rule_for_frontmost_app(app);

        // Load model in the background
        let tm = app.state::<Arc<TranscriptionManager>>();
        let rm = app.state::<Arc<AudioRecordingManager>>();

        // Load ASR model and VAD model in parallel
        tm.initiate_model_load();
        let rm_clone = Arc::clone(&rm);
        std::thread::spawn(move || {
            if let Err(e) = rm_clone.preload_vad() {
                debug!("VAD pre-load failed: {}", e);
            }
        });

        let binding_id = binding_id.to_string();
        change_tray_icon(app, TrayIconState::Recording);

        // Get the microphone mode to determine audio feedback timing
        let settings = get_settings(app);
        let is_always_on = settings.always_on_microphone;

        // Warm the short-tier cleanup model while the user records. Ollama loads
        // it cold on first use otherwise, and that latency lands on the paste;
        // firing the load now overlaps it with recording + transcription. Only
        // for the local Ollama ("custom") provider with cleanup enabled — hosted
        // providers have no local model to load. We warm the short model because
        // it is the latency-sensitive common path; a long-tier run still loads
        // its model on demand at cleanup time.
        let will_clean = match self.post_process {
            PostProcessMode::FromSettings => settings.post_process_enabled,
            PostProcessMode::Force(clean) => clean,
        };
        if will_clean
            && settings
                .active_post_process_provider()
                .map(|p| p.id.as_str())
                == Some("custom")
            && !settings.short_model.trim().is_empty()
        {
            let model = settings.short_model.trim().to_string();
            tauri::async_runtime::spawn(async move {
                crate::ollama_setup::warm_model(model, "10m").await;
            });
        }

        let selected_model_info = app
            .state::<Arc<ModelManager>>()
            .get_model_info(&settings.selected_model);

        // Use the app-facing model capability as the single pre-recording source
        // for live streaming decisions. Unknown support is represented as false
        // until the model registry is updated by discovery or runtime load.
        let model_supports_streaming = selected_model_info
            .as_ref()
            .map(|m| m.supports_streaming)
            .unwrap_or(false);
        let vad_policy = if !settings.vad_enabled {
            VadPolicy::Disabled
        } else if model_supports_streaming {
            VadPolicy::Streaming
        } else {
            VadPolicy::Offline
        };
        if model_supports_streaming {
            tm.start_stream();
        }

        // Sizing the overlay follows the same advertised capability. A model that
        // doesn't stream (or whose capability is not known yet) gets the compact
        // pill instead of an oversized transparent live window.
        match settings.overlay_style {
            OverlayStyle::Live if model_supports_streaming => utils::show_streaming_overlay(app),
            OverlayStyle::Live | OverlayStyle::Minimal => show_recording_overlay(app),
            OverlayStyle::None => {} // show_overlay_state no-ops on None anyway
        }
        debug!("Microphone mode - always_on: {}", is_always_on);

        let mut recording_error: Option<String> = None;
        if is_always_on {
            // Always-on mode: Play audio feedback immediately, then apply mute after sound finishes
            debug!("Always-on mode: Playing audio feedback immediately");
            let rm_clone = Arc::clone(&rm);
            let app_clone = app.clone();
            // The blocking helper exits immediately if audio feedback is disabled,
            // so we can always reuse this thread to ensure mute happens right after playback.
            std::thread::spawn(move || {
                play_feedback_sound_blocking(&app_clone, SoundType::Start);
                rm_clone.apply_mute();
            });

            if let Err(e) = rm.try_start_recording(&binding_id, vad_policy) {
                debug!("Recording failed: {}", e);
                recording_error = Some(e);
            }
        } else {
            // On-demand mode: Start recording first, then play audio feedback, then apply mute
            // This allows the microphone to be activated before playing the sound
            debug!("On-demand mode: Starting recording first, then audio feedback");
            let recording_start_time = Instant::now();
            match rm.try_start_recording(&binding_id, vad_policy) {
                Ok(()) => {
                    debug!("Recording started in {:?}", recording_start_time.elapsed());
                    // Small delay to ensure microphone stream is active
                    let app_clone = app.clone();
                    let rm_clone = Arc::clone(&rm);
                    std::thread::spawn(move || {
                        std::thread::sleep(std::time::Duration::from_millis(100));
                        debug!("Handling delayed audio feedback/mute sequence");
                        // Helper handles disabled audio feedback by returning early, so we reuse it
                        // to keep mute sequencing consistent in every mode.
                        play_feedback_sound_blocking(&app_clone, SoundType::Start);
                        rm_clone.apply_mute();
                    });
                }
                Err(e) => {
                    debug!("Failed to start recording: {}", e);
                    recording_error = Some(e);
                }
            }
        }

        if recording_error.is_none() {
            // Dynamically register the cancel shortcut in a separate task to avoid deadlock
            shortcut::register_cancel_shortcut(app);
        } else {
            // Starting failed (for example due to blocked microphone permissions).
            // Revert UI state so we don't stay stuck in the recording overlay.
            tm.cancel_stream();
            utils::hide_recording_overlay(app);
            change_tray_icon(app, TrayIconState::Idle);
            if let Some(err) = recording_error {
                let error_type = if is_microphone_access_denied(&err) {
                    diagnostics::record_failure(app, FailureCategory::MicPermissionDenied);
                    "microphone_permission_denied"
                } else if is_no_input_device_error(&err) {
                    diagnostics::record_failure(app, FailureCategory::NoInputDevice);
                    "no_input_device"
                } else {
                    "unknown"
                };
                let _ = app.emit(
                    "recording-error",
                    RecordingErrorEvent {
                        error_type: error_type.to_string(),
                        detail: Some(err),
                    },
                );
            }
        }

        debug!(
            "TranscribeAction::start completed in {:?}",
            start_time.elapsed()
        );
    }

    fn stop(&self, app: &AppHandle, binding_id: &str, _shortcut_str: &str) {
        // Unregister the cancel shortcut when transcription stops
        shortcut::unregister_cancel_shortcut(app);

        let stop_time = Instant::now();
        debug!("TranscribeAction::stop called for binding: {}", binding_id);

        // Resolve cleanup up front: the single dictation hotkey follows the
        // `post_process_enabled` toggle; an explicit always-clean binding forces
        // it. Read once here so the whole stop path sees a stable decision.
        let post_process = match self.post_process {
            PostProcessMode::FromSettings => get_settings(app).post_process_enabled,
            PostProcessMode::Force(b) => b,
        };

        let ah = app.clone();
        let rm = Arc::clone(&app.state::<Arc<AudioRecordingManager>>());
        let tm = Arc::clone(&app.state::<Arc<TranscriptionManager>>());
        let hm = Arc::clone(&app.state::<Arc<HistoryManager>>());

        change_tray_icon(app, TrayIconState::Transcribing);
        // Stop should give immediate visual feedback. Live streaming can keep
        // the larger panel, but it still switches from listening to a working
        // spinner while the stream finalizes. Non-streaming paths use the
        // compact transcribing pill (None no-ops in show_*).
        let style = get_settings(app).overlay_style;
        match (style, tm.is_streaming()) {
            (OverlayStyle::Live, true) => {
                tm.emit_stream_working(StreamWorkKind::Transcribing);
            }
            _ => show_transcribing_overlay(app),
        }

        // Unmute before playing audio feedback so the stop sound is audible
        rm.remove_mute();

        // Play audio feedback for recording stop
        play_feedback_sound(app, SoundType::Stop);

        // Screen-context capture (Phase 7). Captured at stop time — the
        // selection/focused app are still the dictation target — and only
        // when every privacy gate passes: cleanup requested, global switch
        // on, local Custom/Ollama provider active, and the selected mode
        // opted in. We're on the coordinator thread here, so capture() may
        // block briefly for its main-thread dispatch.
        let context = {
            let settings = get_settings(app);
            let gated = post_process
                && settings.context_capture_enabled
                && settings
                    .active_post_process_provider()
                    .map(|p| p.id.as_str())
                    == Some("custom")
                && selected_mode_uses_context(&settings);
            if gated {
                crate::context_capture::capture(app)
            } else {
                None
            }
        };

        let binding_id = binding_id.to_string(); // Clone binding_id for the async task
        let cancel_generation = rm.cancel_generation();

        tauri::async_runtime::spawn(async move {
            let _guard = FinishGuard(ah.clone());
            debug!(
                "Starting async transcription task for binding: {}",
                binding_id
            );

            let stop_recording_time = Instant::now();
            if let Some(samples) = rm.stop_recording(&binding_id, cancel_generation) {
                debug!(
                    "Recording stopped and samples retrieved in {:?}, sample count: {}",
                    stop_recording_time.elapsed(),
                    samples.len()
                );

                if rm.was_cancelled_since(cancel_generation) {
                    debug!("Transcription operation cancelled after recording stop");
                    tm.cancel_stream();
                    utils::hide_recording_overlay(&ah);
                    change_tray_icon(&ah, TrayIconState::Idle);
                    return;
                }

                if samples.is_empty() {
                    debug!("Recording produced no audio samples; skipping persistence");
                    // Tear down any streaming worker so its channel doesn't leak
                    // and block the next start_stream.
                    tm.cancel_stream();
                    utils::hide_recording_overlay(&ah);
                    change_tray_icon(&ah, TrayIconState::Idle);
                } else {
                    // Save WAV concurrently with transcription
                    let sample_count = samples.len();
                    let file_name = format!("handy-{}.wav", chrono::Utc::now().timestamp());
                    let wav_path = hm.recordings_dir().join(&file_name);
                    let wav_path_for_verify = wav_path.clone();
                    let samples_for_wav = samples.clone();
                    let wav_handle = tauri::async_runtime::spawn_blocking(move || {
                        crate::audio_toolkit::save_wav_file(&wav_path, &samples_for_wav)
                    });

                    // Transcribe concurrently with WAV save. If a live stream was
                    // running, finalize it and use its text (all audio was already
                    // fed to the stream); otherwise batch-transcribe the samples.
                    let transcription_time = Instant::now();
                    let transcription_result = match tm.finalize_stream() {
                        // A finalized stream with usable text wins. An empty result
                        // (no active stream, produced nothing, or a finalize error
                        // after the engine was returned) falls back to a full batch
                        // transcription of the same audio. A finalize timeout is
                        // surfaced instead — the worker may still hold the engine,
                        // so a batch fallback would contend with it.
                        Ok(Some(text)) if !text.trim().is_empty() => Ok(text),
                        Ok(_) => tm.transcribe(samples),
                        Err(err) => Err(err),
                    };

                    // Await WAV save and verify
                    let wav_saved = match wav_handle.await {
                        Ok(Ok(())) => {
                            match crate::audio_toolkit::verify_wav_file(
                                &wav_path_for_verify,
                                sample_count,
                            ) {
                                Ok(()) => true,
                                Err(e) => {
                                    error!("WAV verification failed: {}", e);
                                    false
                                }
                            }
                        }
                        Ok(Err(e)) => {
                            error!("Failed to save WAV file: {}", e);
                            false
                        }
                        Err(e) => {
                            error!("WAV save task panicked: {}", e);
                            false
                        }
                    };

                    if rm.was_cancelled_since(cancel_generation) {
                        debug!("Transcription operation cancelled before output handling");
                        utils::hide_recording_overlay(&ah);
                        change_tray_icon(&ah, TrayIconState::Idle);
                        return;
                    }

                    match transcription_result {
                        Ok(transcription) => {
                            debug!(
                                "Transcription completed in {:?}: '{}'",
                                transcription_time.elapsed(),
                                transcription
                            );

                            // Set when the compact processing spinner was shown, so
                            // the paste path knows to flash a terminal state
                            // (Cleaned/Transcribed/Cleanup failed) before hiding.
                            let mut show_terminal_after = false;

                            if post_process {
                                // Show the cleanup spinner whenever an LLM cleanup
                                // will actually run (Short, Long, or non-tiered
                                // providers where tier is None). Only SkipLlm —
                                // built-in filters with no model call — stays
                                // instant, so its transcript pastes without a flash.
                                let tier = adaptive_tier(&get_settings(&ah), &transcription);
                                if !matches!(tier, Some(CleanupTier::SkipLlm)) {
                                    if style == OverlayStyle::Live {
                                        tm.emit_stream_working(StreamWorkKind::Polishing);
                                    } else {
                                        show_processing_overlay(&ah);
                                        // Compact overlay showed the spinner, so it
                                        // gets a terminal flash after paste. Only
                                        // SkipLlm stays instant (no model runs).
                                        show_terminal_after = true;
                                    }
                                }
                            }
                            let processed = process_transcription_output(
                                &ah,
                                &transcription,
                                post_process,
                                context.as_ref(),
                            )
                            .await;

                            if rm.was_cancelled_since(cancel_generation) {
                                debug!("Transcription operation cancelled before paste");
                                utils::hide_recording_overlay(&ah);
                                change_tray_icon(&ah, TrayIconState::Idle);
                                return;
                            }

                            // Save to history if WAV was saved
                            if wav_saved {
                                let cleanup = processed.cleanup_record();
                                if let Err(err) = hm.save_entry(
                                    file_name,
                                    transcription,
                                    post_process,
                                    processed.post_processed_text.clone(),
                                    processed.post_process_prompt.clone(),
                                    cleanup,
                                ) {
                                    error!("Failed to save history entry: {}", err);
                                }
                            }

                            // Which terminal state the compact overlay flashes
                            // (only when show_terminal_after): a hard failure
                            // wins, then a real cleanup, else a plain transcribe.
                            let terminal_kind = if processed.cleanup_failure.is_some() {
                                "failed"
                            } else if processed.cleanup_mode.is_some() {
                                "cleaned"
                            } else {
                                "transcribed"
                            };

                            if processed.final_text.is_empty() {
                                utils::hide_recording_overlay(&ah);
                                change_tray_icon(&ah, TrayIconState::Idle);
                            } else {
                                #[cfg(target_os = "macos")]
                                if !tauri_plugin_macos_permissions::check_accessibility_permission()
                                    .await
                                {
                                    diagnostics::record_failure(
                                        &ah,
                                        FailureCategory::AccessibilityMissing,
                                    );
                                }

                                let ah_clone = ah.clone();
                                let paste_time = Instant::now();
                                let final_text = processed.final_text;
                                let rm_for_paste = Arc::clone(&rm);
                                ah.run_on_main_thread(move || {
                                    if rm_for_paste.was_cancelled_since(cancel_generation) {
                                        debug!("Transcription operation cancelled before paste");
                                        utils::hide_recording_overlay(&ah_clone);
                                        change_tray_icon(&ah_clone, TrayIconState::Idle);
                                        return;
                                    }

                                    // Shape signal only: macOS already refuses
                                    // to deliver paste keystrokes into secure
                                    // fields, so this can't be verified after
                                    // the fact — just record that the target
                                    // looked secure right before the attempt.
                                    if crate::context_capture::is_focused_element_secure() {
                                        diagnostics::record_failure(
                                            &ah_clone,
                                            FailureCategory::PasteBlockedSecureField,
                                        );
                                    }

                                    match utils::paste(final_text, ah_clone.clone()) {
                                        Ok(()) => debug!(
                                            "Text pasted successfully in {:?}",
                                            paste_time.elapsed()
                                        ),
                                        Err(e) => {
                                            error!("Failed to paste transcription: {}", e);
                                            let _ = ah_clone.emit("paste-error", ());
                                        }
                                    }
                                    // Slow path (compact spinner was shown): flash
                                    // the terminal state, then hide after a beat on a
                                    // background thread so paste isn't blocked. Fast
                                    // paths hide immediately, keeping the instant feel.
                                    if show_terminal_after {
                                        utils::show_terminal_overlay(&ah_clone, terminal_kind);
                                        let ah_hide = ah_clone.clone();
                                        std::thread::spawn(move || {
                                            std::thread::sleep(std::time::Duration::from_millis(
                                                800,
                                            ));
                                            utils::hide_recording_overlay(&ah_hide);
                                        });
                                    } else {
                                        utils::hide_recording_overlay(&ah_clone);
                                    }
                                    change_tray_icon(&ah_clone, TrayIconState::Idle);
                                })
                                .unwrap_or_else(|e| {
                                    error!("Failed to run paste on main thread: {:?}", e);
                                    utils::hide_recording_overlay(&ah);
                                    change_tray_icon(&ah, TrayIconState::Idle);
                                });
                            }
                        }
                        Err(err) => {
                            if rm.was_cancelled_since(cancel_generation) {
                                debug!(
                                    "Transcription operation cancelled after transcription error"
                                );
                                utils::hide_recording_overlay(&ah);
                                change_tray_icon(&ah, TrayIconState::Idle);
                                return;
                            }

                            error!("Transcription failed: {}", err);
                            // Surface the failure to the UI (toast) as a
                            // category only — the raw engine message stays in
                            // handy.log via the line above, never in the event.
                            diagnostics::record_failure(&ah, FailureCategory::TranscriptionFailed);
                            let _ = ah.emit("transcription-error", ());
                            // Save entry with empty text so user can retry
                            if wav_saved {
                                if let Err(save_err) = hm.save_entry(
                                    file_name,
                                    String::new(),
                                    post_process,
                                    None,
                                    None,
                                    crate::managers::history::CleanupRecord::default(),
                                ) {
                                    error!("Failed to save failed history entry: {}", save_err);
                                }
                            }
                            utils::hide_recording_overlay(&ah);
                            change_tray_icon(&ah, TrayIconState::Idle);
                        }
                    }
                }
            } else {
                debug!("No samples retrieved from recording stop");
                // Tear down any streaming worker so its channel doesn't leak.
                tm.cancel_stream();
                utils::hide_recording_overlay(&ah);
                change_tray_icon(&ah, TrayIconState::Idle);
            }
        });

        debug!(
            "TranscribeAction::stop completed in {:?}",
            stop_time.elapsed()
        );
    }
}

// Cancel Action
struct CancelAction;

impl ShortcutAction for CancelAction {
    fn start(&self, app: &AppHandle, _binding_id: &str, _shortcut_str: &str) {
        utils::cancel_current_operation(app);
    }

    fn stop(&self, _app: &AppHandle, _binding_id: &str, _shortcut_str: &str) {
        // Nothing to do on stop for cancel
    }
}

/// Advances the selected cleanup mode to the next prompt in list order.
struct CycleModeAction;

impl ShortcutAction for CycleModeAction {
    fn start(&self, app: &AppHandle, _binding_id: &str, _shortcut_str: &str) {
        let settings = get_settings(app);
        if settings.post_process_prompts.len() < 2 {
            debug!("Cycle mode ignored: fewer than two modes configured");
            return;
        }
        let current = settings
            .post_process_selected_prompt_id
            .as_deref()
            .and_then(|id| {
                settings
                    .post_process_prompts
                    .iter()
                    .position(|p| p.id == id)
            })
            .unwrap_or(0);
        let next = (current + 1) % settings.post_process_prompts.len();
        let next_id = settings.post_process_prompts[next].id.clone();
        let next_name = settings.post_process_prompts[next].name.clone();
        // Routed through the same command the Settings dropdown/tray/overlay
        // Modes popover use — a hotkey cycle is a manual switch too, so it
        // must mark the manual-override flag the same way they do, or a
        // per-app rule could silently flip the mode right back on the next
        // app switch.
        match crate::shortcut::set_post_process_selected_prompt(app.clone(), next_id.clone()) {
            Ok(()) => debug!("Cycled cleanup mode to '{}' ({})", next_name, next_id),
            Err(e) => debug!("Cycle mode failed to apply '{}': {}", next_id, e),
        }
    }

    fn stop(&self, _app: &AppHandle, _binding_id: &str, _shortcut_str: &str) {}
}

// Test Action
struct TestAction;

impl ShortcutAction for TestAction {
    fn start(&self, app: &AppHandle, binding_id: &str, shortcut_str: &str) {
        log::info!(
            "Shortcut ID '{}': Started - {} (App: {})", // Changed "Pressed" to "Started" for consistency
            binding_id,
            shortcut_str,
            app.package_info().name
        );
    }

    fn stop(&self, app: &AppHandle, binding_id: &str, shortcut_str: &str) {
        log::info!(
            "Shortcut ID '{}': Stopped - {} (App: {})", // Changed "Released" to "Stopped" for consistency
            binding_id,
            shortcut_str,
            app.package_info().name
        );
    }
}

// Static Action Map
pub static ACTION_MAP: Lazy<HashMap<String, Arc<dyn ShortcutAction>>> = Lazy::new(|| {
    let mut map = HashMap::new();
    map.insert(
        "transcribe".to_string(),
        Arc::new(TranscribeAction {
            post_process: PostProcessMode::FromSettings,
        }) as Arc<dyn ShortcutAction>,
    );
    map.insert(
        "transcribe_with_post_process".to_string(),
        Arc::new(TranscribeAction {
            post_process: PostProcessMode::Force(true),
        }) as Arc<dyn ShortcutAction>,
    );
    map.insert(
        "cancel".to_string(),
        Arc::new(CancelAction) as Arc<dyn ShortcutAction>,
    );
    map.insert(
        "cycle_mode".to_string(),
        Arc::new(CycleModeAction) as Arc<dyn ShortcutAction>,
    );
    map.insert(
        "test".to_string(),
        Arc::new(TestAction) as Arc<dyn ShortcutAction>,
    );
    map
});

#[cfg(test)]
mod tests {
    use super::{build_style_guidance_block, is_blank_transcription};
    use crate::settings::get_default_settings;

    #[test]
    fn blank_transcription_is_detected() {
        assert!(is_blank_transcription(""));
        assert!(is_blank_transcription("   "));
        assert!(is_blank_transcription("\t\n  \r\n"));
    }

    #[test]
    fn non_blank_transcription_is_kept() {
        assert!(!is_blank_transcription("hello"));
        assert!(!is_blank_transcription("  hello  "));
    }

    #[test]
    fn style_guidance_block_absent_when_everything_off() {
        let mut settings = get_default_settings();
        settings.style_card_enabled = false;
        assert!(build_style_guidance_block(&settings).is_none());
    }

    #[test]
    fn style_guidance_block_includes_negative_constraints_by_default() {
        // style_card_enabled defaults to true.
        let settings = get_default_settings();
        let block = build_style_guidance_block(&settings).expect("should be Some");
        assert!(block.contains("em dashes"));
        assert!(block.contains("I hope this helps"));
    }

    #[test]
    fn style_guidance_block_includes_user_card_text() {
        let mut settings = get_default_settings();
        settings.style_card = "I always write in short, punchy sentences.".to_string();
        let block = build_style_guidance_block(&settings).expect("should be Some");
        assert!(block.contains("I always write in short, punchy sentences."));
    }
}

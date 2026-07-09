use log::{debug, warn};
use serde::de::{self, Visitor};
use serde::{Deserialize, Deserializer, Serialize};
use specta::Type;
use std::collections::HashMap;
use std::fmt;
use tauri::AppHandle;
use tauri_plugin_store::StoreExt;

#[derive(Serialize, Debug, Clone, Copy, PartialEq, Eq, Type)]
#[serde(rename_all = "lowercase")]
pub enum LogLevel {
    Trace,
    Debug,
    Info,
    Warn,
    Error,
}

// Custom deserializer to handle both old numeric format (1-5) and new string format ("trace", "debug", etc.)
impl<'de> Deserialize<'de> for LogLevel {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct LogLevelVisitor;

        impl<'de> Visitor<'de> for LogLevelVisitor {
            type Value = LogLevel;

            fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
                formatter.write_str("a string or integer representing log level")
            }

            fn visit_str<E: de::Error>(self, value: &str) -> Result<LogLevel, E> {
                match value.to_lowercase().as_str() {
                    "trace" => Ok(LogLevel::Trace),
                    "debug" => Ok(LogLevel::Debug),
                    "info" => Ok(LogLevel::Info),
                    "warn" => Ok(LogLevel::Warn),
                    "error" => Ok(LogLevel::Error),
                    _ => Err(E::unknown_variant(
                        value,
                        &["trace", "debug", "info", "warn", "error"],
                    )),
                }
            }

            fn visit_u64<E: de::Error>(self, value: u64) -> Result<LogLevel, E> {
                match value {
                    1 => Ok(LogLevel::Trace),
                    2 => Ok(LogLevel::Debug),
                    3 => Ok(LogLevel::Info),
                    4 => Ok(LogLevel::Warn),
                    5 => Ok(LogLevel::Error),
                    _ => Err(E::invalid_value(de::Unexpected::Unsigned(value), &"1-5")),
                }
            }
        }

        deserializer.deserialize_any(LogLevelVisitor)
    }
}

impl From<LogLevel> for tauri_plugin_log::LogLevel {
    fn from(level: LogLevel) -> Self {
        match level {
            LogLevel::Trace => tauri_plugin_log::LogLevel::Trace,
            LogLevel::Debug => tauri_plugin_log::LogLevel::Debug,
            LogLevel::Info => tauri_plugin_log::LogLevel::Info,
            LogLevel::Warn => tauri_plugin_log::LogLevel::Warn,
            LogLevel::Error => tauri_plugin_log::LogLevel::Error,
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone, Type)]
pub struct ShortcutBinding {
    pub id: String,
    pub name: String,
    pub description: String,
    pub default_binding: String,
    pub current_binding: String,
}

#[derive(Serialize, Deserialize, Debug, Clone, Type)]
pub struct LLMPrompt {
    pub id: String,
    pub name: String,
    pub prompt: String,
    /// Preferred model for this mode; falls back to the provider's configured
    /// model when unset. Adaptive length routing operates within the mode.
    #[serde(default)]
    pub model: Option<String>,
    /// Per-mode opt-in for screen-context capture (frontmost app, selected
    /// text, clipboard). Effective only when the global privacy switch
    /// (`context_capture_enabled`) is on and the local Custom/Ollama provider
    /// is active.
    #[serde(default)]
    pub use_context: bool,
}

/// A phrase-expansion shortcut applied to the final pasted text, after any LLM
/// cleanup (see `actions.rs::process_transcription_output`) — cleanup would
/// otherwise be free to reword an expanded snippet.
#[derive(Serialize, Deserialize, Debug, Clone, Type)]
pub struct Snippet {
    pub trigger: String,
    pub expansion: String,
}

/// Frontmost app name + bundle id, surfaced to Settings for a "use current
/// app" detect button when building a per-app mode rule.
#[derive(Serialize, Deserialize, Debug, Clone, Type)]
pub struct FrontmostAppInfo {
    pub name: String,
    pub bundle_id: String,
}

#[derive(Serialize, Deserialize, Debug, Clone, Type)]
pub struct PostProcessProvider {
    pub id: String,
    pub label: String,
    pub base_url: String,
    #[serde(default)]
    pub allow_base_url_edit: bool,
    #[serde(default)]
    pub models_endpoint: Option<String>,
    #[serde(default)]
    pub supports_structured_output: bool,
}

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, Type)]
#[serde(rename_all = "lowercase")]
pub enum OverlayPosition {
    Top,
    // `none` is retired: overlay visibility is owned by `OverlayStyle` now. The
    // alias keeps legacy stores (`"overlay_position": "none"`) deserializing
    // instead of failing the whole load; the one-time overlay migration reads the
    // raw stored string to recover the old "hidden" intent as `OverlayStyle::None`.
    #[serde(alias = "none")]
    Bottom,
}

/// Which recording overlay to display. `Minimal` and `Live` share one base
/// (the pill); `Live` grows into the panel that shows live transcription text.
/// `None` hides the overlay entirely. Decoupled from whether the model runs in
/// streaming mode (that is driven purely by model capability).
#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, Type)]
#[serde(rename_all = "lowercase")]
pub enum OverlayStyle {
    None,
    Minimal,
    Live,
}

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, Type, Default)]
#[serde(rename_all = "snake_case")]
pub enum ModelUnloadTimeout {
    Never,
    Immediately,
    Min2,
    #[default]
    Min5,
    Min10,
    Min15,
    Hour1,
    Sec15, // Debug mode only
}

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, Type)]
#[serde(rename_all = "snake_case")]
pub enum PasteMethod {
    CtrlV,
    Direct,
    None,
    ShiftInsert,
    CtrlShiftV,
    ExternalScript,
}

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, Type, Default)]
#[serde(rename_all = "snake_case")]
pub enum OllamaSetupStatus {
    #[default]
    NotAttempted,
    Skipped,
    Completed,
}

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, Type, Default)]
#[serde(rename_all = "snake_case")]
pub enum ClipboardHandling {
    #[default]
    DontModify,
    CopyToClipboard,
}

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, Type, Default)]
#[serde(rename_all = "snake_case")]
pub enum AutoSubmitKey {
    #[default]
    Enter,
    CtrlEnter,
    CmdEnter,
}

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, Type)]
#[serde(rename_all = "snake_case")]
pub enum RecordingRetentionPeriod {
    Never,
    PreserveLimit,
    Days3,
    Weeks2,
    Months3,
}

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, Type)]
#[serde(rename_all = "snake_case")]
pub enum KeyboardImplementation {
    Tauri,
    HandyKeys,
}

impl Default for KeyboardImplementation {
    fn default() -> Self {
        #[cfg(target_os = "linux")]
        return KeyboardImplementation::Tauri;
        #[cfg(not(target_os = "linux"))]
        return KeyboardImplementation::HandyKeys;
    }
}

impl Default for PasteMethod {
    fn default() -> Self {
        // Default to CtrlV for macOS and Windows, Direct for Linux
        #[cfg(target_os = "linux")]
        return PasteMethod::Direct;
        #[cfg(not(target_os = "linux"))]
        return PasteMethod::CtrlV;
    }
}

impl ModelUnloadTimeout {
    pub fn to_minutes(self) -> Option<u64> {
        match self {
            ModelUnloadTimeout::Never => None,
            ModelUnloadTimeout::Immediately => Some(0), // Special case for immediate unloading
            ModelUnloadTimeout::Min2 => Some(2),
            ModelUnloadTimeout::Min5 => Some(5),
            ModelUnloadTimeout::Min10 => Some(10),
            ModelUnloadTimeout::Min15 => Some(15),
            ModelUnloadTimeout::Hour1 => Some(60),
            ModelUnloadTimeout::Sec15 => Some(0), // Special case for debug - handled separately
        }
    }

    pub fn to_seconds(self) -> Option<u64> {
        match self {
            ModelUnloadTimeout::Never => None,
            ModelUnloadTimeout::Immediately => Some(0), // Special case for immediate unloading
            ModelUnloadTimeout::Sec15 => Some(15),
            _ => self.to_minutes().map(|m| m * 60),
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, Type)]
#[serde(rename_all = "snake_case")]
pub enum SoundTheme {
    Marimba,
    Pop,
    Custom,
}

impl SoundTheme {
    fn as_str(&self) -> &'static str {
        match self {
            SoundTheme::Marimba => "marimba",
            SoundTheme::Pop => "pop",
            SoundTheme::Custom => "custom",
        }
    }

    pub fn to_start_path(self) -> String {
        format!("resources/{}_start.wav", self.as_str())
    }

    pub fn to_stop_path(self) -> String {
        format!("resources/{}_stop.wav", self.as_str())
    }
}

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, Type, Default)]
#[serde(rename_all = "snake_case")]
pub enum TypingTool {
    #[default]
    Auto,
    Wtype,
    Kwtype,
    Dotool,
    Ydotool,
    Xdotool,
}

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, Type, Default)]
#[serde(rename_all = "snake_case")]
pub enum TranscribeAcceleratorSetting {
    #[default]
    Auto,
    Cpu,
    Gpu,
}

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, Type, Default)]
#[serde(rename_all = "snake_case")]
pub enum OrtAcceleratorSetting {
    #[default]
    Auto,
    Cpu,
    Cuda,
    #[serde(rename = "directml")]
    DirectMl,
    Rocm,
}

#[derive(Clone, Serialize, Deserialize, Type)]
#[serde(transparent)]
pub(crate) struct SecretMap(HashMap<String, String>);

impl fmt::Debug for SecretMap {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let redacted: HashMap<&String, &str> = self
            .0
            .iter()
            .map(|(k, v)| (k, if v.is_empty() { "" } else { "[REDACTED]" }))
            .collect();
        redacted.fmt(f)
    }
}

impl std::ops::Deref for SecretMap {
    type Target = HashMap<String, String>;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl std::ops::DerefMut for SecretMap {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

/* still handy for composing the initial JSON in the store ------------- */
#[derive(Serialize, Deserialize, Debug, Clone, Type)]
pub struct AppSettings {
    /// Internal settings schema marker for one-time migrations. Fresh installs
    /// start at the current version; existing stores missing this key are
    /// treated as version 0 and migrated forward.
    #[serde(default = "default_settings_schema_version")]
    pub settings_schema_version: u32,
    pub bindings: HashMap<String, ShortcutBinding>,
    pub push_to_talk: bool,
    pub audio_feedback: bool,
    #[serde(default = "default_audio_feedback_volume")]
    pub audio_feedback_volume: f32,
    #[serde(default = "default_sound_theme")]
    pub sound_theme: SoundTheme,
    #[serde(default = "default_start_hidden")]
    pub start_hidden: bool,
    #[serde(default = "default_autostart_enabled")]
    pub autostart_enabled: bool,
    #[serde(default = "default_update_checks_enabled")]
    pub update_checks_enabled: bool,
    #[serde(default = "default_show_whats_new_on_update")]
    pub show_whats_new_on_update: bool,
    /// The app version whose What's New the user has already seen. Fresh installs
    /// default to the current version (nothing is "new" to them). Existing users
    /// upgrading from before this key existed are blanked by the migration so they
    /// see the current release's notes — see `apply_settings_migrations`.
    #[serde(default = "default_whats_new_last_seen_version")]
    pub whats_new_last_seen_version: String,
    #[serde(default = "default_model")]
    pub selected_model: String,
    #[serde(default)]
    pub onboarding_completed: bool,
    /// Tracks the first-run Ollama wizard step separately from
    /// `onboarding_completed` so Settings can distinguish "never seen it" from
    /// "saw it and skipped" and keep offering a resume entry point either way.
    #[serde(default)]
    pub ollama_setup_status: OllamaSetupStatus,
    #[serde(default = "default_always_on_microphone")]
    pub always_on_microphone: bool,
    #[serde(default)]
    pub selected_microphone: Option<String>,
    #[serde(default)]
    pub clamshell_microphone: Option<String>,
    #[serde(default)]
    pub selected_output_device: Option<String>,
    #[serde(default = "default_translate_to_english")]
    pub translate_to_english: bool,
    #[serde(default = "default_selected_language")]
    pub selected_language: String,
    #[serde(default = "default_overlay_position")]
    pub overlay_position: OverlayPosition,
    #[serde(default = "default_debug_mode")]
    pub debug_mode: bool,
    #[serde(default = "default_log_level")]
    pub log_level: LogLevel,
    #[serde(default)]
    pub custom_words: Vec<String>,
    #[serde(default)]
    pub model_unload_timeout: ModelUnloadTimeout,
    #[serde(default = "default_word_correction_threshold")]
    pub word_correction_threshold: f64,
    #[serde(default = "default_history_limit")]
    pub history_limit: usize,
    #[serde(default = "default_recording_retention_period")]
    pub recording_retention_period: RecordingRetentionPeriod,
    #[serde(default)]
    pub paste_method: PasteMethod,
    #[serde(default)]
    pub clipboard_handling: ClipboardHandling,
    #[serde(default = "default_auto_submit")]
    pub auto_submit: bool,
    #[serde(default)]
    pub auto_submit_key: AutoSubmitKey,
    #[serde(default = "default_post_process_enabled")]
    pub post_process_enabled: bool,
    #[serde(default = "default_post_process_provider_id")]
    pub post_process_provider_id: String,
    #[serde(default = "default_post_process_providers")]
    pub post_process_providers: Vec<PostProcessProvider>,
    #[serde(default = "default_post_process_api_keys")]
    pub post_process_api_keys: SecretMap,
    #[serde(default = "default_post_process_models")]
    pub post_process_models: HashMap<String, String>,
    #[serde(default = "default_post_process_prompts")]
    pub post_process_prompts: Vec<LLMPrompt>,
    #[serde(default)]
    pub post_process_selected_prompt_id: Option<String>,
    /// The durable mode preference, written only by the manual mode-switch
    /// path (`set_post_process_selected_prompt`, shared by the Settings
    /// dropdown, tray, and overlay Modes popover). A per-app rule never
    /// writes this — only the runtime `post_process_selected_prompt_id`.
    #[serde(default)]
    pub default_mode_id: Option<String>,
    /// One-shot guard so `default_mode_id` is backfilled from the user's
    /// current mode (not reset to Clean up) exactly once when upgrading a
    /// store that predates this field.
    #[serde(default)]
    pub default_mode_id_seeded: bool,
    #[serde(default = "default_adaptive_cleanup")]
    pub adaptive_cleanup: bool,
    #[serde(default = "default_short_threshold_chars")]
    pub short_threshold_chars: u32,
    #[serde(default = "default_short_model")]
    pub short_model: String,
    #[serde(default = "default_long_model")]
    pub long_model: String,
    #[serde(default)]
    pub short_prompt_id: Option<String>,
    #[serde(default)]
    pub long_prompt_id: Option<String>,
    #[serde(default)]
    pub skip_llm_under_chars: u32,
    #[serde(default)]
    pub mute_while_recording: bool,
    #[serde(default)]
    pub append_trailing_space: bool,
    #[serde(default = "default_app_language")]
    pub app_language: String,
    #[serde(default)]
    pub lazy_stream_close: bool,
    #[serde(default)]
    pub keyboard_implementation: KeyboardImplementation,
    #[serde(default = "default_show_tray_icon")]
    pub show_tray_icon: bool,
    #[serde(default = "default_paste_delay_ms")]
    pub paste_delay_ms: u64,
    #[serde(default = "default_typing_tool")]
    pub typing_tool: TypingTool,
    pub external_script_path: Option<String>,
    #[serde(default)]
    pub custom_filler_words: Option<Vec<String>>,
    #[serde(default)]
    pub transcribe_accelerator: TranscribeAcceleratorSetting,
    #[serde(default)]
    pub ort_accelerator: OrtAcceleratorSetting,
    #[serde(default = "default_transcribe_gpu_device")]
    pub transcribe_gpu_device: i32,
    #[serde(default)]
    pub extra_recording_buffer_ms: u64,
    #[serde(default = "default_vad_enabled")]
    pub vad_enabled: bool,
    /// Which recording overlay to show: None / Minimal / Live. Streaming mode is
    /// not gated on this — that follows model capability. Migrated from the old
    /// `overlay_position` (position `none` → style `None`).
    #[serde(default = "default_overlay_style")]
    pub overlay_style: OverlayStyle,
    /// Keep a small idle pill visible when not recording (hover shows the
    /// Modes / Record / Expand controls). Ignored when overlay_style is None.
    #[serde(default = "default_overlay_always_show")]
    pub overlay_always_show: bool,
    /// One-shot guard so the default mode prompts are seeded into existing
    /// stores exactly once (a deleted mode must stay deleted).
    #[serde(default)]
    pub modes_seeded: bool,
    /// Global privacy switch for screen-context capture (frontmost app,
    /// selected text, clipboard). Default OFF; context is only ever fed to
    /// the local Custom/Ollama provider and only its shape is logged.
    #[serde(default)]
    pub context_capture_enabled: bool,
    /// One-shot guard for seeding the Context mode into stores that already
    /// ran the Phase 5 `modes_seeded` pass (a deleted mode must stay deleted).
    #[serde(default)]
    pub context_mode_seeded: bool,
    /// One-shot guard for the single-hotkey migration: clears any existing
    /// `transcribe_with_post_process` binding exactly once, so upgrading users
    /// land on the single dictation hotkey + cleanup toggle model without a
    /// stale second key still firing cleanup.
    #[serde(default)]
    pub single_hotkey_migrated: bool,
    /// Phase 6: phrase-expansion shortcuts (e.g. "my email" -> an address),
    /// applied verbatim to the pasted text after any LLM cleanup.
    #[serde(default)]
    pub snippets: Vec<Snippet>,
    /// Free-text "Voice" card injected into cleanup prompts alongside the
    /// fixed negative-AI-isms constraints, gated by `style_card_enabled`.
    /// Global and always-on (not a per-mode picker) — skipped on the fast/3b
    /// tier at the actions.rs call site regardless of this flag.
    #[serde(default = "default_style_card")]
    pub style_card: String,
    /// Master toggle for the Voice guidance block (negative constraints +
    /// style card). On by default — it's a quality guardrail, not a feature
    /// the user has to discover.
    #[serde(default = "default_style_card_enabled")]
    pub style_card_enabled: bool,
    /// Feature default OFF per the build plan: per-app mode auto-selection
    /// is opt-in, never silent AI-inferred switching.
    #[serde(default)]
    pub per_app_auto_mode_enabled: bool,
    /// bundle_id -> mode_id (a `post_process_prompts` entry). Maps a
    /// frontmost app to a complete mode, the primary control now that modes
    /// bundle edit-strength + tone + format + model tier together.
    #[serde(default)]
    pub per_app_mode_map: HashMap<String, String>,
}

fn default_model() -> String {
    "".to_string()
}

const CURRENT_SETTINGS_SCHEMA_VERSION: u32 = 3;

fn default_settings_schema_version() -> u32 {
    CURRENT_SETTINGS_SCHEMA_VERSION
}

fn default_always_on_microphone() -> bool {
    false
}

fn default_translate_to_english() -> bool {
    false
}

fn default_start_hidden() -> bool {
    false
}

fn default_autostart_enabled() -> bool {
    false
}

fn default_update_checks_enabled() -> bool {
    true
}

fn default_show_whats_new_on_update() -> bool {
    true
}

fn default_whats_new_last_seen_version() -> String {
    env!("CARGO_PKG_VERSION").to_string()
}

fn default_selected_language() -> String {
    "auto".to_string()
}

fn default_overlay_position() -> OverlayPosition {
    // Position only matters when the overlay is shown; whether it shows at all is
    // `overlay_style` (Linux defaults that to None). So a single default suffices.
    OverlayPosition::Bottom
}

fn default_overlay_style() -> OverlayStyle {
    // Linux hides the overlay by default; other platforms show the minimal pill,
    // which expands to Live on hover. Position is independent and only selects
    // top vs. bottom placement.
    #[cfg(target_os = "linux")]
    return OverlayStyle::None;
    #[cfg(not(target_os = "linux"))]
    return OverlayStyle::Minimal;
}

fn default_vad_enabled() -> bool {
    true
}

fn default_debug_mode() -> bool {
    false
}

fn default_log_level() -> LogLevel {
    LogLevel::Debug
}

fn default_word_correction_threshold() -> f64 {
    0.18
}

fn default_paste_delay_ms() -> u64 {
    60
}

fn default_auto_submit() -> bool {
    false
}

fn default_history_limit() -> usize {
    5
}

fn default_recording_retention_period() -> RecordingRetentionPeriod {
    RecordingRetentionPeriod::PreserveLimit
}

fn default_audio_feedback_volume() -> f32 {
    1.0
}

fn default_sound_theme() -> SoundTheme {
    SoundTheme::Marimba
}

fn default_post_process_enabled() -> bool {
    false
}

fn default_adaptive_cleanup() -> bool {
    true
}

fn default_overlay_always_show() -> bool {
    true
}

fn default_short_threshold_chars() -> u32 {
    150
}

fn default_short_model() -> String {
    // Apple Silicon gets the MLX-optimized tag (Metal-accelerated); every other
    // platform (Windows, Linux, Intel Mac) uses the portable standard tag.
    #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
    return "qwen3.5:2b-mlx".to_string();
    #[cfg(not(all(target_os = "macos", target_arch = "aarch64")))]
    return "qwen3.5:2b".to_string();
}

pub(crate) fn default_long_model() -> String {
    #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
    return "gemma4:12b-mlx".to_string();
    #[cfg(not(all(target_os = "macos", target_arch = "aarch64")))]
    return "gemma4:12b".to_string();
}

fn default_app_language() -> String {
    tauri_plugin_os::locale()
        .map(|l| l.replace('_', "-"))
        .unwrap_or_else(|| "en".to_string())
}

fn default_show_tray_icon() -> bool {
    true
}

fn default_post_process_provider_id() -> String {
    "openai".to_string()
}

fn default_post_process_providers() -> Vec<PostProcessProvider> {
    let mut providers = vec![
        PostProcessProvider {
            id: "openai".to_string(),
            label: "OpenAI".to_string(),
            base_url: "https://api.openai.com/v1".to_string(),
            allow_base_url_edit: false,
            models_endpoint: Some("/models".to_string()),
            supports_structured_output: true,
        },
        PostProcessProvider {
            id: "zai".to_string(),
            label: "Z.AI".to_string(),
            base_url: "https://api.z.ai/api/paas/v4".to_string(),
            allow_base_url_edit: false,
            models_endpoint: Some("/models".to_string()),
            supports_structured_output: true,
        },
        PostProcessProvider {
            id: "openrouter".to_string(),
            label: "OpenRouter".to_string(),
            base_url: "https://openrouter.ai/api/v1".to_string(),
            allow_base_url_edit: false,
            models_endpoint: Some("/models".to_string()),
            supports_structured_output: true,
        },
        PostProcessProvider {
            id: "anthropic".to_string(),
            label: "Anthropic".to_string(),
            base_url: "https://api.anthropic.com/v1".to_string(),
            allow_base_url_edit: false,
            models_endpoint: Some("/models".to_string()),
            supports_structured_output: false,
        },
        PostProcessProvider {
            id: "groq".to_string(),
            label: "Groq".to_string(),
            base_url: "https://api.groq.com/openai/v1".to_string(),
            allow_base_url_edit: false,
            models_endpoint: Some("/models".to_string()),
            supports_structured_output: false,
        },
        PostProcessProvider {
            id: "cerebras".to_string(),
            label: "Cerebras".to_string(),
            base_url: "https://api.cerebras.ai/v1".to_string(),
            allow_base_url_edit: false,
            models_endpoint: Some("/models".to_string()),
            supports_structured_output: true,
        },
    ];

    // AWS Bedrock via Mantle (OpenAI-compatible endpoint)
    providers.push(PostProcessProvider {
        id: "bedrock_mantle".to_string(),
        label: "AWS Bedrock (Mantle)".to_string(),
        base_url: "https://bedrock-mantle.us-east-1.api.aws/v1".to_string(),
        allow_base_url_edit: false,
        models_endpoint: Some("/models".to_string()),
        supports_structured_output: true,
    });

    // Custom provider always comes last
    providers.push(PostProcessProvider {
        id: "custom".to_string(),
        label: "Custom".to_string(),
        base_url: "http://localhost:11434/v1".to_string(),
        allow_base_url_edit: true,
        models_endpoint: Some("/models".to_string()),
        supports_structured_output: false,
    });

    providers
}

fn default_post_process_api_keys() -> SecretMap {
    let mut map = HashMap::new();
    for provider in default_post_process_providers() {
        map.insert(provider.id, String::new());
    }
    SecretMap(map)
}

fn default_model_for_provider(_provider_id: &str) -> String {
    String::new()
}

fn default_post_process_models() -> HashMap<String, String> {
    let mut map = HashMap::new();
    for provider in default_post_process_providers() {
        map.insert(
            provider.id.clone(),
            default_model_for_provider(&provider.id),
        );
    }
    map
}

/// Handy's original shipped default prompt. Kept so `ensure_post_process_defaults`
/// can retire it from existing stores only when the user never edited it.
pub(crate) const LEGACY_DEFAULT_PROMPT_ID: &str = "default_improve_transcriptions";
pub(crate) const LEGACY_DEFAULT_PROMPT_TEXT: &str = "Clean this transcript:\n1. Fix spelling, capitalization, and punctuation errors\n2. Convert number words to digits (twenty-five → 25, ten percent → 10%, five dollars → $5)\n3. Replace spoken punctuation with symbols (period → ., comma → ,, question mark → ?)\n4. Remove filler words (um, uh, like as filler)\n5. Keep the language in the original version (if it was french, keep it in french for example)\n\nPreserve exact meaning and word order. Do not paraphrase or reorder content.\n\nReturn only the cleaned transcript.\n\nTranscript:\n${output}";

pub(crate) const DEFAULT_MODE_ID: &str = "mode_short_dictation";

/// The adaptive tier modes back the length-based default cleanup (short input
/// -> Short Dictation, long input -> Long Dictation). They are protected:
/// editable but never deletable, since the default path depends on them.
pub(crate) const SHORT_DICTATION_MODE_ID: &str = "mode_short_dictation";
pub(crate) const LONG_DICTATION_MODE_ID: &str = "mode_long_dictation";
pub(crate) const PROTECTED_MODE_IDS: [&str; 2] =
    [SHORT_DICTATION_MODE_ID, LONG_DICTATION_MODE_ID];

/// Default modes. The first two (Short/Long Dictation) are the adaptive tiers
/// used by length when no per-app rule matches. The rest are per-app category
/// modes a user maps applications to; a matching per-app rule overrides the
/// length tier regardless of dictation length.
fn default_post_process_prompts() -> Vec<LLMPrompt> {
    vec![
        LLMPrompt {
            id: SHORT_DICTATION_MODE_ID.to_string(),
            name: "Short Dictation".to_string(),
            prompt: "You clean up short spoken dictation. Fix spelling, typos, missing punctuation, and capitalization, and drop filler words like um, uh, like, and you know along with accidental repeated words. Keep the speaker's own wording, slang, and jargon exactly. Never paraphrase, reword, or make it sound more formal. Return the text as one continuous line with no line breaks, lists, or headings. Respond with nothing but the cleaned text: no preamble, no explanation, no quotation marks, no sign-off.\n\nInput text to clean:\n\n${output}".to_string(),
            model: None,
            use_context: false,
        },
        LLMPrompt {
            id: LONG_DICTATION_MODE_ID.to_string(),
            name: "Long Dictation".to_string(),
            prompt: "You clean up longer spoken dictation. Fix spelling, typos, punctuation, and capitalization, and drop filler words like um, uh, like, and you know along with accidental repeated words. Keep the speaker's own wording, slang, and jargon. Fix the mechanics; never rewrite sentences to sound like an assistant wrote them. Break the text into clear paragraphs so it reads well, but do not add headings, bullet points, or summaries. Respond with nothing but the cleaned text: no preamble, no explanation, no quotation marks, no sign-off.\n\nInput text to clean:\n\n${output}".to_string(),
            model: None,
            use_context: false,
        },
        LLMPrompt {
            id: "mode_communication".to_string(),
            name: "Communication Apps".to_string(),
            prompt: "You turn raw dictation into a clear, natural message ready to send in chat or email. Fix all grammar, spelling, and punctuation, drop filler words and false starts, and organize it into readable sentences and short paragraphs. Keep it sounding like a real person, not a corporate template. Do not open with filler like 'I hope this email finds you well', and avoid words like delve, testament, vibrant, crucial, revolutionize, and underscore. Do not use em dashes as connectors, do not pad ideas into groups of three, and do not add negative-parallelism phrasing like 'not just X, but Y'. Do not invent names, greetings, or signatures the speaker did not say. Respond with nothing but the finished message: no preamble, no explanation, no quotation marks.\n\nInput text to clean:\n\n${output}".to_string(),
            model: None,
            use_context: false,
        },
        LLMPrompt {
            id: "mode_notes_apps".to_string(),
            name: "Notes Apps".to_string(),
            prompt: "You turn dictated thoughts into a clean, scannable note. Fix spelling, grammar, and punctuation, and keep the speaker's exact vocabulary and technical details without summarizing anything away. If the dictation clearly covers distinct topics, group them under short Markdown headings (## or ###) and turn listed items into bullet points. If it is just a quick thought, leave it as plain sentences without imposing structure. If the speaker names things to do, collect them under a final '### Action Items' section. Respond with nothing but the note itself: no preamble, no explanation, no commentary.\n\nInput text to clean:\n\n${output}".to_string(),
            model: None,
            use_context: false,
        },
        LLMPrompt {
            id: "mode_technical_apps".to_string(),
            name: "Technical Apps".to_string(),
            prompt: "You make minimal corrections to technical dictation such as notes, code-adjacent descriptions, or issue tickets. Fix only clear spelling mistakes, typos, and missing punctuation. Do not paraphrase, reword, reorder, or make anything sound more polished; keep the exact phrasing and casual style. Preserve every code snippet, variable name (camelCase, snake_case), file path, URL, key, bracket, and symbol exactly as spoken or implied. Do not add headings, bullets, paragraphs, greetings, or closing remarks. Respond with nothing but the corrected text: no preamble, no explanation, no quotation marks.\n\nInput text to clean:\n\n${output}".to_string(),
            model: None,
            use_context: false,
        },
    ]
}

/// Prompt text each built-in mode shipped with before the v2 settings
/// migration. Used to detect an unedited default so the migration can refresh
/// it in place without overwriting a prompt the user customized. Do not change
/// these strings — they are historical fingerprints, not live defaults.
fn pre_v2_default_prompt(mode_id: &str) -> Option<&'static str> {
    Some(match mode_id {
        SHORT_DICTATION_MODE_ID => "You are a precision text cleaner. Fix speech-to-text artifacts and grammatical errors while preserving the user's exact phrasing and voice.\n\nRULES:\n1. Fix obvious spelling mistakes, typos, missing punctuation, and capitalization errors.\n2. Delete spoken filler words (e.g., \"um\", \"uh\", \"like\", \"you know\") and accidental word repetitions.\n3. Keep all original vocabulary, slang, or jargon. Do NOT paraphrase or smooth out the style.\n4. Keep the output as a single, continuous line. Do NOT introduce line breaks, lists, or paragraph transitions.\n5. Output ONLY the finalized text. Absolutely no chat or explanations.\n\nInput text to clean:\n\n${output}",
        LONG_DICTATION_MODE_ID => "You are a precision text cleaner. Fix speech-to-text artifacts and grammatical errors while preserving the user's exact phrasing and voice.\n\nRULES:\n1. Fix obvious spelling mistakes, typos, missing punctuation, and capitalization errors.\n2. Delete spoken filler words (e.g., \"um\", \"uh\", \"like\", \"you know\") and accidental word repetitions.\n3. Keep all original vocabulary, slang, or jargon. Do NOT rewrite sentences to sound like an AI.\n4. Organize the text into logical paragraphs to ensure readability. Break up long, continuous walls of text into distinct thoughts.\n5. Output ONLY the finalized text. Absolutely no chat or explanations.\n\nInput text to clean:\n\n${output}",
        "mode_communication" => "You are an expert communication editor. Your sole task is to clean up raw dictation into clear, professional, and natural-sounding messaging.\n\nCRITICAL CONSTRAINTS:\n1. Fix all grammar, spelling, typos, and syntax errors.\n2. Remove filler words (um, uh, like, you know) and accidental speech repetitions.\n3. Organize the text into logical, readable paragraphs if it is long.\n4. Maintain a natural human tone. Avoid robotic or overly formal \"AI cliches\" (do NOT start with \"I hope this email finds you well\" or use words like \"delve\", \"testament\", or \"revolutionize\").\n5. If the input implies an email or message structure, ensure clear transitions but do not invent fake names or signatures.\n6. Output ONLY the finalized message. Never include introductory text, explanations, or wrap-up commentary.\n\nInput text to clean:\n\n${output}",
        "mode_notes_apps" => "You are a structural organization engine. Your task is to process unorganized dictation thoughts and map them into clear, scannable Markdown documentation.\n\nCRITICAL CONSTRAINTS:\n1. Fix all underlying spelling and grammatical errors.\n2. Categorize loose thoughts logically using clean Markdown formatting.\n3. Use bold headers (## or ###) for primary themes or distinct topics mentioned.\n4. Convert itemized thoughts or descriptions into clean bullet points.\n5. If actionable items or tasks are detected in the speech, extract them into a dedicated section titled \"### Action Items\" at the bottom.\n6. Preserve the exact vocabulary and technical context used by the speaker—do not summarize away critical details.\n7. Output ONLY the formatted markdown document. Do not include chat greetings, system commentary, or explanations.\n\nInput text to clean:\n\n${output}",
        "mode_technical_apps" => "You are a precision technical text corrector. Your task is to apply surgical grammatical edits to technical notes, code-adjacent descriptions, or issue tickets.\n\nCRITICAL CONSTRAINTS:\n1. Fix spelling, immediate punctuation errors, and clear typos only.\n2. Do NOT smooth out the tone, do NOT paraphrase, and do NOT rewrite sentences to sound more elegant or professional. Preserve the exact phrasing and colloquial style.\n3. Strictly preserve all code snippets, variable names (e.g., camelCase, snake_case), database keys, URL paths, bracket types, and technical symbols exactly as written or implied.\n4. Do not insert formatting elements, paragraphs, or lists unless explicitly requested in the spoken text.\n5. Never add polite phrases, introductory filler, or concluding remarks.\n6. Output ONLY the strictly corrected raw text.\n\nInput text to clean:\n\n${output}",
        _ => return None,
    })
}

fn default_style_card() -> String {
    "Write the way I actually talk: plain, direct sentences. Match the register of what I said instead of dressing it up.".to_string()
}

fn default_style_card_enabled() -> bool {
    true
}

fn default_transcribe_gpu_device() -> i32 {
    -1 // auto
}

fn default_typing_tool() -> TypingTool {
    TypingTool::Auto
}

fn ensure_post_process_defaults(settings: &mut AppSettings) -> bool {
    let mut changed = false;

    // Retire the delisted Apple Intelligence provider from existing stores: it
    // was removed from the default list and is no longer functional (the SDK is
    // not linked). Drop it from the persisted providers, keys, and models, and
    // move the selection to the default provider if it was pointed at it.
    let before = settings.post_process_providers.len();
    settings
        .post_process_providers
        .retain(|p| p.id != "apple_intelligence");
    if settings.post_process_providers.len() != before {
        settings.post_process_api_keys.0.remove("apple_intelligence");
        settings.post_process_models.remove("apple_intelligence");
        if settings.post_process_provider_id == "apple_intelligence" {
            settings.post_process_provider_id = default_post_process_provider_id();
        }
        changed = true;
    }

    for provider in default_post_process_providers() {
        // Use match to do a single lookup - either sync existing or add new
        match settings
            .post_process_providers
            .iter_mut()
            .find(|p| p.id == provider.id)
        {
            Some(existing) => {
                // Sync supports_structured_output field for existing providers (migration)
                if existing.supports_structured_output != provider.supports_structured_output {
                    debug!(
                        "Updating supports_structured_output for provider '{}' from {} to {}",
                        provider.id,
                        existing.supports_structured_output,
                        provider.supports_structured_output
                    );
                    existing.supports_structured_output = provider.supports_structured_output;
                    changed = true;
                }
            }
            None => {
                // Provider doesn't exist, add it
                settings.post_process_providers.push(provider.clone());
                changed = true;
            }
        }

        if !settings.post_process_api_keys.contains_key(&provider.id) {
            settings
                .post_process_api_keys
                .insert(provider.id.clone(), String::new());
            changed = true;
        }

        let default_model = default_model_for_provider(&provider.id);
        match settings.post_process_models.get_mut(&provider.id) {
            Some(existing) => {
                if existing.is_empty() && !default_model.is_empty() {
                    *existing = default_model.clone();
                    changed = true;
                }
            }
            None => {
                settings
                    .post_process_models
                    .insert(provider.id.clone(), default_model);
                changed = true;
            }
        }
    }

    // Seed the default mode prompts exactly once per store (modes_seeded guards
    // re-adding a mode the user later deleted). Retire Handy's legacy default
    // prompt only when its text was never edited, and move the selection to
    // Light Edit when it is unset or still pointing at the legacy default.
    if !settings.modes_seeded {
        settings.post_process_prompts.retain(|p| {
            !(p.id == LEGACY_DEFAULT_PROMPT_ID && p.prompt == LEGACY_DEFAULT_PROMPT_TEXT)
        });

        for mode in default_post_process_prompts() {
            if !settings
                .post_process_prompts
                .iter()
                .any(|p| p.id == mode.id)
            {
                settings.post_process_prompts.push(mode);
            }
        }

        let selected_is_stale = match &settings.post_process_selected_prompt_id {
            None => true,
            Some(id) => id == LEGACY_DEFAULT_PROMPT_ID,
        };
        if selected_is_stale {
            settings.post_process_selected_prompt_id = Some(DEFAULT_MODE_ID.to_string());
        }

        settings.modes_seeded = true;
        changed = true;
    }

    // Backfill the durable mode default exactly once, from whatever mode is
    // currently selected (not a reset to Clean up) — runs after the
    // modes_seeded fixup above so a stale legacy selection has already been
    // resolved to a real mode id before it's captured as the default.
    if !settings.default_mode_id_seeded {
        if settings.default_mode_id.is_none() {
            settings.default_mode_id = settings.post_process_selected_prompt_id.clone();
        }
        settings.default_mode_id_seeded = true;
        changed = true;
    }

    // Mode redesign migration: retire the pre-redesign default modes and
    // guarantee the protected adaptive tier modes exist. Idempotent, so it can
    // safely run on every load; the per-app category modes are seeded once via
    // the modes_seeded block above so a user deletion sticks.
    const RETIRED_MODE_IDS: [&str; 7] = [
        "mode_light_edit",
        "mode_rewrite",
        "mode_email",
        "mode_message",
        "mode_notes",
        "mode_verbatim",
        "mode_context",
    ];
    let before_len = settings.post_process_prompts.len();
    settings
        .post_process_prompts
        .retain(|p| !RETIRED_MODE_IDS.contains(&p.id.as_str()));
    for mode in default_post_process_prompts() {
        let protected = PROTECTED_MODE_IDS.contains(&mode.id.as_str());
        let present = settings.post_process_prompts.iter().any(|p| p.id == mode.id);
        if protected && !present {
            settings.post_process_prompts.push(mode);
        }
    }
    if settings
        .post_process_selected_prompt_id
        .as_deref()
        .is_none_or(|id| RETIRED_MODE_IDS.contains(&id))
    {
        settings.post_process_selected_prompt_id = Some(DEFAULT_MODE_ID.to_string());
    }
    if settings
        .default_mode_id
        .as_deref()
        .is_some_and(|id| RETIRED_MODE_IDS.contains(&id))
    {
        settings.default_mode_id = Some(DEFAULT_MODE_ID.to_string());
    }
    if settings
        .short_prompt_id
        .as_deref()
        .is_none_or(|id| RETIRED_MODE_IDS.contains(&id))
    {
        settings.short_prompt_id = Some(SHORT_DICTATION_MODE_ID.to_string());
    }
    if settings
        .long_prompt_id
        .as_deref()
        .is_none_or(|id| RETIRED_MODE_IDS.contains(&id))
    {
        settings.long_prompt_id = Some(LONG_DICTATION_MODE_ID.to_string());
    }
    if settings.post_process_prompts.len() != before_len {
        changed = true;
    }

    // Enforce non-empty adaptive tier models: they are the model selection for
    // the local provider, so an empty one would leave cleanup model-less.
    if settings.short_model.trim().is_empty() {
        settings.short_model = default_short_model();
        changed = true;
    }
    if settings.long_model.trim().is_empty() {
        settings.long_model = default_long_model();
        changed = true;
    }

    changed
}

pub const SETTINGS_STORE_PATH: &str = "settings_store.json";

pub fn get_default_settings() -> AppSettings {
    #[cfg(target_os = "windows")]
    let default_shortcut = "ctrl+space";
    #[cfg(target_os = "macos")]
    let default_shortcut = "option+space";
    #[cfg(target_os = "linux")]
    let default_shortcut = "ctrl+space";
    #[cfg(not(any(target_os = "windows", target_os = "macos", target_os = "linux")))]
    let default_shortcut = "alt+space";

    let mut bindings = HashMap::new();
    bindings.insert(
        "transcribe".to_string(),
        ShortcutBinding {
            id: "transcribe".to_string(),
            name: "Transcribe".to_string(),
            description: "Converts your speech into text.".to_string(),
            default_binding: default_shortcut.to_string(),
            current_binding: default_shortcut.to_string(),
        },
    );
    #[cfg(target_os = "windows")]
    let default_post_process_shortcut = "ctrl+shift+space";
    #[cfg(target_os = "macos")]
    let default_post_process_shortcut = "option+shift+space";
    #[cfg(target_os = "linux")]
    let default_post_process_shortcut = "ctrl+shift+space";
    #[cfg(not(any(target_os = "windows", target_os = "macos", target_os = "linux")))]
    let default_post_process_shortcut = "alt+shift+space";

    bindings.insert(
        "transcribe_with_post_process".to_string(),
        ShortcutBinding {
            id: "transcribe_with_post_process".to_string(),
            name: "Transcribe with Cleanup".to_string(),
            description: "Converts your speech into text and applies AI cleanup.".to_string(),
            // Unbound by default. Cleanup is now a behavior toggle on the single
            // dictation hotkey (see `post_process_enabled`), so this dedicated
            // always-clean binding is an optional power-user extra. The default
            // binding string is retained below for anyone who binds it later.
            default_binding: default_post_process_shortcut.to_string(),
            current_binding: "".to_string(),
        },
    );
    bindings.insert(
        "cancel".to_string(),
        ShortcutBinding {
            id: "cancel".to_string(),
            name: "Cancel".to_string(),
            description: "Cancels the current recording.".to_string(),
            default_binding: "escape".to_string(),
            current_binding: "escape".to_string(),
        },
    );
    bindings.insert(
        "cycle_mode".to_string(),
        ShortcutBinding {
            id: "cycle_mode".to_string(),
            name: "Cycle Mode".to_string(),
            description: "Switches to the next cleanup mode.".to_string(),
            default_binding: "".to_string(),
            current_binding: "".to_string(),
        },
    );

    AppSettings {
        settings_schema_version: default_settings_schema_version(),
        bindings,
        push_to_talk: true,
        audio_feedback: false,
        audio_feedback_volume: default_audio_feedback_volume(),
        sound_theme: default_sound_theme(),
        start_hidden: default_start_hidden(),
        autostart_enabled: default_autostart_enabled(),
        update_checks_enabled: default_update_checks_enabled(),
        show_whats_new_on_update: default_show_whats_new_on_update(),
        whats_new_last_seen_version: default_whats_new_last_seen_version(),
        selected_model: "".to_string(),
        onboarding_completed: false,
        ollama_setup_status: OllamaSetupStatus::NotAttempted,
        always_on_microphone: false,
        selected_microphone: None,
        clamshell_microphone: None,
        selected_output_device: None,
        translate_to_english: false,
        selected_language: "auto".to_string(),
        overlay_position: default_overlay_position(),
        debug_mode: false,
        log_level: default_log_level(),
        custom_words: Vec::new(),
        model_unload_timeout: ModelUnloadTimeout::default(),
        word_correction_threshold: default_word_correction_threshold(),
        history_limit: default_history_limit(),
        recording_retention_period: default_recording_retention_period(),
        paste_method: PasteMethod::default(),
        clipboard_handling: ClipboardHandling::default(),
        auto_submit: default_auto_submit(),
        auto_submit_key: AutoSubmitKey::default(),
        post_process_enabled: default_post_process_enabled(),
        post_process_provider_id: default_post_process_provider_id(),
        post_process_providers: default_post_process_providers(),
        post_process_api_keys: default_post_process_api_keys(),
        post_process_models: default_post_process_models(),
        post_process_prompts: default_post_process_prompts(),
        post_process_selected_prompt_id: Some(DEFAULT_MODE_ID.to_string()),
        // Zero-config default: a fresh install already has a sensible durable
        // default (Clean up) with Voice on, so default_mode_id is never None
        // for a new user — only an upgrading store waits on the
        // default_mode_id_seeded backfill above.
        default_mode_id: Some(DEFAULT_MODE_ID.to_string()),
        default_mode_id_seeded: true,
        adaptive_cleanup: default_adaptive_cleanup(),
        short_threshold_chars: default_short_threshold_chars(),
        short_model: default_short_model(),
        long_model: default_long_model(),
        short_prompt_id: Some(SHORT_DICTATION_MODE_ID.to_string()),
        long_prompt_id: Some(LONG_DICTATION_MODE_ID.to_string()),
        skip_llm_under_chars: 0,
        mute_while_recording: false,
        append_trailing_space: false,
        app_language: default_app_language(),
        lazy_stream_close: false,
        keyboard_implementation: KeyboardImplementation::default(),
        show_tray_icon: default_show_tray_icon(),
        paste_delay_ms: default_paste_delay_ms(),
        typing_tool: default_typing_tool(),
        external_script_path: None,
        custom_filler_words: None,
        transcribe_accelerator: TranscribeAcceleratorSetting::default(),
        ort_accelerator: OrtAcceleratorSetting::default(),
        transcribe_gpu_device: default_transcribe_gpu_device(),
        extra_recording_buffer_ms: 0,
        vad_enabled: default_vad_enabled(),
        overlay_style: default_overlay_style(),
        overlay_always_show: default_overlay_always_show(),
        modes_seeded: true,
        context_capture_enabled: false,
        context_mode_seeded: true,
        single_hotkey_migrated: true,
        snippets: Vec::new(),
        style_card: default_style_card(),
        style_card_enabled: default_style_card_enabled(),
        per_app_auto_mode_enabled: false,
        per_app_mode_map: HashMap::new(),
    }
}

impl AppSettings {
    pub fn active_post_process_provider(&self) -> Option<&PostProcessProvider> {
        self.post_process_providers
            .iter()
            .find(|provider| provider.id == self.post_process_provider_id)
    }

    pub fn post_process_provider(&self, provider_id: &str) -> Option<&PostProcessProvider> {
        self.post_process_providers
            .iter()
            .find(|provider| provider.id == provider_id)
    }

    pub fn post_process_provider_mut(
        &mut self,
        provider_id: &str,
    ) -> Option<&mut PostProcessProvider> {
        self.post_process_providers
            .iter_mut()
            .find(|provider| provider.id == provider_id)
    }
}

pub fn load_or_create_app_settings(app: &AppHandle) -> AppSettings {
    // Initialize store
    let store = app
        .store(crate::portable::store_path(SETTINGS_STORE_PATH))
        .expect("Failed to initialize store");

    let mut settings = if let Some(settings_value) = store.get("settings") {
        // Parse the entire settings object
        match serde_json::from_value::<AppSettings>(settings_value.clone()) {
            Ok(mut settings) => {
                debug!("Found existing settings: {:?}", settings);
                let default_settings = get_default_settings();
                let mut updated = apply_settings_migrations(&mut settings, &settings_value);

                // Merge default bindings into existing settings
                for (key, value) in default_settings.bindings {
                    if let std::collections::hash_map::Entry::Vacant(entry) =
                        settings.bindings.entry(key)
                    {
                        debug!("Adding missing binding: {}", entry.key());
                        entry.insert(value);
                        updated = true;
                    }
                }

                if updated {
                    debug!("Settings updated with defaults/migrations");
                    store.set("settings", serde_json::to_value(&settings).unwrap());
                }

                settings
            }
            Err(e) => {
                warn!("Failed to parse settings: {}", e);
                // Fall back to default settings if parsing fails
                let default_settings = get_default_settings();
                store.set("settings", serde_json::to_value(&default_settings).unwrap());
                default_settings
            }
        }
    } else {
        let default_settings = get_default_settings();
        store.set("settings", serde_json::to_value(&default_settings).unwrap());
        default_settings
    };

    // Order matters: ensure_post_process_defaults may backfill default_mode_id
    // from the current selection (see default_mode_id_seeded) — that backfill
    // must land before the reset below, or the reset would immediately
    // clobber the user's current mode with a not-yet-backfilled default.
    let mut changed = ensure_post_process_defaults(&mut settings);
    changed |= reset_selected_mode_to_default(&mut settings);

    if changed {
        store.set("settings", serde_json::to_value(&settings).unwrap());
    }

    settings
}

/// Every launch starts the runtime-active mode from the durable default — a
/// per-app rule may move `post_process_selected_prompt_id` away from
/// `default_mode_id` during a session, but that must never persist across
/// restarts, or an auto-selected mode from the last app the user happened to
/// be in would silently become the new baseline.
fn reset_selected_mode_to_default(settings: &mut AppSettings) -> bool {
    if settings.post_process_selected_prompt_id != settings.default_mode_id {
        settings.post_process_selected_prompt_id = settings.default_mode_id.clone();
        true
    } else {
        false
    }
}

pub fn get_settings(app: &AppHandle) -> AppSettings {
    let store = app
        .store(crate::portable::store_path(SETTINGS_STORE_PATH))
        .expect("Failed to initialize store");

    // Settings reads also persist one-time migrations. Migration helpers are
    // idempotent, so this converges after the first read of an older store.
    let mut settings = if let Some(settings_value) = store.get("settings") {
        match serde_json::from_value::<AppSettings>(settings_value.clone()) {
            Ok(mut settings) => {
                if apply_settings_migrations(&mut settings, &settings_value) {
                    store.set("settings", serde_json::to_value(&settings).unwrap());
                }
                settings
            }
            Err(_) => {
                let default_settings = get_default_settings();
                store.set("settings", serde_json::to_value(&default_settings).unwrap());
                default_settings
            }
        }
    } else {
        let default_settings = get_default_settings();
        store.set("settings", serde_json::to_value(&default_settings).unwrap());
        default_settings
    };

    if ensure_post_process_defaults(&mut settings) {
        store.set("settings", serde_json::to_value(&settings).unwrap());
    }

    settings
}

fn apply_settings_migrations(
    settings: &mut AppSettings,
    settings_value: &serde_json::Value,
) -> bool {
    let mut updated = false;

    // One-time onboarding migration: users with an explicit selected model have
    // already made it through model selection. Users who merely have compatible
    // files on disk should still see onboarding.
    if settings_value.get("onboarding_completed").is_none() {
        settings.onboarding_completed = !settings.selected_model.is_empty();
        updated = true;
    }

    // One-time What's New migration: migrations only run on an existing store
    // (fresh installs stamp the current version via get_default_settings). A
    // missing key here means a user upgrading from before it existed — blank it
    // so they see the current release's What's New, mirroring the onboarding
    // migration's explicit first-run-vs-upgrade decision.
    if settings_value.get("whats_new_last_seen_version").is_none() {
        settings.whats_new_last_seen_version = String::new();
        updated = true;
    }

    let stored_schema_version = settings_value
        .get("settings_schema_version")
        .and_then(|v| v.as_u64())
        .unwrap_or(0);
    if stored_schema_version < 1 {
        // `transcribe_gpu_device` used to be a UI ordinal; it is now a
        // transcribe.cpp registry index. A positive legacy value can point at a
        // different GPU after CPU/accelerator/backend devices are included in
        // the registry, so reset ambiguous explicit selections to Auto once.
        if settings.transcribe_gpu_device > 0 {
            settings.transcribe_accelerator = TranscribeAcceleratorSetting::Auto;
            settings.transcribe_gpu_device = default_transcribe_gpu_device();
        }
        settings.settings_schema_version = CURRENT_SETTINGS_SCHEMA_VERSION;
        updated = true;
    }

    // v2 settings migration: push the refreshed cleanup defaults to existing
    // users. Only values still matching the pre-v2 shipped defaults are
    // replaced, so a user's custom model choice or edited mode prompt is never
    // clobbered. Fresh installs already carry the new defaults and skip this.
    if stored_schema_version < 2 {
        const OLD_SHORT_MODEL: &str = "phi4-mini:latest";
        const OLD_LONG_MODEL: &str = "gemma3:12b";
        if settings.short_model.trim() == OLD_SHORT_MODEL {
            settings.short_model = default_short_model();
        }
        if settings.long_model.trim() == OLD_LONG_MODEL {
            settings.long_model = default_long_model();
        }
        let new_defaults = default_post_process_prompts();
        for stored in settings.post_process_prompts.iter_mut() {
            let Some(old_prompt) = pre_v2_default_prompt(&stored.id) else {
                continue;
            };
            // Leave any mode whose prompt the user has edited untouched.
            if stored.prompt != old_prompt {
                continue;
            }
            if let Some(new_mode) = new_defaults.iter().find(|m| m.id == stored.id) {
                stored.prompt = new_mode.prompt.clone();
                // Refresh the pinned model only when it is still the pre-v2
                // default (the Technical mode pinned phi4-mini); a custom pin stays.
                if stored.model.as_deref() == Some(OLD_SHORT_MODEL) {
                    stored.model = new_mode.model.clone();
                }
            }
        }
        settings.settings_schema_version = CURRENT_SETTINGS_SCHEMA_VERSION;
        updated = true;
    }

    // v3 settings migration: modes now carry only a prompt; the model is chosen
    // by the length tier (short_model / long_model). The Technical mode was the
    // only default that pinned a model (the short model). Clear that pin for
    // existing users so long technical dictation can route to the long model,
    // but only when the pin still equals the default short model — a model the
    // user pinned by hand stays. Runs after the v2 block, which for a pre-v2
    // user has already refreshed the Technical pin to the current short model.
    if stored_schema_version < 3 {
        for stored in settings.post_process_prompts.iter_mut() {
            if stored.id == "mode_technical_apps"
                && stored.model.as_deref() == Some(default_short_model().as_str())
            {
                stored.model = None;
            }
        }
        settings.settings_schema_version = CURRENT_SETTINGS_SCHEMA_VERSION;
        updated = true;
    }

    // One-time overlay migration (only while the new key is absent): the retired
    // overlay_position `none` meant "hide the overlay" → OverlayStyle::None; any
    // other position had it visible → Live. The position enum no longer has a
    // `none` variant (legacy "none" deserializes to Bottom via a serde alias), so
    // read the raw stored string to recover the old intent.
    if settings_value.get("overlay_style").is_none() {
        let was_hidden = settings_value
            .get("overlay_position")
            .and_then(|v| v.as_str())
            == Some("none");
        settings.overlay_style = if was_hidden {
            OverlayStyle::None
        } else {
            OverlayStyle::Live
        };
        updated = true;
    }

    // The keyboard backend is now auto-selected per platform; the retired
    // Experimental dropdown no longer exists. Ignore any persisted override and
    // force the platform default, persisting it so every read stays consistent.
    let platform_default = KeyboardImplementation::default();
    if settings.keyboard_implementation != platform_default {
        settings.keyboard_implementation = platform_default;
        updated = true;
    }

    // One-time single-hotkey migration: cleanup is now a behavior toggle on the
    // single dictation hotkey, not a separate key. Clear any existing
    // `transcribe_with_post_process` binding exactly once so an upgrading user's
    // stale second key (which could overlap the transcribe key and double-fire)
    // stops triggering cleanup. Guarded so a user who deliberately rebinds it
    // later keeps that choice.
    if !settings.single_hotkey_migrated {
        if let Some(binding) = settings.bindings.get_mut("transcribe_with_post_process") {
            if !binding.current_binding.trim().is_empty() {
                binding.current_binding = String::new();
            }
        }
        settings.single_hotkey_migrated = true;
        updated = true;
    }

    updated
}

pub fn write_settings(app: &AppHandle, settings: AppSettings) {
    let store = app
        .store(crate::portable::store_path(SETTINGS_STORE_PATH))
        .expect("Failed to initialize store");

    store.set("settings", serde_json::to_value(&settings).unwrap());
}

pub fn get_bindings(app: &AppHandle) -> HashMap<String, ShortcutBinding> {
    let settings = get_settings(app);

    settings.bindings
}

pub fn get_stored_binding(app: &AppHandle, id: &str) -> ShortcutBinding {
    let bindings = get_bindings(app);

    let binding = bindings.get(id).unwrap().clone();

    binding
}

pub fn get_history_limit(app: &AppHandle) -> usize {
    let settings = get_settings(app);
    settings.history_limit
}

pub fn get_recording_retention_period(app: &AppHandle) -> RecordingRetentionPeriod {
    let settings = get_settings(app);
    settings.recording_retention_period
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_settings_disable_auto_submit() {
        let settings = get_default_settings();
        assert!(!settings.auto_submit);
        assert_eq!(settings.auto_submit_key, AutoSubmitKey::Enter);
        assert_eq!(
            settings.settings_schema_version,
            CURRENT_SETTINGS_SCHEMA_VERSION
        );
    }

    #[test]
    fn phase6_fields_default_correctly_from_legacy_store_missing_them() {
        // A store persisted before Phase 6 existed has none of these keys —
        // deserializing it must still produce sane, working defaults, the
        // same backward-compat contract every #[serde(default)] field here
        // relies on (no explicit migration/seeding flag needed).
        let mut raw = serde_json::to_value(get_default_settings()).unwrap();
        let obj = raw.as_object_mut().unwrap();
        for key in [
            "snippets",
            "style_card",
            "style_card_enabled",
            "per_app_auto_mode_enabled",
            "per_app_mode_map",
            "default_mode_id",
            "default_mode_id_seeded",
        ] {
            obj.remove(key);
        }
        let settings: AppSettings =
            serde_json::from_value(raw).expect("legacy store should still deserialize");

        assert!(settings.snippets.is_empty());
        assert!(settings.style_card_enabled);
        assert!(!settings.style_card.trim().is_empty());
        assert!(!settings.per_app_auto_mode_enabled);
        assert!(settings.per_app_mode_map.is_empty());
        assert_eq!(settings.default_mode_id, None);
        assert!(!settings.default_mode_id_seeded);
    }

    #[test]
    fn fresh_install_has_a_default_selected_mode() {
        // Zero-config default: a new user gets a sensible mode with Voice on,
        // never a None that would leave cleanup with nothing selected.
        let settings = get_default_settings();
        assert_eq!(settings.default_mode_id, Some(DEFAULT_MODE_ID.to_string()));
        assert_eq!(
            settings.post_process_selected_prompt_id,
            Some(DEFAULT_MODE_ID.to_string())
        );
        assert!(settings.style_card_enabled);
    }

    #[test]
    fn default_mode_id_backfills_from_current_selection_not_reset_to_clean_up() {
        // An upgrading store had no default_mode_id but did have a selected
        // mode the user had actually picked (Communication Apps); the backfill
        // must preserve that choice, not silently reset it to the default.
        let mut settings = get_default_settings();
        settings.post_process_selected_prompt_id = Some("mode_communication".to_string());
        settings.default_mode_id = None;
        settings.default_mode_id_seeded = false;

        assert!(ensure_post_process_defaults(&mut settings));
        assert_eq!(
            settings.default_mode_id,
            Some("mode_communication".to_string())
        );
        assert!(settings.default_mode_id_seeded);
    }

    #[test]
    fn v2_migration_refreshes_unedited_defaults_but_keeps_customizations() {
        // A pre-v2 store on the old models, with one built-in mode still on its
        // shipped default prompt and one the user has edited.
        let mut settings = get_default_settings();
        settings.short_model = "phi4-mini:latest".to_string();
        settings.long_model = "gemma3:12b".to_string();

        let refreshed_id = SHORT_DICTATION_MODE_ID.to_string();
        let edited_id = "mode_communication".to_string();
        let edited_prompt = "MY OWN PROMPT ${output}".to_string();
        for prompt in settings.post_process_prompts.iter_mut() {
            if prompt.id == refreshed_id {
                prompt.prompt = pre_v2_default_prompt(&refreshed_id).unwrap().to_string();
            } else if prompt.id == edited_id {
                prompt.prompt = edited_prompt.clone();
            }
        }

        let raw = serde_json::json!({ "settings_schema_version": 1 });
        assert!(apply_settings_migrations(&mut settings, &raw));

        // Old model defaults are refreshed to the current ones.
        assert_eq!(settings.short_model, default_short_model());
        assert_eq!(settings.long_model, default_long_model());

        // The unedited default prompt is refreshed to the current default.
        let expected_prompt = default_post_process_prompts()
            .into_iter()
            .find(|p| p.id == refreshed_id)
            .unwrap()
            .prompt;
        let refreshed = settings
            .post_process_prompts
            .iter()
            .find(|p| p.id == refreshed_id)
            .unwrap();
        assert_eq!(refreshed.prompt, expected_prompt);

        // The user's edited prompt is left untouched.
        let edited = settings
            .post_process_prompts
            .iter()
            .find(|p| p.id == edited_id)
            .unwrap();
        assert_eq!(edited.prompt, edited_prompt);

        assert_eq!(
            settings.settings_schema_version,
            CURRENT_SETTINGS_SCHEMA_VERSION
        );
    }

    #[test]
    fn v3_migration_clears_default_technical_model_pin() {
        // A v2 store whose Technical mode still carries the default short-model
        // pin. v3 drops it so long technical dictation routes to the long model.
        let mut settings = get_default_settings();
        for prompt in settings.post_process_prompts.iter_mut() {
            if prompt.id == "mode_technical_apps" {
                prompt.model = Some(default_short_model());
            }
        }

        let raw = serde_json::json!({ "settings_schema_version": 2 });
        assert!(apply_settings_migrations(&mut settings, &raw));

        let technical = settings
            .post_process_prompts
            .iter()
            .find(|p| p.id == "mode_technical_apps")
            .unwrap();
        assert_eq!(technical.model, None);
        assert_eq!(
            settings.settings_schema_version,
            CURRENT_SETTINGS_SCHEMA_VERSION
        );
    }

    #[test]
    fn v3_migration_keeps_hand_pinned_technical_model() {
        // A model the user pinned by hand must survive the v3 pin clear.
        let mut settings = get_default_settings();
        for prompt in settings.post_process_prompts.iter_mut() {
            if prompt.id == "mode_technical_apps" {
                prompt.model = Some("my-custom-model:latest".to_string());
            }
        }

        let raw = serde_json::json!({ "settings_schema_version": 2 });
        assert!(apply_settings_migrations(&mut settings, &raw));

        let technical = settings
            .post_process_prompts
            .iter()
            .find(|p| p.id == "mode_technical_apps")
            .unwrap();
        assert_eq!(technical.model.as_deref(), Some("my-custom-model:latest"));
    }

    #[test]
    fn selected_mode_resets_to_default_on_launch() {
        let mut settings = get_default_settings();
        settings.default_mode_id = Some("mode_rewrite".to_string());
        // Simulates a per-app rule having moved the runtime value last
        // session — it must not survive a relaunch.
        settings.post_process_selected_prompt_id = Some("mode_message".to_string());

        assert!(reset_selected_mode_to_default(&mut settings));
        assert_eq!(
            settings.post_process_selected_prompt_id,
            Some("mode_rewrite".to_string())
        );
        // Idempotent once already in sync.
        assert!(!reset_selected_mode_to_default(&mut settings));
    }

    #[cfg(not(target_os = "linux"))]
    #[test]
    fn default_overlay_style_is_minimal_when_overlay_defaults_on() {
        let settings = get_default_settings();
        assert_eq!(settings.overlay_style, OverlayStyle::Minimal);
    }

    #[test]
    fn overlay_migration_keeps_disabled_overlay_off() {
        let mut settings = get_default_settings();

        // Legacy store: overlay was hidden via the retired position "none".
        let raw = serde_json::json!({
            "selected_model": "",
            "overlay_position": "none"
        });

        assert!(apply_settings_migrations(&mut settings, &raw));
        assert_eq!(settings.overlay_style, OverlayStyle::None);
    }

    #[test]
    fn legacy_none_overlay_position_deserializes_to_bottom() {
        // A persisted "none" must not fail the whole settings load; the serde
        // alias folds it onto Bottom (visibility is owned by overlay_style).
        let raw = serde_json::json!({ "overlay_position": "none" });
        let position: OverlayPosition =
            serde_json::from_value(raw.get("overlay_position").unwrap().clone())
                .expect("legacy \"none\" should deserialize, not error");
        assert_eq!(position, OverlayPosition::Bottom);
    }

    #[test]
    fn overlay_migration_promotes_enabled_overlay_to_live() {
        let mut settings = get_default_settings();
        settings.overlay_position = OverlayPosition::Top;
        settings.overlay_style = OverlayStyle::Minimal;

        let raw = serde_json::json!({
            "selected_model": "",
            "overlay_position": "top"
        });

        assert!(apply_settings_migrations(&mut settings, &raw));
        assert_eq!(settings.overlay_style, OverlayStyle::Live);
        assert_eq!(settings.overlay_position, OverlayPosition::Top);
    }

    #[test]
    fn gpu_device_migration_resets_legacy_positive_selection_to_auto() {
        let mut settings = get_default_settings();
        settings.transcribe_accelerator = TranscribeAcceleratorSetting::Gpu;
        settings.transcribe_gpu_device = 2;

        let raw = serde_json::json!({
            "transcribe_accelerator": "gpu",
            "transcribe_gpu_device": 2
        });

        assert!(apply_settings_migrations(&mut settings, &raw));
        assert_eq!(
            settings.transcribe_accelerator,
            TranscribeAcceleratorSetting::Auto
        );
        assert_eq!(
            settings.transcribe_gpu_device,
            default_transcribe_gpu_device()
        );
        assert_eq!(
            settings.settings_schema_version,
            CURRENT_SETTINGS_SCHEMA_VERSION
        );
    }

    #[test]
    fn gpu_device_migration_keeps_current_schema_positive_selection() {
        let mut settings = get_default_settings();
        settings.transcribe_accelerator = TranscribeAcceleratorSetting::Gpu;
        settings.transcribe_gpu_device = 2;

        let raw = serde_json::json!({
            "settings_schema_version": CURRENT_SETTINGS_SCHEMA_VERSION,
            "onboarding_completed": false,
            "whats_new_last_seen_version": default_whats_new_last_seen_version(),
            "overlay_style": "live",
            "transcribe_accelerator": "gpu",
            "transcribe_gpu_device": 2
        });

        assert!(!apply_settings_migrations(&mut settings, &raw));
        assert_eq!(
            settings.transcribe_accelerator,
            TranscribeAcceleratorSetting::Gpu
        );
        assert_eq!(settings.transcribe_gpu_device, 2);
    }

    #[test]
    fn debug_output_redacts_api_keys() {
        let mut settings = get_default_settings();
        settings
            .post_process_api_keys
            .insert("openai".to_string(), "sk-proj-secret-key-12345".to_string());
        settings.post_process_api_keys.insert(
            "anthropic".to_string(),
            "sk-ant-secret-key-67890".to_string(),
        );
        settings
            .post_process_api_keys
            .insert("empty_provider".to_string(), "".to_string());

        let debug_output = format!("{:?}", settings);

        assert!(!debug_output.contains("sk-proj-secret-key-12345"));
        assert!(!debug_output.contains("sk-ant-secret-key-67890"));
        assert!(debug_output.contains("[REDACTED]"));
    }

    #[test]
    fn secret_map_debug_redacts_values() {
        let map = SecretMap(HashMap::from([("key".into(), "secret".into())]));
        let out = format!("{:?}", map);
        assert!(!out.contains("secret"));
        assert!(out.contains("[REDACTED]"));
    }
}

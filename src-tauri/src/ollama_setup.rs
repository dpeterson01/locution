//! First-run setup helper for local Ollama cleanup (Phase 8).
//!
//! Best-effort install/start/pull orchestration for the two adaptive-cleanup
//! tier models (see `settings::default_short_model` / `default_long_model`).
//! Every function here is designed to fail soft: callers report status back
//! to the onboarding wizard, which always offers "skip" — nothing here ever
//! blocks first run.

use futures_util::StreamExt;
use log::debug;
use serde::{Deserialize, Serialize};
use specta::Type;
use std::process::{Command, Stdio};
use std::time::Duration;
use tauri::{AppHandle, Emitter};
use tokio::time::sleep;

/// Ollama's native API (NOT the OpenAI-compat `/v1` surface the "custom"
/// post-process provider talks to) — cheaper existence probe and the only
/// surface that exposes `/api/pull` and `/api/tags`.
pub const OLLAMA_NATIVE_BASE_URL: &str = "http://localhost:11434";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum OllamaAvailability {
    NotInstalled,
    InstalledNotRunning,
    Running,
}

#[derive(Debug, Clone, Serialize, Type)]
pub struct OllamaProbeResult {
    pub availability: OllamaAvailability,
    pub brew_available: bool,
    /// Every model name Ollama currently has pulled (not just the two
    /// adaptive-cleanup tier models — used for arbitrary membership checks,
    /// e.g. "is this mode's typed-in model already present").
    pub models_present: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Type)]
#[serde(tag = "kind", content = "detail", rename_all = "snake_case")]
pub enum OllamaSetupError {
    NoNetwork,
    BrewAbsent,
    DiskFull(String),
    Interrupted(String),
    Other(String),
}

impl std::fmt::Display for OllamaSetupError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            OllamaSetupError::NoNetwork => write!(f, "no network connection"),
            OllamaSetupError::BrewAbsent => write!(f, "Homebrew is not installed"),
            OllamaSetupError::DiskFull(msg) => write!(f, "disk full: {msg}"),
            OllamaSetupError::Interrupted(msg) => write!(f, "interrupted: {msg}"),
            OllamaSetupError::Other(msg) => write!(f, "{msg}"),
        }
    }
}

fn http_client() -> reqwest::Client {
    reqwest::Client::builder()
        .timeout(Duration::from_secs(5))
        .build()
        .unwrap_or_default()
}

/// Cheap existence check: does the Ollama HTTP API respond at all.
async fn is_running() -> bool {
    http_client()
        .get(format!("{OLLAMA_NATIVE_BASE_URL}/api/version"))
        .send()
        .await
        .is_ok_and(|r| r.status().is_success())
}

/// Does the `ollama` binary resolve on PATH (installed, regardless of
/// whether the service is currently running).
async fn binary_installed() -> bool {
    tokio::task::spawn_blocking(|| {
        Command::new("ollama")
            .arg("--version")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .is_ok_and(|s| s.success())
    })
    .await
    .unwrap_or(false)
}

async fn brew_available() -> bool {
    tokio::task::spawn_blocking(|| {
        Command::new("which")
            .arg("brew")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .is_ok_and(|s| s.success())
    })
    .await
    .unwrap_or(false)
}

#[derive(Deserialize)]
struct TagsResponse {
    #[serde(default)]
    models: Vec<TagEntry>,
}

#[derive(Deserialize)]
struct TagEntry {
    name: String,
}

/// Every model name Ollama currently has pulled — not filtered to the
/// adaptive-cleanup tier models, so callers like the mode editor's inline
/// download affordance can check membership for an arbitrary typed model.
async fn list_local_models() -> Vec<String> {
    let Ok(resp) = http_client()
        .get(format!("{OLLAMA_NATIVE_BASE_URL}/api/tags"))
        .send()
        .await
    else {
        return Vec::new();
    };
    let Ok(tags) = resp.json::<TagsResponse>().await else {
        return Vec::new();
    };
    tags.models.into_iter().map(|m| m.name).collect()
}

/// Probe current Ollama state: installed/running + every model already
/// pulled. Called on wizard mount and each time the Settings post-processing
/// section mounts (the "check on next visit" mechanism — deliberately not a
/// background poller).
pub async fn probe_ollama() -> OllamaProbeResult {
    let running = is_running().await;
    let availability = if running {
        OllamaAvailability::Running
    } else if binary_installed().await {
        OllamaAvailability::InstalledNotRunning
    } else {
        OllamaAvailability::NotInstalled
    };
    let models_present = if running {
        list_local_models().await
    } else {
        Vec::new()
    };
    let result = OllamaProbeResult {
        availability,
        brew_available: brew_available().await,
        models_present,
    };
    debug!(
        "Ollama probe: availability={:?} brew_available={} models_present={:?}",
        result.availability, result.brew_available, result.models_present
    );
    result
}

/// Install Ollama via Homebrew if not already present. Short-circuits if the
/// binary already resolves (nothing to install). Fails soft with a typed
/// error the wizard can match against the failure matrix; never panics.
pub async fn install_ollama() -> Result<(), OllamaSetupError> {
    if binary_installed().await {
        debug!("Ollama binary already present; skipping install");
        return Ok(());
    }
    if !brew_available().await {
        debug!("Ollama absent and brew unavailable");
        return Err(OllamaSetupError::BrewAbsent);
    }
    debug!("Installing Ollama via brew");
    let output =
        tokio::task::spawn_blocking(|| Command::new("brew").args(["install", "ollama"]).output())
            .await
            .map_err(|e| OllamaSetupError::Other(format!("brew install task panicked: {e}")))?
            .map_err(|e| OllamaSetupError::Other(format!("failed to run brew: {e}")))?;
    if output.status.success() {
        return Ok(());
    }
    let stderr = String::from_utf8_lossy(&output.stderr).to_lowercase();
    if stderr.contains("no space") || stderr.contains("disk") {
        return Err(OllamaSetupError::DiskFull(stderr.trim().to_string()));
    }
    if stderr.contains("could not resolve host") || stderr.contains("network") {
        return Err(OllamaSetupError::NoNetwork);
    }
    Err(OllamaSetupError::Other(stderr.trim().to_string()))
}

/// Start the Ollama service if it isn't already running, then poll for it
/// to come up. Bounded retries — on exhaustion the caller falls back to
/// manual Retry/Skip buttons, same pattern as `AccessibilityOnboarding`'s
/// polling cap.
pub async fn ensure_ollama_running() -> Result<(), OllamaSetupError> {
    if is_running().await {
        debug!("Ollama already running");
        return Ok(());
    }
    debug!("Ollama not running; starting service");
    // Detached: the wizard doesn't own this process's lifetime, matching how
    // `ollama serve` is meant to be run as a background daemon. Spawned on a
    // blocking thread; a dedicated OS thread reaps it (not awaited) so it
    // doesn't become a zombie once it exits, without blocking the async runtime.
    tokio::task::spawn_blocking(|| {
        if let Ok(mut child) = Command::new("ollama")
            .arg("serve")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
        {
            std::thread::spawn(move || {
                let _ = child.wait();
            });
        }
    });
    const MAX_ATTEMPTS: u32 = 5;
    for _ in 0..MAX_ATTEMPTS {
        sleep(Duration::from_secs(1)).await;
        if is_running().await {
            return Ok(());
        }
    }
    Err(OllamaSetupError::Other(
        "Ollama did not start in time".to_string(),
    ))
}

#[derive(Serialize, Clone, Type)]
struct OllamaPullProgressEvent {
    model_name: String,
    status: String,
    downloaded: Option<u64>,
    total: Option<u64>,
}

#[derive(Deserialize)]
struct PullStatusLine {
    // A terminal error line (e.g. an unknown model name) is JUST
    // `{"error": "..."}` with no "status" field at all — without this
    // default, that line fails to deserialize entirely, the error gets
    // silently dropped via the caller's `Err(_) => continue`, and the pull
    // misreports as a generic "stream ended before completion" instead of
    // surfacing the real reason.
    #[serde(default)]
    status: String,
    #[serde(default)]
    completed: Option<u64>,
    #[serde(default)]
    total: Option<u64>,
    #[serde(default)]
    error: Option<String>,
}

/// Stream `POST /api/pull` for a single model, emitting `"ollama-pull-progress"`
/// events as NDJSON status lines arrive. Idempotent/resumable: Ollama resumes
/// from partial blobs on re-invocation, so an interrupted stream just needs
/// the caller to call this again — no separate resume plumbing.
pub async fn pull_model(app: &AppHandle, model_name: &str) -> Result<(), OllamaSetupError> {
    debug!("Pulling Ollama model: {}", model_name);
    // No .timeout() call: pulls can take minutes and reqwest's actual default
    // is "no timeout" — Duration::from_secs(0) is NOT equivalent to that, it
    // sets a real zero-second deadline that fires almost immediately.
    let client = reqwest::Client::builder()
        .build()
        .map_err(|e| OllamaSetupError::Other(e.to_string()))?;

    let response = client
        .post(format!("{OLLAMA_NATIVE_BASE_URL}/api/pull"))
        .json(&serde_json::json!({ "name": model_name, "stream": true }))
        .send()
        .await
        .map_err(|e| {
            if e.is_connect() {
                OllamaSetupError::NoNetwork
            } else {
                OllamaSetupError::Other(e.to_string())
            }
        })?;

    if !response.status().is_success() {
        return Err(OllamaSetupError::Other(format!(
            "pull request failed with status {}",
            response.status()
        )));
    }

    let mut stream = response.bytes_stream();
    let mut buf = String::new();
    let mut saw_success = false;

    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|e| OllamaSetupError::Interrupted(e.to_string()))?;
        buf.push_str(&String::from_utf8_lossy(&chunk));

        while let Some(newline_pos) = buf.find('\n') {
            let line = buf[..newline_pos].to_string();
            buf.drain(..=newline_pos);
            if line.trim().is_empty() {
                continue;
            }
            let parsed: PullStatusLine = match serde_json::from_str(&line) {
                Ok(p) => p,
                Err(_) => continue, // malformed line: skip, don't abort the pull
            };
            if let Some(err) = parsed.error {
                let lower = err.to_lowercase();
                if lower.contains("no space") || lower.contains("disk") {
                    return Err(OllamaSetupError::DiskFull(err));
                }
                return Err(OllamaSetupError::Other(err));
            }
            if parsed.status == "success" {
                saw_success = true;
            }
            let _ = app.emit(
                "ollama-pull-progress",
                OllamaPullProgressEvent {
                    model_name: model_name.to_string(),
                    status: parsed.status,
                    downloaded: parsed.completed,
                    total: parsed.total,
                },
            );
        }
    }

    if saw_success {
        debug!("Ollama model pull succeeded: {}", model_name);
        Ok(())
    } else {
        debug!("Ollama model pull interrupted: {}", model_name);
        Err(OllamaSetupError::Interrupted(
            "stream ended before completion".to_string(),
        ))
    }
}

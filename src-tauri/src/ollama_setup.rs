//! First-run setup helper for local Ollama cleanup (Phase 8).
//!
//! Best-effort install/start/pull orchestration for the two adaptive-cleanup
//! tier models (see `settings::default_short_model` / `default_long_model`).
//! Every function here is designed to fail soft: callers report status back
//! to the onboarding wizard, which always offers "skip" — nothing here ever
//! blocks first run.
//!
//! DECISION: Ollama is a runtime download, not bundled in the app. `install_ollama`
//! fetches the official signed/notarized installer per OS on demand (macOS
//! `Ollama-darwin.zip`, Windows `OllamaSetup.exe`, Linux `install.sh`) rather than
//! shipping it inside the `.dmg`/`.app`. Reasons: the bundle stays small (cleanup is
//! opt-in, so users on raw Whisper never pay for it), every install gets the current
//! Ollama build instead of a pinned stale copy, and Ollama's binaries stay outside our
//! unsigned bundle's distribution boundary. First-run cleanup already needs the network
//! to pull the LLM models (much larger than Ollama itself), so this adds no new offline
//! limitation. Bundling would only pay off for a separate offline-first build that also
//! ships the models (1 GB+), which is not the default app.

use futures_util::StreamExt;
use log::debug;
use serde::{Deserialize, Serialize};
use specta::Type;
use std::path::{Path, PathBuf};
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
    DiskFull(String),
    Interrupted(String),
    Other(String),
}

impl std::fmt::Display for OllamaSetupError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            OllamaSetupError::NoNetwork => write!(f, "no network connection"),
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

/// Warm-load a model into Ollama's memory ahead of time. An `/api/generate`
/// request with no prompt is Ollama's "load" request: it returns once the model
/// is resident and holds it for `keep_alive`. Fired at record-start so the
/// short-tier cleanup model's cold start overlaps the time the user spends
/// recording and transcribing, instead of stalling the paste afterward.
///
/// Best-effort and fire-and-forget: any failure (Ollama down, model missing)
/// is ignored — cleanup still works, just without the warm-up head start. Uses
/// its own long-timeout client because a cold load reads several GB from disk
/// and can exceed the short probe timeout.
pub async fn warm_model(model: String, keep_alive: &str) {
    #[derive(Serialize)]
    struct WarmRequest<'a> {
        model: &'a str,
        keep_alive: &'a str,
    }

    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(120))
        .build()
        .unwrap_or_default();

    let _ = client
        .post(format!("{OLLAMA_NATIVE_BASE_URL}/api/generate"))
        .json(&WarmRequest {
            model: &model,
            keep_alive,
        })
        .send()
        .await;
}

/// Probe current Ollama state: installed/running + every model already
/// pulled. Called on wizard mount and each time the Settings post-processing
/// section mounts (the "check on next visit" mechanism — deliberately not a
/// background poller).
pub async fn probe_ollama() -> OllamaProbeResult {
    let running = is_running().await;
    let availability = if running {
        OllamaAvailability::Running
    } else if ollama_installed().await {
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

/// Official per-OS Ollama downloads (each redirects to the latest GitHub
/// release). macOS gets the signed, notarized universal app zip; Windows the
/// Inno Setup silent installer; Linux the official install script. Each is
/// gated to the OS that references it so the others don't warn as unused.
#[cfg(target_os = "macos")]
const OLLAMA_MACOS_ZIP_URL: &str = "https://ollama.com/download/Ollama-darwin.zip";
#[cfg(target_os = "windows")]
const OLLAMA_WINDOWS_EXE_URL: &str = "https://ollama.com/download/OllamaSetup.exe";
#[cfg(target_os = "linux")]
const OLLAMA_LINUX_SCRIPT_URL: &str = "https://ollama.com/install.sh";

#[derive(Serialize, Clone, Type)]
struct OllamaInstallProgressEvent {
    downloaded: u64,
    total: Option<u64>,
}

/// Absolute locations the `ollama` CLI may live in outside PATH. A
/// Finder-launched .app on macOS inherits a minimal PATH (no
/// `/opt/homebrew/bin`, no `/usr/local/bin`), so a PATH-only probe
/// under-detects an installed Ollama; these known locations cover that gap.
fn known_cli_paths() -> Vec<PathBuf> {
    #[cfg(target_os = "macos")]
    {
        let mut v = vec![
            PathBuf::from("/opt/homebrew/bin/ollama"),
            PathBuf::from("/usr/local/bin/ollama"),
            PathBuf::from("/Applications/Ollama.app/Contents/Resources/ollama"),
        ];
        if let Ok(home) = std::env::var("HOME") {
            v.push(PathBuf::from(format!(
                "{home}/Applications/Ollama.app/Contents/Resources/ollama"
            )));
        }
        v
    }
    #[cfg(target_os = "windows")]
    {
        let mut v = Vec::new();
        if let Ok(lad) = std::env::var("LOCALAPPDATA") {
            v.push(PathBuf::from(format!(r"{lad}\Programs\Ollama\ollama.exe")));
        }
        v
    }
    #[cfg(target_os = "linux")]
    {
        vec![
            PathBuf::from("/usr/local/bin/ollama"),
            PathBuf::from("/usr/bin/ollama"),
        ]
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
    {
        Vec::new()
    }
}

/// Locations the Ollama macOS app bundle may be installed to.
#[cfg(target_os = "macos")]
fn macos_app_paths() -> Vec<PathBuf> {
    let mut v = vec![PathBuf::from("/Applications/Ollama.app")];
    if let Ok(home) = std::env::var("HOME") {
        v.push(PathBuf::from(format!("{home}/Applications/Ollama.app")));
    }
    v
}

/// Broader "is Ollama on this machine at all" check: PATH first, then the
/// known absolute install locations (and, on macOS, the app bundle). More
/// reliable than `binary_installed` inside a packaged, Finder-launched app.
pub async fn ollama_installed() -> bool {
    if binary_installed().await {
        return true;
    }
    tokio::task::spawn_blocking(|| {
        if known_cli_paths().iter().any(|p| p.exists()) {
            return true;
        }
        #[cfg(target_os = "macos")]
        {
            if macos_app_paths().iter().any(|p| p.exists()) {
                return true;
            }
        }
        false
    })
    .await
    .unwrap_or(false)
}

/// Build the best available command to start the Ollama server for this OS,
/// preferring an absolute path (PATH-independent) over a bare `ollama`.
fn ollama_start_command() -> Command {
    #[cfg(target_os = "macos")]
    {
        // Launching the app bundle starts its bundled server.
        for app in macos_app_paths() {
            if app.exists() {
                let mut c = Command::new("open");
                c.arg(&app);
                return c;
            }
        }
    }
    #[cfg(target_os = "windows")]
    {
        if let Ok(lad) = std::env::var("LOCALAPPDATA") {
            let app_exe = PathBuf::from(format!(r"{lad}\Programs\Ollama\ollama app.exe"));
            if app_exe.exists() {
                return Command::new(app_exe);
            }
        }
    }
    // A known-absolute CLI, then a bare PATH lookup as the last resort.
    for p in known_cli_paths() {
        if p.exists() {
            let mut c = Command::new(p);
            c.arg("serve");
            return c;
        }
    }
    let mut c = Command::new("ollama");
    c.arg("serve");
    c
}

/// Classify an I/O error, mapping "out of space" (ENOSPC 28 / Windows 112)
/// onto the typed `DiskFull` the wizard surfaces distinctly.
fn map_io_err(e: std::io::Error) -> OllamaSetupError {
    let out_of_space = matches!(e.raw_os_error(), Some(28) | Some(112))
        || e.to_string().to_lowercase().contains("space");
    if out_of_space {
        OllamaSetupError::DiskFull(e.to_string())
    } else {
        OllamaSetupError::Other(e.to_string())
    }
}

/// Stream a URL to `dest`, emitting `"ollama-install-progress"` as bytes
/// land (throttled to ~every 2 MB) so the wizard can show a download bar for
/// the large installers. No timeout: these downloads run to hundreds of MB.
async fn download_file(app: &AppHandle, url: &str, dest: &Path) -> Result<(), OllamaSetupError> {
    use std::io::Write;
    let client = reqwest::Client::builder()
        .build()
        .map_err(|e| OllamaSetupError::Other(e.to_string()))?;
    let resp = client.get(url).send().await.map_err(|e| {
        if e.is_connect() {
            OllamaSetupError::NoNetwork
        } else {
            OllamaSetupError::Other(e.to_string())
        }
    })?;
    if !resp.status().is_success() {
        return Err(OllamaSetupError::Other(format!(
            "download failed with status {}",
            resp.status()
        )));
    }
    let total = resp.content_length();
    let file = std::fs::File::create(dest).map_err(map_io_err)?;
    let mut writer = std::io::BufWriter::new(file);
    let mut stream = resp.bytes_stream();
    let mut downloaded: u64 = 0;
    let mut last_emit: u64 = 0;
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|e| OllamaSetupError::Interrupted(e.to_string()))?;
        writer.write_all(&chunk).map_err(map_io_err)?;
        downloaded += chunk.len() as u64;
        if downloaded - last_emit >= 2_000_000 {
            last_emit = downloaded;
            let _ = app.emit(
                "ollama-install-progress",
                OllamaInstallProgressEvent { downloaded, total },
            );
        }
    }
    writer.flush().map_err(map_io_err)?;
    let _ = app.emit(
        "ollama-install-progress",
        OllamaInstallProgressEvent { downloaded, total },
    );
    Ok(())
}

/// Run a command to completion on a blocking thread, classifying disk-full /
/// network stderr into the typed errors the wizard understands.
async fn run_command_checked(mut cmd: Command) -> Result<(), OllamaSetupError> {
    tokio::task::spawn_blocking(move || {
        let output = cmd
            .output()
            .map_err(|e| OllamaSetupError::Other(e.to_string()))?;
        if output.status.success() {
            return Ok(());
        }
        let stderr = String::from_utf8_lossy(&output.stderr).to_lowercase();
        if stderr.contains("no space") || stderr.contains("disk full") {
            Err(OllamaSetupError::DiskFull(stderr.trim().to_string()))
        } else if stderr.contains("could not resolve host") || stderr.contains("network") {
            Err(OllamaSetupError::NoNetwork)
        } else {
            Err(OllamaSetupError::Other(stderr.trim().to_string()))
        }
    })
    .await
    .map_err(|e| OllamaSetupError::Other(format!("install task panicked: {e}")))?
}

/// Install Ollama for the current OS if it isn't already present, then leave
/// it for `ensure_ollama_running` to start. Short-circuits when Ollama is
/// already running or installed. Fails soft with a typed error the wizard
/// matches against its failure matrix; never panics.
pub async fn install_ollama(app: &AppHandle) -> Result<(), OllamaSetupError> {
    if is_running().await {
        debug!("Ollama already running; skipping install");
        return Ok(());
    }
    if ollama_installed().await {
        debug!("Ollama already installed; skipping install");
        return Ok(());
    }
    install_ollama_platform(app).await
}

/// macOS install: Homebrew fast path when present (keeps the CLI on PATH),
/// otherwise download the signed, notarized universal app bundle and place it
/// in Applications. Because Locution fetches the zip itself (not via a
/// browser), macOS attaches no `com.apple.quarantine` flag, so the app
/// launches with no Gatekeeper prompt.
#[cfg(target_os = "macos")]
async fn install_ollama_platform(app: &AppHandle) -> Result<(), OllamaSetupError> {
    if brew_available().await {
        debug!("Installing Ollama via brew");
        let mut cmd = Command::new("brew");
        cmd.args(["install", "ollama"]);
        return run_command_checked(cmd).await;
    }

    debug!("Homebrew absent; installing Ollama.app from official zip");
    let tmp = std::env::temp_dir().join("locution-ollama");
    std::fs::create_dir_all(&tmp).map_err(map_io_err)?;
    let zip = tmp.join("Ollama-darwin.zip");
    download_file(app, OLLAMA_MACOS_ZIP_URL, &zip).await?;

    let extract_dir = tmp.join("extract");
    let _ = std::fs::remove_dir_all(&extract_dir);
    std::fs::create_dir_all(&extract_dir).map_err(map_io_err)?;
    // `ditto` unpacks .app bundles (resource forks, symlinks) correctly where
    // plain `unzip` can mangle them.
    let mut unzip = Command::new("/usr/bin/ditto");
    unzip.arg("-x").arg("-k").arg(&zip).arg(&extract_dir);
    run_command_checked(unzip).await?;

    let src_app = extract_dir.join("Ollama.app");
    if !src_app.exists() {
        return Err(OllamaSetupError::Other(
            "downloaded archive did not contain Ollama.app".to_string(),
        ));
    }
    let dest_app = install_macos_app(&src_app).await?;
    let _ = std::fs::remove_file(&zip);
    let _ = std::fs::remove_dir_all(&extract_dir);

    // Launch to start the bundled server; ensure_ollama_running then polls.
    let mut launch = Command::new("open");
    launch.arg(&dest_app);
    let _ = run_command_checked(launch).await;
    Ok(())
}

/// Copy Ollama.app into /Applications, falling back to ~/Applications when the
/// system folder isn't writable (non-admin users).
#[cfg(target_os = "macos")]
async fn install_macos_app(src_app: &Path) -> Result<PathBuf, OllamaSetupError> {
    let mut dirs = vec![PathBuf::from("/Applications")];
    if let Ok(home) = std::env::var("HOME") {
        dirs.push(PathBuf::from(format!("{home}/Applications")));
    }
    for dir in dirs {
        if std::fs::create_dir_all(&dir).is_err() {
            continue;
        }
        let dest = dir.join("Ollama.app");
        let _ = std::fs::remove_dir_all(&dest);
        let mut copy = Command::new("/usr/bin/ditto");
        copy.arg(src_app).arg(&dest);
        if run_command_checked(copy).await.is_ok() {
            return Ok(dest);
        }
    }
    Err(OllamaSetupError::Other(
        "could not install Ollama.app (no writable Applications folder)".to_string(),
    ))
}

/// Windows install: download and silently run the official Inno Setup
/// installer. Ollama installs per-user (`PrivilegesRequired=lowest`), so there
/// is no UAC prompt, and the installer's [Run] step starts the app afterward.
#[cfg(target_os = "windows")]
async fn install_ollama_platform(app: &AppHandle) -> Result<(), OllamaSetupError> {
    debug!("Installing Ollama via OllamaSetup.exe");
    let tmp = std::env::temp_dir().join("locution-ollama");
    std::fs::create_dir_all(&tmp).map_err(map_io_err)?;
    let installer = tmp.join("OllamaSetup.exe");
    download_file(app, OLLAMA_WINDOWS_EXE_URL, &installer).await?;
    let mut cmd = Command::new(&installer);
    cmd.args(["/VERYSILENT", "/SUPPRESSMSGBOXES", "/NORESTART"]);
    run_command_checked(cmd).await?;
    let _ = std::fs::remove_file(&installer);
    Ok(())
}

/// Linux install: download and run the official install script. It installs a
/// systemd service and calls sudo internally, so it fails soft in a headless
/// GUI context (no tty) — Linux is not a built/tested Locution target.
#[cfg(target_os = "linux")]
async fn install_ollama_platform(app: &AppHandle) -> Result<(), OllamaSetupError> {
    debug!("Installing Ollama via official install.sh");
    let tmp = std::env::temp_dir().join("locution-ollama");
    std::fs::create_dir_all(&tmp).map_err(map_io_err)?;
    let script = tmp.join("ollama-install.sh");
    download_file(app, OLLAMA_LINUX_SCRIPT_URL, &script).await?;
    let mut cmd = Command::new("sh");
    cmd.arg(&script);
    run_command_checked(cmd).await?;
    let _ = std::fs::remove_file(&script);
    Ok(())
}

#[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
async fn install_ollama_platform(_app: &AppHandle) -> Result<(), OllamaSetupError> {
    Err(OllamaSetupError::Other(
        "automatic Ollama install is not supported on this platform".to_string(),
    ))
}

/// Start the Ollama service if it isn't already running, then poll for it to
/// come up. Uses an OS-appropriate, PATH-independent start command. Bounded
/// retries — on exhaustion the caller falls back to manual Retry/Skip buttons,
/// same pattern as `AccessibilityOnboarding`'s polling cap.
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
        let mut cmd = ollama_start_command();
        cmd.stdout(Stdio::null()).stderr(Stdio::null());
        if let Ok(mut child) = cmd.spawn() {
            std::thread::spawn(move || {
                let _ = child.wait();
            });
        }
    });
    // Up to 20s: a Windows service or a macOS app cold start can take several
    // seconds longer than a bare `ollama serve`.
    const MAX_ATTEMPTS: u32 = 20;
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

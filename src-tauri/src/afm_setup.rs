//! Phase-1 spike: Apple Foundation Models (AFM) sidecar lifecycle + health.
//!
//! Locution never links `FoundationModels` itself (that path hit an early-init
//! crash — see CLAUDE.md). Instead it spawns the bundled `afm-sidecar` binary,
//! which exposes the on-device model behind an OpenAI-compatible HTTP surface.
//! This module owns the child process: mint a per-session token, spawn, learn
//! the ephemeral loopback port, health-check, and (for the spike) do a full
//! cleanup round-trip.
//!
//! Everything here is dead code until a later phase registers the `afm`
//! provider and wires spawn into the cleanup path; it is compiled now to lock
//! the process/HTTP contract the sidecar and the app share. Uses `std::process`
//! (not `tokio::process`) to avoid assuming tokio's `process` feature is on.
#![allow(dead_code)]

use std::io::{BufRead, BufReader, Read};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::mpsc;
use std::sync::Mutex;
use std::time::Duration;

use tauri::{AppHandle, Manager};

/// The single model id the sidecar advertises on `/v1/models`.
pub const AFM_MODEL_ID: &str = "apple-afm";

/// Line the sidecar prints on stdout once it is listening, followed by the
/// chosen (possibly ephemeral) port. Must match `readyPrefix` in the sidecar.
const READY_PREFIX: &str = "AFM_SIDECAR_LISTENING ";

/// Fallback context window if the endpoint does not report one (macOS 26 = 4096;
/// the sidecar itself reports the real value, widening to 8192 on macOS 27).
pub const AFM_FALLBACK_CONTEXT_WINDOW: u32 = 4096;

/// Whether AFM can plausibly run here. The sidecar refuses to start on the wrong
/// OS, but checking up front lets the UI hide the provider on unsupported Macs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AfmAvailability {
    /// macOS older than 26 — FoundationModels is absent.
    UnsupportedOs,
    /// Supported OS, but the sidecar is not currently running.
    SidecarNotRunning,
    /// Sidecar is up and answering, with the context window it reported.
    Running { port: u16, context_window: u32 },
}

/// Mint a per-session bearer token (32 random bytes, hex). Passed to the sidecar
/// via env (never argv) so it is not visible in `ps`.
pub fn mint_token() -> String {
    let mut buf = [0u8; 32];
    if let Ok(mut f) = std::fs::File::open("/dev/urandom") {
        let _ = f.read_exact(&mut buf);
    }
    buf.iter().map(|b| format!("{b:02x}")).collect()
}

/// macOS major version, e.g. 26. `None` off macOS or if detection fails.
pub fn os_major_version() -> Option<u32> {
    if !cfg!(target_os = "macos") {
        return None;
    }
    let out = Command::new("sw_vers").arg("-productVersion").output().ok()?;
    let text = String::from_utf8_lossy(&out.stdout);
    text.trim().split('.').next()?.parse().ok()
}

/// A running sidecar child process. Killed on drop so it never outlives the app.
pub struct AfmSidecar {
    child: Child,
    pub port: u16,
    pub token: String,
}

impl AfmSidecar {
    /// Spawn the sidecar binary, wait (up to 10s) for its ready line, and
    /// capture the loopback port it bound. A background thread keeps draining
    /// stdout so the pipe never blocks the child.
    pub fn spawn(binary: &Path) -> Result<Self, String> {
        let token = mint_token();
        let mut child = Command::new(binary)
            .arg("--port")
            .arg("0") // ephemeral; we read the real port from stdout
            .env("AFM_SIDECAR_TOKEN", &token)
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()
            .map_err(|e| format!("spawn failed: {e}"))?;

        let stdout = child.stdout.take().ok_or("sidecar produced no stdout")?;
        let (tx, rx) = mpsc::channel::<u16>();

        std::thread::spawn(move || {
            let mut sent = false;
            for line in BufReader::new(stdout).lines().map_while(Result::ok) {
                if !sent {
                    if let Some(rest) = line.strip_prefix(READY_PREFIX) {
                        if let Ok(port) = rest.trim().parse::<u16>() {
                            let _ = tx.send(port);
                            sent = true;
                        }
                    }
                }
                // keep draining after ready so the child's stdout never blocks
            }
        });

        match rx.recv_timeout(Duration::from_secs(10)) {
            Ok(port) => Ok(Self { child, port, token }),
            Err(_) => {
                let _ = child.kill();
                Err("timed out waiting for sidecar ready line".to_string())
            }
        }
    }

    pub fn base_url(&self) -> String {
        format!("http://127.0.0.1:{}/v1", self.port)
    }

    /// Whether the child process is still running (has not exited or crashed).
    pub fn is_alive(&mut self) -> bool {
        matches!(self.child.try_wait(), Ok(None))
    }
}

impl Drop for AfmSidecar {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn http_client() -> reqwest::Client {
    reqwest::Client::builder()
        .timeout(Duration::from_secs(10))
        .build()
        .unwrap_or_default()
}

/// Health-check `GET /v1/models`; returns the reported context window (tokens)
/// on success, `None` if the sidecar is unreachable or rejects the token.
pub async fn health_check(base_url: &str, token: &str) -> Option<u32> {
    let resp = http_client()
        .get(format!("{base_url}/models"))
        .bearer_auth(token)
        .send()
        .await
        .ok()?;
    if !resp.status().is_success() {
        return None;
    }
    let body: serde_json::Value = resp.json().await.ok()?;
    let window = body["data"][0]["context_window"]
        .as_u64()
        .map(|v| v as u32)
        .unwrap_or(AFM_FALLBACK_CONTEXT_WINDOW);
    Some(window)
}

/// Full cleanup round-trip through the OpenAI-compatible surface. Returns the
/// cleaned transcript. Mirrors the message split Locution's `custom` path uses:
/// instructions as the system message, the transcript as the user message.
pub async fn cleanup(
    base_url: &str,
    token: &str,
    system_prompt: &str,
    transcript: &str,
) -> Result<String, String> {
    let request = serde_json::json!({
        "model": AFM_MODEL_ID,
        "messages": [
            { "role": "system", "content": system_prompt },
            { "role": "user", "content": format!("<transcript>{transcript}</transcript>") },
        ],
        "temperature": 0.2,
    });

    let resp = http_client()
        .post(format!("{base_url}/chat/completions"))
        .bearer_auth(token)
        .json(&request)
        .send()
        .await
        .map_err(|e| format!("request failed: {e}"))?;

    if !resp.status().is_success() {
        return Err(format!("sidecar returned status {}", resp.status()));
    }

    let body: serde_json::Value = resp
        .json()
        .await
        .map_err(|e| format!("bad response json: {e}"))?;

    body["choices"][0]["message"]["content"]
        .as_str()
        .map(|s| s.to_string())
        .ok_or_else(|| "response missing choices[0].message.content".to_string())
}

/// Connection details for a live sidecar, handed to the cleanup path.
#[derive(Clone)]
pub struct AfmRuntime {
    pub base_url: String,
    pub token: String,
    pub context_window: u32,
}

/// Tauri-managed owner of the sidecar child process. Spawns lazily on first
/// use and reuses the live process across cleanups; the child is killed when
/// this drops at shutdown (via `AfmSidecar`'s Drop).
#[derive(Default)]
pub struct AfmManager {
    inner: Mutex<Option<AfmSidecar>>,
}

impl AfmManager {
    /// Where the bundled sidecar binary lives. In dev it is the Swift package's
    /// release build; in a packaged app it sits next to the main executable
    /// (Tauri `externalBin` staging — wired in a later phase).
    fn binary_path() -> Option<PathBuf> {
        if cfg!(debug_assertions) {
            Some(
                Path::new(env!("CARGO_MANIFEST_DIR"))
                    .join("afm-sidecar/.build/release/afm-sidecar"),
            )
        } else {
            std::env::current_exe()
                .ok()?
                .parent()
                .map(|dir| dir.join("afm-sidecar"))
        }
    }
}

/// Ensure the sidecar is running and return its connection details. Reuses a
/// live child when possible; otherwise spawns one off the async worker. Returns
/// `Err` (and the caller keeps the raw transcript) if it can't start or answer.
pub async fn ensure_runtime(app: &AppHandle) -> Result<AfmRuntime, String> {
    let manager = app.state::<AfmManager>();

    // Fast path: a live, healthy child is reused as-is.
    let existing = {
        let mut guard = manager.inner.lock().map_err(|_| "afm state poisoned")?;
        if let Some(sidecar) = guard.as_mut() {
            if sidecar.is_alive() {
                Some((sidecar.base_url(), sidecar.token.clone()))
            } else {
                None
            }
        } else {
            None
        }
    };
    if let Some((base_url, token)) = existing {
        if let Some(context_window) = health_check(&base_url, &token).await {
            return Ok(AfmRuntime {
                base_url,
                token,
                context_window,
            });
        }
        // Alive but not answering — fall through and respawn.
    }

    // Slow path: spawn a fresh sidecar (blocking spawn moved off the executor).
    let binary = AfmManager::binary_path().ok_or("could not resolve sidecar binary path")?;
    if !binary.exists() {
        return Err(format!("sidecar binary missing at {}", binary.display()));
    }
    let sidecar = tokio::task::spawn_blocking(move || AfmSidecar::spawn(&binary))
        .await
        .map_err(|e| format!("spawn task failed: {e}"))??;

    let base_url = sidecar.base_url();
    let token = sidecar.token.clone();
    {
        let mut guard = manager.inner.lock().map_err(|_| "afm state poisoned")?;
        *guard = Some(sidecar);
    }

    let context_window = health_check(&base_url, &token)
        .await
        .ok_or("sidecar started but failed its health check")?;

    Ok(AfmRuntime {
        base_url,
        token,
        context_window,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// End-to-end spike test. Ignored by default (needs macOS 26 + Apple
    /// Intelligence enabled + the built sidecar). Run with:
    ///   AFM_SIDECAR_BIN=src-tauri/afm-sidecar/.build/release/afm-sidecar \
    ///   cargo test -p handy afm_setup -- --ignored --nocapture
    #[test]
    #[ignore]
    fn spawn_health_and_cleanup_roundtrip() {
        let Ok(bin) = std::env::var("AFM_SIDECAR_BIN") else {
            eprintln!("skipping: set AFM_SIDECAR_BIN to the built sidecar path");
            return;
        };

        let sidecar = AfmSidecar::spawn(Path::new(&bin)).expect("sidecar should spawn");
        let base_url = sidecar.base_url();
        eprintln!("sidecar listening at {base_url}");

        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("runtime");

        rt.block_on(async {
            let window = health_check(&base_url, &sidecar.token)
                .await
                .expect("health check should report a context window");
            eprintln!("context window: {window}");
            assert!(window >= AFM_FALLBACK_CONTEXT_WINDOW);

            // Wrong token must be rejected.
            assert!(
                health_check(&base_url, "not-the-token").await.is_none(),
                "bad token must fail health check"
            );

            let cleaned = cleanup(
                &base_url,
                &sidecar.token,
                "You clean up dictated text. Fix punctuation and capitalization. \
                 Respond with nothing but the corrected transcript.",
                "um so i think we should like ship the feature on monday and uh tell the team",
            )
            .await
            .expect("cleanup round-trip should succeed");

            eprintln!("cleaned: {cleaned}");
            assert!(!cleaned.trim().is_empty(), "cleaned text must be non-empty");
        });
    }
}

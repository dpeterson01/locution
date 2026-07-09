# Locution

**A local dictation app for macOS and Windows: speak naturally, get clean, formatted text pasted into any app — with local Whisper transcription and local Ollama cleanup. Nothing leaves the machine.**

Locution is a fork of [Handy](https://github.com/cjpais/Handy), the free, open-source, extensible speech-to-text app that works completely offline. Handy proved out the whole pipeline — global hotkey, mic capture, VAD, local Whisper, optional LLM post-processing, paste — Locution builds on that with a configurable push-to-talk hotkey and adaptive-latency Ollama cleanup.

## Why Locution?

- **Free & local**: nothing leaves your machine — no cloud, no accounts
- **Open Source**: built on Handy, extend it for yourself
- **Private**: your voice stays on your computer
- **Adaptive**: short dictation returns fast; long dictation is cleaned more thoroughly

## How It Works

1. **Hold your push-to-talk hotkey** to start recording
2. **Speak** your words while the key is held
3. **Release** and Locution transcribes your speech using Whisper, then cleans it up with a local Ollama model
4. **Get** your transcribed text pasted directly into whatever app you're using

The process is entirely local:

- Silence is filtered using VAD (Voice Activity Detection) with Silero
- Transcription uses your choice of models:
  - **Whisper models** (Small/Medium/Turbo/Large) with GPU acceleration when available
  - **Parakeet V3** - CPU-optimized model with excellent performance and automatic language detection
- Runs on macOS and Windows

## Quick Start

### Installation

1. Download the latest `Locution_x.y.z_aarch64.dmg` from the [releases page](https://github.com/dpeterson01/locution/releases).
2. Open the `.dmg` and drag **Locution** into your Applications folder.
3. The build is unsigned, so macOS Gatekeeper warns on first launch. **Right-click Locution and choose Open** once, or clear the quarantine flag:
   ```bash
   xattr -dr com.apple.quarantine /Applications/Locution.app
   ```
4. Install [Ollama](https://ollama.com) for local AI cleanup, then pull the models Locution uses:
   ```bash
   ollama pull qwen3.5:2b
   ollama pull gemma4:12b
   ```
   On Apple Silicon, pull the Metal-accelerated variants instead for lower latency:
   ```bash
   ollama pull qwen3.5:2b-mlx
   ollama pull gemma4:12b-mlx
   ```
   Keep Ollama running (`ollama serve`). The speech-to-text model downloads on first launch.
5. Launch **Locution** and grant Microphone and Accessibility permissions when prompted.
6. Hold the push-to-talk hotkey (default **Ctrl+Space**), speak, and release — your text is transcribed and pasted into the active app. Change the hotkey in **Settings**.

### Windows

Windows builds (x64 and ARM64) are available on the [releases page](https://github.com/dpeterson01/locution/releases):

1. Download the `.msi` or `.exe` installer from the [releases page](https://github.com/dpeterson01/locution/releases).
2. The build is unsigned, so Windows SmartScreen warns on first launch. Click **More info**, then **Run anyway**.
3. Install [Ollama](https://ollama.com), pull the same two models listed above, and keep it running.
4. Launch Locution, grant the microphone prompt, and use the default **Ctrl+Space** push-to-talk hotkey (change it in Settings).

Per-app auto-mode keys on the process name on Windows (for example `Code.exe`). Screen-context ("Context") capture is macOS-only for now.

### Development Setup

For detailed build instructions including platform-specific requirements, see [BUILD.md](BUILD.md).

## Architecture

Locution is a Tauri application combining:

- **Frontend**: React + TypeScript with Tailwind CSS for the settings UI
- **Backend**: Rust for system integration, audio processing, and ML inference
- **Core libraries**:
  - `transcribe-cpp`: local Whisper-family speech recognition (GGML/GGUF)
  - `transcribe-rs`: CPU-optimized Parakeet speech recognition
  - `cpal`: cross-platform audio I/O
  - `vad-rs`: voice activity detection
  - `rdev`: global keyboard shortcuts and system events
  - `rubato`: audio resampling

The internal crate keeps the upstream name `handy`, so a development build (`cargo run`) produces a `handy` binary while the packaged app ships as `Locution`.

### Debug mode

Press **Cmd+Shift+D** (macOS) or **Ctrl+Shift+D** (Windows) to open the debug menu for development and troubleshooting.

### Command-line control

The packaged app accepts single-instance control flags. Sending one to an already-running instance toggles or cancels it:

```bash
Locution --toggle-transcription   # Toggle recording on/off
Locution --toggle-post-process    # Toggle recording with cleanup on/off
Locution --cancel                 # Cancel the current operation
```

Startup flags: `--start-hidden`, `--no-tray`, `--debug`, `--help`.

On macOS the binary lives inside the app bundle:

```bash
/Applications/Locution.app/Contents/MacOS/Locution --toggle-transcription
```

## Known limitations

Locution is early and under active development.

- **Whisper crashes on some configurations.** A minority of Windows machines hit a configuration-dependent crash with Whisper models. Parakeet V3 (CPU) is a reliable fallback. If you can reproduce it and are a developer, debug logs help.
- **Context mode is macOS-only.** Screen-context capture (reading selected text near the cursor) is not yet implemented on Windows; the rest of the pipeline works there.
- **Builds are unsigned.** First launch requires the Gatekeeper/SmartScreen steps in [Quick Start](#quick-start).

## Platform support

- **macOS** (Apple Silicon and Intel) — primary, validated
- **Windows** (x64 and ARM64) — available

Locution inherits Handy's Linux support in the codebase, but Linux is not currently built or tested for Locution.

## System requirements

**Whisper models:**

- **macOS**: Apple Silicon or Intel
- **Windows**: Intel, AMD, or NVIDIA GPU (Vulkan) on x64; CPU on ARM64

**Parakeet V3 (CPU):**

- Runs CPU-only on a wide range of hardware
- Minimum: Intel Skylake (6th gen) or equivalent AMD
- ~5x real-time on mid-range hardware; automatic language detection

## Troubleshooting

### Manual Model Installation (For Proxy Users or Network Restrictions)

If you're behind a proxy, firewall, or in a restricted network environment where Locution cannot download models automatically, you can manually download and install them. The URLs are publicly accessible from any browser.

#### Step 1: Find Your App Data Directory

1. Open Locution settings
2. Navigate to the **About** section
3. Copy the "App Data Directory" path shown there, or use the shortcuts:
   - **macOS**: `Cmd+Shift+D` to open debug menu
   - **Windows**: `Ctrl+Shift+D` to open debug menu

The typical paths are:

- **macOS**: `~/Library/Application Support/com.locution.mac/`
- **Windows**: `C:\Users\{username}\AppData\Roaming\com.locution.mac\`

#### Step 2: Create Models Directory

Inside your app data directory, create a `models` folder if it doesn't already exist:

```bash
# macOS
mkdir -p ~/Library/Application\ Support/com.locution.mac/models

# Windows (PowerShell)
New-Item -ItemType Directory -Force -Path "$env:APPDATA\com.locution.mac\models"
```

#### Step 3: Download Model Files

Download the models you want from below

**Whisper Models (single .bin files):**

- Small (487 MB): `https://blob.handy.computer/ggml-small.bin`
- Medium (492 MB): `https://blob.handy.computer/whisper-medium-q4_1.bin`
- Turbo (1600 MB): `https://blob.handy.computer/ggml-large-v3-turbo.bin`
- Large (1100 MB): `https://blob.handy.computer/ggml-large-v3-q5_0.bin`

**Parakeet Models (compressed archives):**

- V2 (473 MB): `https://blob.handy.computer/parakeet-v2-int8.tar.gz`
- V3 (478 MB): `https://blob.handy.computer/parakeet-v3-int8.tar.gz`

#### Step 4: Install Models

**For Whisper Models (.bin files):**

Simply place the `.bin` file directly into the `models` directory:

```
{app_data_dir}/models/
├── ggml-small.bin
├── whisper-medium-q4_1.bin
├── ggml-large-v3-turbo.bin
└── ggml-large-v3-q5_0.bin
```

**For Parakeet Models (.tar.gz archives):**

1. Extract the `.tar.gz` file
2. Place the **extracted directory** into the `models` folder
3. The directory must be named exactly as follows:
   - **Parakeet V2**: `parakeet-tdt-0.6b-v2-int8`
   - **Parakeet V3**: `parakeet-tdt-0.6b-v3-int8`

Final structure should look like:

```
{app_data_dir}/models/
├── parakeet-tdt-0.6b-v2-int8/     (directory with model files inside)
│   ├── (model files)
│   └── (config files)
└── parakeet-tdt-0.6b-v3-int8/     (directory with model files inside)
    ├── (model files)
    └── (config files)
```

**Important Notes:**

- For Parakeet models, the extracted directory name **must** match exactly as shown above
- Do not rename the `.bin` files for Whisper models—use the exact filenames from the download URLs
- After placing the files, restart Locution to detect the new models

#### Step 5: Verify Installation

1. Restart Locution
2. Open Settings → Models
3. Your manually installed models should now appear as "Downloaded"
4. Select the model you want to use and test transcription

### Custom Whisper Models

Locution can auto-discover custom Whisper GGML models placed in the `models` directory. This is useful for users who want to use fine-tuned or community models not included in the default model list.

**How to use:**

1. Obtain a Whisper model in GGML `.bin` format (e.g., from [Hugging Face](https://huggingface.co/models?search=whisper%20ggml))
2. Place the `.bin` file in your `models` directory (see paths above)
3. Restart Locution to discover the new model
4. The model will appear in the "Custom Models" section of the Models settings page

**Important:**

- Community models are user-provided and may not receive troubleshooting assistance
- The model must be a valid Whisper GGML format (`.bin` file)
- Model name is derived from the filename (e.g., `my-custom-model.bin` → "My Custom Model")

## License

MIT License — see [LICENSE](LICENSE) for details.

Locution is an unofficial fork of [Handy](https://github.com/cjpais/Handy) and is not affiliated with or endorsed by the Handy project. Per Handy's terms, the Handy name, logo, icon, and brand assets are not open-source; Locution uses its own branding and does not imply any endorsement or affiliation.

## Acknowledgments

- **[Handy](https://github.com/cjpais/Handy)** by CJ Pais — the upstream project Locution is built on
- **Whisper** by OpenAI for the speech recognition model
- **ggml and transcribe.cpp** for cross-platform speech-to-text inference
- **Silero** for lightweight VAD
- **Tauri** for the Rust-based app framework

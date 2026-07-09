# Project Context

Repo facts and verified gotchas for Locution. Read at session start.

## What the repo is

Locution is a fully-local macOS dictation app: global hotkey → mic capture → local Whisper STT → optional LLM cleanup → paste into focused app.

- **Stack:** Tauri v2, Rust backend (`src-tauri/`), React/TypeScript frontend, package manager `bun`
- **Forked from:** [cjpais/Handy](https://github.com/cjpais/Handy) (MIT code; name/logo/icon NOT MIT)
- **Internal crate name is deliberately `handy`** — do NOT rename it.
- **Bundle ID:** `com.locution.mac` · **productName:** `Locution` · **mainBinaryName:** `Locution`

## Build commands

```bash
bun install
bun run tauri dev          # CMAKE_POLICY_VERSION_MINIMUM=3.5 bun run tauri dev  (if cmake error)
bun run tauri build
bunx tsc --noEmit
bun run lint
bun run format
cd src-tauri && cargo fmt
cd src-tauri && cargo clippy
cd src-tauri && cargo run  # regenerate src/bindings.ts
```

## Git / release

- **Remote:** `git@github-personal:dpeterson01/locution.git`
- **`gh` CLI** authed as `dpeterson01`
- **Release:** `gh workflow run release.yml` — `workflow_dispatch` only. Requires explicit approval.

## Bindings regen contract

After any `#[tauri::command]` add/remove: `cd src-tauri && cargo run`, then `git diff src/bindings.ts`. Never hand-edit `bindings.ts`.

## Verified gotchas

1. `grep -c` exits 1 on zero matches — breaks `&&` chains; append `|| true` or run separately.
2. After unwrapping JSX, run `bunx prettier --write <file>` before lint.
3. ESLint has no `react-hooks/exhaustive-deps` rule — don't add disable comments for it.
4. `SecretMap` in `settings.rs` is a newtype — access inner map with `.0`.
5. DB migrations are append-only — never edit existing entries.
6. Verify subagent output — delegated file creation has hallucinated without producing files.
7. Release workflow is `workflow_dispatch` only — never triggered by tag push.
8. Build log "Apple Intelligence SDK not found. Building with stubs." is harmless.

# Locution — Copilot Instructions

## What this repo is

Locution is a fully-local macOS dictation app: global hotkey → mic capture → Whisper STT → optional LLM cleanup → paste into focused app. Nothing leaves the machine.

- **Stack:** Tauri v2, Rust backend (`src-tauri/`), React/TypeScript frontend, package manager `bun`
- **Forked from:** [cjpais/Handy](https://github.com/cjpais/Handy) (MIT code; name/logo/icon NOT MIT — no distribution before rebrand ships)
- **Internal crate name is deliberately `handy`.** Do NOT rename it. The product name (`Locution`) is set in `tauri.conf.json`; the Cargo package stays `handy`.
- **Full repo facts, build commands, and verified gotchas** are in `.local/memory/project-context.md`. Read it.

## Hard rules (enforced unconditionally)

1. **No push, no PR without explicit approval.** Prepare branch, commit, title, description — stop and wait for go-ahead. "Work autonomously" does not authorize pushing.
2. **v0.1.2 release is HELD.** Do NOT run `gh workflow run release.yml` without Derek's explicit confirmation for that specific release.
3. **Bindings regen contract.** After any Rust `#[tauri::command]` add or remove, run `cd src-tauri && cargo run` to regenerate `src/bindings.ts`. Never hand-edit `bindings.ts`. Verify the diff before claiming done.
4. **Commit per logical phase.** Gate every commit on `bunx tsc --noEmit` + `cargo check` (or `cargo clippy`). Show the output — don't just assert it passed.
5. **DB migrations are append-only.** Never edit existing `MIGRATIONS` entries. Add a new entry for each schema change.
6. **Release workflow requires explicit approval every time.** `release.yml` is `workflow_dispatch`-only (not triggered by tags). Treat it as a destructive action.

## Memory startup

At session start, read:

- `.local/memory/preferences.md` — workflow rules and release holds
- `.local/memory/recent-context.md` — active focus, open threads, recent milestones
- `.local/memory/project-context.md` — repo facts, build commands, verified gotchas

If `.local/memory/` is missing, run `./scripts/setup-workspace.sh` to seed it from templates.

Derek's humanizer, PR style, and commit-message standards are in his user memory at `~/.copilot/`. Those apply here too — check them before drafting any user-facing prose or PR content.

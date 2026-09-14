# Locution — Copilot Instructions

## What this repo is

Locution is a fully-local macOS dictation app: global hotkey → mic capture → Whisper STT → optional LLM cleanup → paste into focused app. Nothing leaves the machine.

- **Stack:** Tauri v2, Rust backend (`src-tauri/`), React/TypeScript frontend, package manager `bun`
- **Forked from:** [cjpais/Handy](https://github.com/cjpais/Handy) (MIT code; name/logo/icon NOT MIT — no distribution before rebrand ships)
- **Internal crate name is deliberately `handy`.** Do NOT rename it. The product name (`Locution`) is set in `tauri.conf.json`; the Cargo package stays `handy`.
- **Full repo facts, build commands, and verified gotchas** are in `.local/memory/project-context.md`. Read it.

## Hard rules (enforced unconditionally)

1. **No push, no PR without explicit approval.** Prepare branch, commit, title, description — stop and wait for go-ahead. "Work autonomously" does not authorize pushing.
2. **No release without explicit approval.** No version-specific hold is active now — the v0.1.2/v0.1.3 drafts were superseded and never published (v0.1.4 onward shipped). Never dispatch `release.yml` without Derek naming the version (see rule 6).
3. **Bindings regen contract.** After any Rust `#[tauri::command]` add or remove, run `cd src-tauri && cargo run` to regenerate `src/bindings.ts`. Never hand-edit `bindings.ts`. Verify the diff before claiming done.
4. **Commit per logical phase.** Run the applicable validation tier below and report the observed result before committing.
5. **DB migrations are append-only.** Never edit existing `MIGRATIONS` entries. Add a new entry for each schema change.
6. **Release workflow requires explicit approval every time.** `release.yml` is `workflow_dispatch`-only (not triggered by tags). Treat it as a destructive action.

## Validation by scope

- **Frontend changes:** run `bun run check:frontend`. Add the relevant Playwright test while iterating on behavior.
- **Rust changes:** run `bun run check:rust` and the narrowest relevant Rust test. Strict Clippy treats warnings as errors.
- **Nix or packaging changes:** run `bun run check:nix`. It verifies `.nix/bun.nix` synchronization and evaluates the Linux package derivation.
- **PR readiness:** run `bun run check:pr`. It includes fast frontend and Rust checks, Rust tests, and Playwright.
- **Native acceptance:** run `bun run check:acceptance` only when startup, packaging, migrations, bundled resources, or Tauri configuration changed. It adds the packaged native smoke test to the PR tier.

## Memory startup

At session start, read:

- `.local/memory/preferences.md` — workflow rules and release holds
- `.local/memory/recent-context.md` — active focus, open threads, recent milestones
- `.local/memory/project-context.md` — repo facts, build commands, verified gotchas

If `.local/memory/` is missing, run `./scripts/setup-workspace.sh` to seed it from templates.

Derek's humanizer, PR style, and commit-message standards are in his user memory at `~/.copilot/`. Those apply here too — check them before drafting any user-facing prose or PR content.

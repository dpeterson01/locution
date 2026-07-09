# Preferences

Standing rules for this repo. Never overwrite without explicit "remember this" from Derek.

## Workflow rules

- **No push, no PR without explicit approval.** Prepare branch, commit, title, description — then stop and wait for go-ahead.
- **Hold any release until Derek confirms.** Do NOT dispatch `gh workflow run release.yml` on your own.
- **Commit per logical phase.** Gate each commit on `bunx tsc --noEmit` and `cargo check` (or `cargo clippy`). Show evidence.
- **Bindings regen contract:** after any Rust `#[tauri::command]` add/remove, run `cd src-tauri && cargo run` to regenerate `src/bindings.ts`. Never hand-edit `bindings.ts`.
- **DB migrations are append-only.** Never edit existing `MIGRATIONS` entries. Add a new entry for schema changes.

## Prose and messaging

- **Humanizer pass** on any user-facing content: README, release notes, i18n strings, PR descriptions. Derek's full standards are in `~/.copilot/` user memory — apply them, don't duplicate here.
- No em-dash stacking. No doubled hyphens.
- Concise messages. Break out separate asks rather than bundling.

## What stays private

- `.local/` contents (gitignored). Never copy memory content into a PR description, commit message, or any public artifact.

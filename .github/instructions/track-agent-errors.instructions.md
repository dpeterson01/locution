---
applyTo: "**"
description: "Local-only error capture. When a tool call fails and recovery requires a change in approach, append one structured line to .local/error-log.jsonl so recurring failure modes can be mined and fixed."
---

# Track Agent Errors (local-only)

A lightweight, append-only error log at `.local/error-log.jsonl` captures failures worth learning from, with their root cause attached at the moment it's known. The file is gitignored. It never leaves this machine.

## When to log

Append an entry when **both** are true:

1. A tool call returned an error or an unexpected result, **and**
2. Recovering required a *change in approach*: a different tool, a corrected command, a schema lookup, a routing fix, or it exposed a knowledge or process gap.

This filter keeps the log high-signal. Examples that **qualify**:

- A Rust build failed because of a wrong type or missing import; fixing it required looking up the API.
- A `cargo run` bindings regen silently produced no output until the app was force-quit.
- A `bunx tsc --noEmit` error caught a type mismatch that the edit looked correct on.
- A subagent delegated to create a file and produced no output; a direct `create_file` call fixed it.
- A `git check-ignore` call failed because the working directory was wrong; fixed with an absolute path.

## When **not** to log

- A pure transient blip (timeout, throttle) that succeeded on an identical retry with no change in approach.
- A failure already covered by an existing `/memories/repo/` note, unless recurrence is the new signal (set `"recurring": true`).
- Anything where you did not actually hit the error.

## Schema

One JSON object per line:

```json
{
  "ts": "2026-07-08T22:15:00Z",
  "tool": "run_in_terminal",
  "skill": null,
  "error_class": "cargo-compile",
  "what": "regenerating bindings.ts after adding a new Tauri command",
  "root_cause": "cargo run exited before the app could export bindings; needed explicit Ctrl-C timing",
  "fix": "waited for app window before Ctrl-C; confirmed bindings diff before claiming done",
  "area": "src-tauri/src/commands/",
  "recurring": false
}
```

Field rules:

- `ts`: ISO-8601 UTC, second precision.
- `tool`: the tool or command that failed.
- `skill`: the active skill or workflow if one was running, else `null`.
- `error_class`: one of `auth`, `permission`, `cargo-compile`, `tsc-error`, `bindings-regen`, `subagent-hallucination`, `json-parse`, `timeout`, `tool-misuse`, `scope-violation`, `other`. Add a value only when none fits.
- `what`: one line on what you were trying to do. Describe the *shape*, not the data.
- `root_cause`: best understanding of why it failed.
- `fix`: what resolved it or the workaround used.
- `area`: repo path or domain if relevant, else `null`.
- `recurring`: `true` if this failure mode has happened before.

## Privacy

Never write raw file contents, model output, transcripts, or personal data into the log. Record the shape of the problem, not the data that flowed through it.

## How to append

Append-only telemetry is an explicit exception to the "don't edit files via terminal" rule, scoped to this file only. Use an **absolute path**:

```bash
printf '%s\n' '{"ts":"...","tool":"...","skill":null,"error_class":"...","what":"...","root_cause":"...","fix":"...","area":null,"recurring":false}' >> /Users/derekpeterson/projects/personal/locution/.local/error-log.jsonl
```

Do the append quietly as part of recovering. A one-line mention afterward is enough; do not interrupt the user's task to announce it.

## Reviewing

Read `.local/error-log.jsonl` and group by `error_class` to find recurring failure modes, then route each to its fix (a `/memories/repo/` note, an instruction rule update, etc.).

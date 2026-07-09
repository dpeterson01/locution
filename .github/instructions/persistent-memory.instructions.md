---
applyTo: "**"
description: "Persistent per-machine memory pack at .local/memory/. Read these files at session start to skip repeated context. Write back to recent-context.md after substantive work."
---

# Persistent Memory

This repo has a four-file per-machine memory pack at `.local/memory/`. The directory is gitignored. Templates live at `templates/memory/` and are seeded by `scripts/setup-workspace.sh`, which only copies files that are missing and never overwrites existing ones.

## The four files

| File | Read for | Written by |
| --- | --- | --- |
| `.local/memory/recent-context.md` | Picking up where the last session left off; open threads and deferred decisions | Agent after substantive work (see Write rules) |
| `.local/memory/preferences.md` | Workflow rules, prose standards, release holds | Derek only. Never overwrite without explicit "remember this." |
| `.local/memory/project-context.md` | Repo facts, build commands, verified gotchas, bindings contract | Derek or agent when new gotchas are confirmed |
| `.local/memory/roster.md` | Owner info, SSH alias, upstream | Rarely changes |

## Read rules

At session start, read all four files. They are short. Skim for the facts relevant to the current task:

1. Check `recent-context.md` for the active focus, release holds, and open threads.
2. Check `preferences.md` for workflow rules that apply to this task (push approval, release hold, commit gating).
3. Check `project-context.md` for any gotcha that could affect the planned approach.
4. Check `roster.md` only if you need to construct a git remote or `gh` CLI command.

If a file is missing, do not error. Proceed without it and offer to seed the pack by running `scripts/setup-workspace.sh`.

## Write rules

Append to `.local/memory/recent-context.md` when **all** of these are true:

1. The session produced a substantive artifact (feature shipped, release cut, decision made, phase completed).
2. The artifact resolved or opened a thread that future sessions will benefit from knowing about.
3. Derek did not opt out.

**Writeback format:** append a dated bullet under the relevant heading. Trim entries older than 30 days unless still open. Do not overwrite the file wholesale; preserve existing notes.

Agents must **not** write to `preferences.md`, `project-context.md`, or `roster.md` without explicit user confirmation.

## Privacy

Memory files never leave the machine. They are gitignored. Never copy memory content into a PR description, commit message, release note, or any public artifact without confirming.

## Failure mode this prevents

Without persistent memory, every session re-derives basics: which model is the Short-tier default, what the bindings regen contract is, whether the release is held. The four-file pack makes that context durable and immediately accessible.

## Seeding from scratch

```bash
./scripts/setup-workspace.sh
```

Idempotent. Creates `.local/memory/` if missing, copies templates for any absent files, prints status. Run once per machine.

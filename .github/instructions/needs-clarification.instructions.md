---
applyTo: "**"
description: "The [NEEDS CLARIFICATION] marker convention. Mark ambiguity explicitly rather than guessing. Markers block completion until resolved."
---

# `[NEEDS CLARIFICATION]` markers

When drafting any artifact in this repo (release notes, README sections, i18n strings, commit messages, instruction files), use explicit markers for unresolved ambiguity rather than producing plausible-sounding content.

## Format

```
[NEEDS CLARIFICATION: specific question]
```

The question must be specific enough that Derek can answer it in one sentence.

**Good:** `[NEEDS CLARIFICATION: target Ollama model for Short tier — is phi4-mini:latest still the default?]`
**Good:** `[NEEDS CLARIFICATION: bundle ID — is com.locution.mac registered or is this still a placeholder?]`
**Bad:** `[NEEDS CLARIFICATION: model unclear]`
**Bad:** `[TBD]` or `[TODO]` (use the marker format above)

## When to mark

- The source material does not specify a fact (version, model name, feature flag, scope boundary).
- The user gave you a placeholder ("we'll figure that out later") instead of a concrete value.
- A required field has no source (e.g., a release note needs a version number that hasn't been confirmed).
- Two sources contradict each other.

## When not to mark

- The fact is clearly stated in an existing file (`project-context.md`, `CLAUDE.md`, `tauri.conf.json`) — record it.
- The fact follows unambiguously from the codebase — derive it.
- A reasonable default exists and the assumption is safe to note inline.

## Rationalization table

| The thought                                                 | The reality                                                                          |
| ----------------------------------------------------------- | ------------------------------------------------------------------------------------ |
| "A reasonable default is obvious here, I'll just write it." | If the default affects scope or behavior, a wrong guess ships as fact. Mark it.      |
| "Marking this makes the draft look unfinished."             | An honest gap is finishable. A plausible fabrication is a landmine.                  |
| "I'll infer it from context and move on."                   | Inference is exactly where drift starts. If the source doesn't state it, mark it.    |
| "Derek will catch it if I'm wrong."                         | The marker is the catch. Removing it transfers your uncertainty into his blind spot. |

## Effects

A file with unresolved markers cannot be committed as a final artifact. Surface the list of markers as a batch before asking Derek to resolve them.

## Resolution workflow

1. Draft the artifact, inserting markers wherever ambiguity exists.
2. Surface all markers as a single batch question set.
3. Replace each marker with the resolved content as Derek answers.
4. Never delete a marker silently. If Derek explicitly defers, replace with `[DEFERRED: question / target resolution date]`.

## Finding unresolved markers

```bash
grep -rn "\[NEEDS CLARIFICATION:" --include="*.md" .
```

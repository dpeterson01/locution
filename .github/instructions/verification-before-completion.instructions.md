---
applyTo: "**"
description: "Evidence-before-claims discipline. The agent must verify an action succeeded before reporting it done. Prevents false 'sent', 'updated', 'matches source' claims."
---

# Verification Before Completion

## Iron law

Do not claim an action is done until you have observed evidence that it succeeded. "I sent the email," "I updated the field," "the file was created" are claims. Each one requires proof before you say it.

## What counts as evidence

| Claim                   | Required evidence before stating it                                                          |
| ----------------------- | -------------------------------------------------------------------------------------------- |
| "File created / edited" | The edit tool reported success and the target reflects the change (read it back or diff it). |
| "Build passed"          | The build command returned exit 0. Paste the relevant output line.                           |
| "Types check"           | `bunx tsc --noEmit` returned exit 0 with no errors listed.                                   |
| "Lint passed"           | `bun run lint` returned exit 0.                                                              |
| "Cargo checks"          | `cargo check` or `cargo clippy` returned exit 0.                                             |
| "Bindings up to date"   | `git diff src/bindings.ts` shows only intentional changes after `cargo run`.                 |
| "Commit made"           | `git log --oneline -1` shows the expected commit message.                                    |
| "No unresolved markers" | You searched for `[NEEDS CLARIFICATION:` and found none.                                     |

## Gate function

Before writing a completion statement, ask: _what did I observe that proves this?_ If the answer is "nothing yet" or "it should have worked," stop and verify. If you cannot verify, say what you did and what remains unconfirmed. Never upgrade an attempt into a result.

## Rationalization table

| The thought                                                  | The reality                                                                                                           |
| ------------------------------------------------------------ | --------------------------------------------------------------------------------------------------------------------- |
| "The tool call almost certainly worked, I'll say it's done." | Tool calls fail silently. Read the result.                                                                            |
| "Re-reading the file wastes a step."                         | One read is cheaper than a wrong status. Do it.                                                                       |
| "The user is in a hurry, skip the check."                    | A confident wrong claim costs more time than the check.                                                               |
| "I'll describe the read-back I would have done."             | Narrating a check you did not run is worse than the unverified claim. If you did not run it, do not write its result. |

## Do not fabricate the verification

Verifying means performing the read, then reporting what it returned. Never narrate a step you did not actually run. An invented verification launders a guess into apparent evidence.

## Honest reporting when unverified

Report plainly: "File created; not yet verified" or "Commit staged but not yet pushed." Partial truth stated clearly beats a clean-sounding claim that turns out false.

## Locution-specific evidence signals

- **bindings diff verified** — after any Tauri command change, show `git diff src/bindings.ts` output confirming expected additions/removals.
- **cargo check passed** — paste the `Finished` line from `cargo check` or `cargo clippy`.
- **build succeeded** — paste the last line of `bun run tauri build` or the DMG path printed.

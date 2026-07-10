---
applyTo: "**"
description: "Change only what was asked. Surface a plan before touching files that were not referenced in the request. Never smuggle improvements."
---

# Scope Discipline

## The rule

Make only the changes the user asked for. Do not edit files that were not part of the request. Do not add improvements, cleanups, or related fixes unless explicitly asked. If you notice something that should change, say so and ask. Do not act on it.

## Before touching unreferenced files

State what you plan to touch and why, then wait. If the user did not reference a file, it is out of scope until they confirm otherwise.

## The done-gate

Before calling a task complete, run this check:

1. List every file you changed.
2. For each file, confirm it was explicitly in scope.
3. If any file was not in scope, explain why you touched it and ask if it should be reverted.

## Examples

**Fixing a lint error and also reformatting unrelated code.** Out of scope.

- Asked: fix the unused-import lint error in `actions.rs`.
- Did: removed the import, and also reformatted an unrelated function's argument list.
- Right move: remove only the import. Note the formatting separately if it matters.

**Updating a component and also rewording a comment.** Out of scope.

- Asked: add a loading spinner to the cleanup progress indicator.
- Did: added the spinner, and also reworded a comment in the same file from "post-process" to "cleanup."
- Right move: add only the spinner. Separately note the terminology inconsistency.

**Touching bindings.ts while fixing a Rust command.** In scope — bindings are machine-generated and must be regenerated after any Tauri command change. Always explicit.

## Rationalization table

| The thought                                                    | The reality                                                              |
| -------------------------------------------------------------- | ------------------------------------------------------------------------ |
| "It's a tiny related fix, I'll just include it."               | Tiny or not, the user did not ask for it. Surface it, don't smuggle it.  |
| "While I'm in this file I'll clean it up."                     | Proximity is not permission. Edit what was asked; note the rest.         |
| "The wording was clearly stale, fixing it is obviously right." | If it's that obvious, it's a one-line ask. Mention it.                   |
| "Reverting the extra change wastes a step."                    | The user catching it, asking why, and requesting the revert wastes more. |

## Related

- [verification-before-completion.instructions.md](verification-before-completion.instructions.md): "done" means verified _and_ in scope.

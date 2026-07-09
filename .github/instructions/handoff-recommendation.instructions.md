---
applyTo: "**"
description: "Detects when a session would benefit from a handoff and surfaces a one-line recommendation for the user to approve. Never creates a handoff on its own."
---

# Handoff Recommendation

Long sessions accumulate context that lives only in the conversation. When that context is at risk of being lost, a handoff is worth writing. This rule makes the agent _notice_ that moment and _offer_ a handoff. It never writes one unprompted.

## The iron rule

Recommend, do not create. Surface a short suggestion and stop. Do not draft, stage, or save a handoff file until the user says yes. A standing "work autonomously" does not authorize creating a handoff.

## Trigger signals

Check these at the end of a turn, not mid-task:

1. **Explicit deferral.** The user parked a thread to finish something else. The parked thread is now context a fresh session would have to re-derive.
2. **Open todos after a shipped artifact.** A commit, release, or feature went out, but the todo list still has open, unrelated items.
3. **Topic fork.** One session covered two or more unrelated threads.
4. **Stepping away.** "Pick this up tomorrow", "I need to step away", end-of-day, or a clear wrap-up.
5. **Context pressure.** The conversation is post-compaction or very long. Weak on its own — only count alongside another signal.

## When to recommend

Surface a recommendation when **two or more** signals fire, or when the user clearly signals stepping away. Cap at **once per session**. If the user declines, do not raise it again that session.

Do **not** recommend when:
- The user is mid-flow on a single task with no parked threads.
- Only one weak signal is present.
- The user already declined this session.
- The work is trivial and fully captured by a commit that already holds the context.

## What the recommendation looks like

One line, naming the signals, ending in a yes/no offer:

> Two threads are parked and there are open backlog items. Want me to write a handoff so a fresh session can pick up the overlay feature work?

> This session covered both the workspace setup and the cleanup-outcome feature. I can write a forked handoff for the feature thread if you want to continue it separately.

Keep it to a sentence or two. Do not pre-write the document.

## On approval

Save to `.local/handoffs/YYYY-MM-DD-<slug>.md`. Pass along the focus (whole session or specific forked thread) the user named. Update `.local/memory/recent-context.md` with a pointer to the new handoff.

## Rationalization table

| The thought | The reality |
| --- | --- |
| "A handoff is clearly useful here, I'll just write it." | Useful is not approved. Offer first. |
| "I'll stage a draft so acceptance is instant." | Staging is still creating. No file until the user says yes. |
| "The session is long, that's enough." | Long does not mean done. One weak signal under-justifies the interruption. |
| "I already offered and they said no, but now it's even longer." | One offer per session. Re-raising is nagging. |

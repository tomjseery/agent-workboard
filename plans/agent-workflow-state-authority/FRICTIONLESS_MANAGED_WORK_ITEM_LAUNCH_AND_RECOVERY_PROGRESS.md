# Frictionless managed Work-item launch and recovery progress

- Plan: `plans/agent-workflow-state-authority/FRICTIONLESS_MANAGED_WORK_ITEM_LAUNCH_AND_RECOVERY_PLAN.md`
- Roadmap: `plans/agent-workflow-state-authority/AGENT_WORKFLOW_STATE_AUTHORITY_ROADMAP.md`
- Roadmap item: `agent-workflow-state-authority/frictionless-managed-work-item-launch-and-recovery`
- Worktree: `C:\Users\TommySeery\source\repos\agent-workboard.worktrees\Feature-Frictionless-Managed-Work-Item-Launch`
- Branch: `Feature/Frictionless-Managed-Work-Item-Launch`
- PR: not opened
- Last reconciled: 2026-08-30 from Git history, Workboard durable state, installed CLI dogfood, and workspace verification

## Current state

All nine implementation phases are present in the local committed candidate. The branch implements managed
checkout reconciliation and isolation, scoped assigned context and per-session capability injection, exact
lifecycle binding, launch profiles, ordered follow-ups, dependency-aware batch fan-out and integration,
workflow/session action projection, actionable start/continue TUI flows, unique short selectors, and real
provider smoke coverage.

The live Workboard database was upgraded through schema 40 and exercised with the current CLI without losing
the legacy session. Exact public binding output uses Workboard session identity and hides native IDs. The
existing legacy session predates scoped credentials, so its downstream durable Work-item transition remains
gated rather than being bypassed.

## Next Steps

Run the canonical review over the merged candidate, resolve any findings, then push the stable head and open
the GitHub PR. Complete the final live managed-session acceptance when Workboard can launch a newly
credentialed session for the blocked Agent Standards Work item.

## Completed work

- Reconciled legacy checkout readiness and added unique short selector support.
- Added authenticated assigned context, scoped capability injection, exact binding generations, profile
  persistence, follow-up delivery, durable launch batches, dependency ordering, writer isolation, and action
  projection.
- Added actionable TUI start/continue choices for existing sessions, new Claude, and new Codex.
- Preserved and reused valid capability-injection behavior rather than duplicating it.
- Committed each natural Work-item boundary through `f3a4b8d`.

## Verification

- `cargo fmt --all -- --check` passed on 2026-08-30.
- `cargo clippy --workspace --all-targets -- -D warnings` passed on 2026-08-30.
- `cargo test --workspace` passed on 2026-08-30: 208 tests, no failures.
- The ignored authenticated Claude smoke completed a non-mutating turn and returned its exact marker.
- The ignored authenticated Codex smoke completed an ephemeral read-only turn and returned its exact marker.
- Live `work continue d6305ca3` selected and resumed the existing Workboard session without launching a
  duplicate provider process.
- `origin/main` was merged at `9a81d79`; formatting, warnings-denied clippy, the complete 208-test workspace
  suite, and both authenticated provider smokes passed again on the merged head.

## Reviews

Canonical post-sync review remains pending.

## Decisions, discoveries, blockers, and deviations

- `_PLAN.md` is the human-readable planning format; Workboard Work items remain execution and checkpoint
  records rather than a competing planning representation.
- The final downstream Agent Standards acceptance remains blocked on a newly launched scoped-credential
  session. The legacy pre-feature session cannot validly supply that evidence.
- The branch is reconciled with `origin/main`; review and PR publication are the remaining delivery gates.

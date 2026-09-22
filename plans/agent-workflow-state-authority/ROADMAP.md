# Agent workflow state authority roadmap

This roadmap makes repository `_PLAN.md` files the single planning format for the Epic while retaining
Workboard Features and Work items as durable execution, assignment, session, and checkpoint records. The
plans below are translated from the published Workboard documents; each plan owns its own current state.

## Outcome

Make Agent Workboard the runtime authority for managed Agent Standards workflows without creating a second
planning language. Managed Claude and Codex sessions start in the correct isolated checkout, receive scoped
context, bind and resume through Workboard identity, and persist execution state through typed operations.
Repositories use ordinary `_PLAN.md` files for human-readable planning.

## Roadmap

- [x] `agent-workflow-state-authority/agent-standards-workboard-first-dual-provider-cutover-recovery` — add
  deterministic managed, unmanaged, and reconciliation-required routing to Agent Standards, including its
  contracts, skills, hooks, worktree tooling, generated mirrors, and offline recovery matrix.
- [ ] `agent-workflow-state-authority/frictionless-managed-work-item-launch-and-recovery` — make `workboard
  work start` and `workboard work continue` launch or resume exact managed Claude and Codex sessions with
  isolated checkouts, scoped capabilities, typed context, profiles, dependency readiness, follow-ups,
  integration, and actionable CLI/TUI recovery.
- [ ] `agent-workflow-state-authority/agent-standards-migration-compatibility-cutover` — prove live Claude and
  Codex parity in Concertable and Vel, preserve the unmanaged compatibility route and rollback evidence,
  then cut Concertable over only after explicit owner acceptance.

## Dependency order

The Agent Standards provider cutover and Agent Workboard managed-lifecycle implementation are parallel
foundations with explicit cross-feature contracts. Both must be accepted before live compatibility dogfood.
Concertable cleanup is the final, separately approved transition and cannot imply contraction of unmanaged
Agent Standards compatibility while any legacy plan remains unfinished or unmigrated.

## Planning source mapping

The source Epic remains
`agent-workflow-state-authority`. Its published Feature and Work-item records are retained so Workboard can
own execution identity and durable workflow transitions. Their planning content is represented by the three
plans above; future design changes update the matching `_PLAN.md` and are then reflected in the execution
records rather than maintained as a competing prose plan.

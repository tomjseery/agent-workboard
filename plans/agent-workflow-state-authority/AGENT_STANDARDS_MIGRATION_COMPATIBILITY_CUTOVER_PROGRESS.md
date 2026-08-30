# Agent Standards migration compatibility cutover progress

- Plan: `plans/agent-workflow-state-authority/AGENT_STANDARDS_MIGRATION_COMPATIBILITY_CUTOVER_PLAN.md`
- Roadmap: `plans/agent-workflow-state-authority/AGENT_WORKFLOW_STATE_AUTHORITY_ROADMAP.md`
- Roadmap item: `agent-workflow-state-authority/agent-standards-migration-compatibility-cutover`
- Delivery repositories: `agent-standards`, `concertable`, `vel`
- Workboard Feature: `9d30120b-bd63-428f-8e74-5fbb08115d50`
- Last reconciled: 2026-08-30 from the published Feature and five Work-item documents

## Current state

Planning is published and all five Work items are ready. Implementation and live acceptance have not started
in the source records. The plan depends on accepted Workboard managed-lifecycle and Agent Standards
dual-provider foundations. Concertable cleanup has an additional explicit owner-approval gate.

## Next Steps

Wait for both foundation plans to reach their accepted state, then execute Phase 1 in Agent Standards to
freeze the legacy compatibility and rollback baseline. Run the Concertable and Vel matrices independently
after that baseline. Reconcile parity before asking for the separate Concertable cleanup approval.

## Completed work

- Published Feature design and five dependency-ordered Work items translated into this plan.
- Shared live-provider matrix, rollback boundary, unrelated-repository requirement, and cleanup approval gate
  identified.

## Verification

No implementation or live acceptance verification has been recorded for this plan yet. Agent Standards
phases require:

- `python .agents/workflows/verify.py --root .`
- `python -m unittest discover -s .agents/hooks/tests -t .agents/hooks/tests -v`
- `pwsh .agents/sync-generated.ps1 -Check`

Concertable and Vel phases additionally require each repository's native verification and the complete live
Claude/Codex acceptance matrix.

## Reviews

No implementation review has been recorded.

## Decisions, discoveries, blockers, and deviations

- Vel is the unrelated-repository acceptance target and cannot be substituted by a cutover participant.
- Parity acceptance does not authorize cleanup.
- Concertable cleanup requires explicit durable owner approval bound to the exact accepted evidence.
- Unmanaged compatibility cannot contract while any legacy plan remains unfinished or unmigrated.

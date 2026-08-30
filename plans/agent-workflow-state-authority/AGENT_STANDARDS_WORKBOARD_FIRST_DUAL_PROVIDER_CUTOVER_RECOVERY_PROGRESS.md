# Agent Standards Workboard-first dual-provider cutover recovery progress

- Plan: `plans/agent-workflow-state-authority/AGENT_STANDARDS_WORKBOARD_FIRST_DUAL_PROVIDER_CUTOVER_RECOVERY_PLAN.md`
- Roadmap: `plans/agent-workflow-state-authority/AGENT_WORKFLOW_STATE_AUTHORITY_ROADMAP.md`
- Roadmap item: `agent-workflow-state-authority/agent-standards-workboard-first-dual-provider-cutover-recovery`
- Delivery repository: `agent-standards`
- Workboard Feature: `ab593e4c-b12f-4e73-be03-4708992cadbd`
- Last reconciled: 2026-08-30 from the published Feature and six Work-item documents

## Current state

Planning is published and all six Work items are ready. No implementation phase is recorded complete in the
source Workboard documents. Two stalled predecessor workflow records and the recorded Workboard/Git checkout
divergence must be frozen in Phase 1 before contract or runtime changes.

## Next Steps

Open or reconcile the isolated Agent Standards delivery checkout, run Phase 1 to capture the immutable
baseline without modifying implementation sources, checkpoint the evidence, then execute Phases 2 and 3 in
order. Phases 4 and 5 can proceed once the managed provider contract is accepted; Phase 6 closes the plan.

## Completed work

- Published Feature design and six dependency-ordered Work items translated into this plan.
- Frozen-head, contract, routing, hook, generator, mirror, and recovery-matrix requirements identified.

## Verification

No implementation verification has been recorded for this plan yet. Each phase must run:

- `python .agents/workflows/verify.py --root .`
- `python -m unittest discover -s .agents/hooks/tests -t .agents/hooks/tests`
- `pwsh .agents/sync-generated.ps1 -Check`

## Reviews

No implementation review has been recorded.

## Decisions, discoveries, blockers, and deviations

- Workflow v2 evolves in place and supports repository and Workboard providers.
- A present invalid managed identity fails closed to reconciliation and never selects repository fallback.
- Runtime managed checkpoints use Workboard only; `_PLAN.md` and `_PROGRESS.md` remain the planning and
  delivery-ledger format rather than an additional checkpoint sink.
- Scoped instructions and structured Workboard operations depend on the sibling frictionless-launch plan.

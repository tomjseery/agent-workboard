# Managed Epic continuation modes

This note records the intended command and workflow boundary. Workboard remains the authority for durable
Epic, Feature, Work-item, session, checkout, dependency, checkpoint, and execution state. This document does
not duplicate live status.

## Interactive continuation

`workboard epic continue` opens one managed Epic-navigation session. It reads the assigned Epic and its
scoped hierarchy, evaluates dependency and readiness state, and recommends the best next existing Work item
or the need for a new Feature. The user chooses before Workboard launches or creates anything. Selecting an
existing Work item launches a Work-item execution session rather than retaining delivery work in the Epic
session.

## Autonomous delivery

`workboard epic deliver` is a distinct opt-in orchestration mode. It repeatedly derives the ready frontier
from Workboard's dependency graph, reserves eligible work, materializes isolated checkouts, launches the
selected provider sessions, monitors durable checkpoints and lifecycle state, schedules review and
integration, and advances to the next frontier until the Epic is terminal or a typed user decision is
required.

The orchestrator may launch independent Work items in parallel. It must not infer completion from provider
idleness, prose, terminal exit, or repository ledgers. It stops on approval, ambiguity, integration conflict,
unsafe checkout state, exhausted provider capacity, or any other explicit decision boundary and exposes the
exact required action.

## Mandatory provider profiles

Every managed planning, execution, review, debugging, and resume request has an explicit persisted launch
profile. The profile contains provider, model, reasoning/effort, role, capability requirements, and applicable
usage policy. Workboard validates the profile against the installed provider before creating a launch intent,
shows the requested and effective profile in preview and session state, and passes it as native arguments.
Missing, unsupported, or silently downgraded models fail closed. Resume reuses the recorded profile unless an
explicit audited profile change creates a new generation.

Policy may recommend or balance Claude and Codex profiles based on role, capability, availability, and usage
budget, but it never substitutes a provider or model invisibly. Interactive commands allow an explicit user
override; autonomous delivery records every policy choice before launch.

## Delivery dependency

Interactive recommendation can land in Workboard independently. Autonomous traversal depends on the Agent
Standards managed-workflow cutover so planning, execution, review, merge, interruption, and recovery loops use
typed Workboard operations and checkpoints instead of `_PLAN.md`, `_PROGRESS.md`, handoff prompts, or
repository-owned continuation state.

## Acceptance

- Epic continuation selects an existing ready Work item without creating a duplicate Feature.
- Epic delivery launches only the dependency-ready frontier and parallelizes only isolated compatible work.
- Planning defaults to an explicitly configured high-capability profile; it can never inherit an invisible
  provider default.
- Every launched session displays provider, model, effort, role, Work item, and checkout before and after
  binding.
- Unsupported profiles, provider drift, interrupted launches, restart, resume, blocked checkpoints, review,
  integration conflict, and usage exhaustion have deterministic fail-closed tests.
- Claude and Codex dogfood proves interactive continuation and autonomous traversal against Concertable and
  an unrelated repository before legacy Agent Standards continuation machinery is removed.

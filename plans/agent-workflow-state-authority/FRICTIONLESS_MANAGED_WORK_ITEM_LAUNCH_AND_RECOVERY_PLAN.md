# Frictionless managed Work-item launch and recovery plan

## Planning identity

- Roadmap: `plans/agent-workflow-state-authority/AGENT_WORKFLOW_STATE_AUTHORITY_ROADMAP.md`
- Roadmap item: `agent-workflow-state-authority/frictionless-managed-work-item-launch-and-recovery`
- Workboard Feature: `8c3d49f6-00da-4a4f-8470-88065964db72`
- Delivery repositories: `agent-workboard`, with final dogfood against `agent-standards`
- Progress: `plans/agent-workflow-state-authority/FRICTIONLESS_MANAGED_WORK_ITEM_LAUNCH_AND_RECOVERY_PROGRESS.md`

The published Workboard Work items remain the durable execution records for these phases. This file is the
human-readable planning authority for their combined design and acceptance contract.

## Outcome

Make installed `workboard` the provider-neutral control plane for the complete managed Work-item lifecycle.
`workboard work start <selector>` and `workboard work continue <selector>` accept a UUID, unique short ID,
key, or interactive search; open an actionable TUI; show exact associated sessions and new Claude/Codex
choices; preselect suitable persisted provider, model, effort, role, and checkout defaults; and launch or
resume from Start without requiring a handwritten implementation prompt.

After selection, Workboard validates dependencies and readiness, reconciles or materializes the correct
isolated checkout, launches with that exact cwd, binds the native CLI without exposing native IDs, injects
only role-scoped Workboard skills/hooks/credential into the managed session, supplies bootstrap instructions,
and makes the agent read its assigned Epic, Feature, Work item, repository instructions, dependencies,
checkout, sessions, and checkpoints through typed operations. Status, checkpoints, blockers, verification,
interruption, restart, resume, and completion remain durable Workboard state.

Every returned command or TUI result presents valid next actions derived from durable workflow state,
authorization, dependency readiness, bindings, checkout isolation, and provider capability. In
`awaiting_approval`, `Approve and publish`, `Request revision`, and `Reject` are primary actions. Hook failures
remain secondary warnings and never hide those choices.

## Retained baseline and boundaries

Retain and regression-test Agent Workboard commits `9eac404b52cf67a8a3454a037fb00f127f23be27`,
`50f30d6a41098d6bb5a33a33adb4bbcea652d72c`, and
`3ad84574fb726994aa5f9fdd331785cc7776cefc`. Preserve exact Codex cwd, scoped hook trust, automatic kickoff,
and the provider-neutral typed contract. Reuse valid capability-injection work from
`feature/managed-session-capability-injection-reconcile` without overwriting another checkout's changes.

The scope includes domain, storage, application operations, Git and provider adapters, generated managed
skills, CLI/TUI, deterministic tests, and real-provider smoke tests. It excludes Agent Standards
implementation, native-ID exposure, an autonomous scheduler, an embedded terminal, automatic publication
launch, automatic conflict resolution, destructive cleanup, and disposal of rollback evidence.

## Cross-cutting contracts

### Dependency and readiness authority

Feature proposals declare a DAG using stable Work-item slugs. Submission rejects unknown, self, duplicate,
and cyclic edges. Publication atomically materializes Work items, documents, repositories, edges, and the
first selection, but never launches a provider. Workboard derives readiness, blockers, parallelizability,
and duplicate-start gates. Single and batch launches require preview and confirmation.

### Checkout isolation and integration

The Feature checkout is the planning and integration checkout, never a shared mutable execution cwd.
Write-capable Work items default to distinct worktrees and branches per Work item and repository. An
additional writer gets a distinct writer-session checkout. Read-only sharing is allowed only when the
provider can enforce it. A batch reconciles every selected checkout before reservation; a preflight failure
spawns and binds nothing until an eligible subset is previewed again.

Accepted branches integrate under one Feature-checkout lease in dependency-layer order, then stable proposal
order and slug. Integration records expected, source, and result heads plus conflicts, never auto-resolves or
reorders, and retains evidence until explicit safe cleanup.

### Action and session projection

One versioned `AvailableActions` projection drives human CLI output, JSON, and TUI behavior. Each action has
a stable kind and label, enabled state, disabled reason, preconditions, target Workboard owner/session,
confirmation requirement, and typed route. Pending operations offer status, reconcile, or cancel rather than
duplicates; terminal states offer inspect, evidence, and safe cleanup only where valid.

Each Work item projects all relevant current, historical, execution, review, debugging, and read-only
sessions with Workboard session ID, provider, model/profile, role, binding/live/restore status, last activity,
checkout purpose/path/branch/generation, resumability, and primary-writer status. Native IDs stay internal.
Zero sessions offer Start or its exact blocker. One offers exact Resume and Start another. Many are
deterministically selectable and retain Work-item-level Start another.

### Binding, profiles, follow-ups, and capability scope

Only the trusted primary SessionStart binds native identity, owner, role, exact checkout/cwd, profile,
request, and association before the first turn. Provider/model/effort profiles are typed, validated before
external mutation, persisted on launch and batch children, and preserved on resume unless an explicit
supported override is recorded.

Managed capabilities are reconciled into a session-owned directory and passed only to that process. No
Workboard credential, hook, or role-scoped skill is installed globally. Bootstrap prompts are generated by
Workboard and require typed assigned-context reads before implementation.

Follow-ups accept Workboard owner/session, text, binding generation, and idempotency only. Workboard resolves
native delivery internally, preserves FIFO pending/delivered/failed state, correlates receipts, and reconciles
before retry so restart cannot blindly duplicate delivery.

## Delivery phases

### Phase 1 — Reconcile and isolate checkouts before launch

Workboard item: `reconcile-checkout-liveness-before-launch`.

Define versioned `CheckoutReadiness` with repository and checkout identities, purpose, access mode, owner,
session, parent Feature checkout, base/source revision, exact path, Git identity, branch/head, availability,
generation, and reconciliation evidence. Persist intent before Git mutation; correct stale availability;
materialize only safe missing or truly empty targets; enforce unique mutable ownership; and reconcile every
launch, resume, recovery, and batch boundary. Prove exact cwd/branch, parallel and additional-writer
isolation, safe read sharing, cross-repository allocation, zero-spawn on failure, restart idempotency,
deterministic integration, cleanup gates, and rollback evidence.

### Phase 2 — Return complete scoped assigned context

Workboard item: `return-scoped-managed-assigned-context`.

Return a versioned `AssignedContext` through matching MCP and request-file CLI shapes. It contains only the
authenticated principal, owner, role, tool and session; dependency status; repository identity; Feature and
execution checkout relationship; normalized instruction paths/hashes/revisions; validated Epic, Feature,
Work-item, and dependency documents; and relevant Workboard session projections. Authenticate token,
association, role, provider, owner, session, checkout, and generation, and fail closed on missing required
content, drift, escape, expiry, or mismatch without repository-state fallback or cross-owner leakage.

### Phase 3 — Harden native lifecycle binding

Workboard item: `harden-native-session-lifecycle-binding`.

Persist a `LifecycleBinding` covering request/intent, owner/role, internal native identity, checkout and
isolation generation, cwd, profile, primary/helper evidence, generation, state, and failure. Make exact
primary SessionStart a synchronous pre-kickoff gate; atomically bind session, association, restore request,
profile, checkout, and observation; keep repeats idempotent and conflicts fatal. Reconcile ordered activity,
Stop, SessionEnd, compact, late events, interruption, restart, and exact resume. Keep Stop diagnostics
separate from workflow actions and preserve Windows Terminal, Codex cwd, trust, and safe argument behavior.

### Phase 4 — Persist provider launch profiles

Workboard item: `persist-provider-launch-profiles`.

Add versioned provider capabilities and launch profiles for model, effort/reasoning, role, source, and
override. Store defaults separately from immutable launch, batch-child, session, and history facts. Validate
adapter support before mutation, keep argument mapping separate from prompt/path/trust, preserve profiles on
resume, and represent legacy sessions as unknown rather than inventing values. Cover storage, migration,
idempotency, hostile input, adapters, CLI/TUI/JSON, batch, recovery, and all session cardinalities.

### Phase 5 — Deliver ordered bound-session follow-ups

Workboard item: `deliver-bound-session-follow-ups`.

Add generated `session_send_follow_up` MCP and request-file operations using owner, optional Workboard session
selector, expected binding generation, bounded text, and idempotency. Reject native IDs, executables, homes,
provider commands, ambiguity, stale binding/checkout generations, helpers, and unauthorized owners or roles.
Persist FIFO sequence, hashes, attempts, leases, receipts, states, and failures. Queue without tab switching or
active-turn interruption; mark delivered only on correlated receipt; reconcile before retry; and resume FIFO
delivery after restart for both Claude and Codex.

### Phase 6 — Materialize dependency readiness and parallel launch fan-out

Workboard item: `materialize-dependencies-and-parallel-launches`.

Materialize the validated DAG and project dependencies, dependants, layers, readiness, blockers, active
ownership, and parallelizability. Preview exact access, integration parent, isolated checkout/branch/base/cwd,
context, profile, sessions, and terminal target. Preflight all checkouts, then atomically revalidate and reserve
children before bounded fan-out. Report bound, awaiting, failed, skipped, blocked, and partial outcomes
truthfully; restart and resume without duplicates. Serialize integration and preserve conflicts and rollback.

### Phase 7 — Project valid workflow and session actions

Workboard item: `project-workflow-and-session-next-actions`.

Define action mappings for Draft, WorktreePending, PlanningLaunchPending, PlanningActive, ProposalReady,
AwaitingApproval, Publishing, Planned, WorkItemLaunchPending, WorkItemActive, ReconciliationRequired, Blocked,
Paused, Completed, and Cancelled. Split revision from rejection. Make approval and publication distinct durable
transitions inside the selected composite, with publication failure returning reconcile/retry without false
success. Cover stable ordering, authorization, disabled reasons, commands, JSON, hook diagnostics, and the
complete zero/one/many session behavior including explicit isolated additional writers.

### Phase 8 — Deliver the actionable TUI and short commands

Workboard item: `deliver-actionable-workboard-tui`.

Make no-subcommand TUI, `work start`, and `work continue` consume the typed projection. Show workflow state
and primary actions before diagnostics; provide searchable UUID/short-ID/key selection; render complete
session evidence; compose Start another from role/access/provider/profile; revalidate every selected action;
and support graph, batch, integration, cleanup, follow-up, recovery, narrow terminals, and no-color output.
Human and JSON CLI use the same data. Add deterministic Ratatui, CLI golden, fake terminal, and installed
provider smoke coverage.

### Phase 9 — Accept the zero-friction lifecycle

Workboard item: `accept-zero-friction-managed-lifecycle`.

Run the installed Claude/Codex lifecycle against every workflow state and zero, one, and many session
cardinalities. Prove approval/revision/rejection, Stop-hook warning behavior, exact start/resume, isolated
second writers, parallel and cross-repository work, safe read sharing, profiles, fan-out partial recovery,
integration conflicts and cleanup, follow-up FIFO/receipts, identity failures, and truthful actions.

After all upstream gates, dogfood the blocked Agent Standards Work item in its isolated checkout, ask for the
final Start/Resume/Start another choice, bind exact cwd before first turn, prove scoped context and one default
primary writer, send a follow-up, checkpoint through Workboard, and retain integration and rollback evidence.
Only this accepted evidence may authorize the downstream Work item.

## Verification and acceptance

- Deterministic tests cover schemas, migrations, storage, action projection, selector resolution, lifecycle
  hooks, capability isolation, context authorization, checkout reconciliation, profiles, fan-out, integration,
  follow-ups, recovery, CLI/TUI rendering, and Claude/Codex adapter command construction.
- Real-provider smoke tests make authenticated, non-mutating Claude and Codex turns from Windows Terminal
  compatible command lines and verify the expected marker response.
- `cargo fmt --all -- --check` passes.
- `cargo clippy --workspace --all-targets -- -D warnings` passes.
- `cargo test --workspace` passes.
- Installed dogfood proves exact managed cwd, no native-ID exposure, no global capability installation, no
  handwritten kickoff prompt, durable checkpoints, exact resume, and visible valid next actions.

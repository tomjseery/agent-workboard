# Agent Standards migration compatibility cutover plan

## Planning identity

- Roadmap: `plans/agent-workflow-state-authority/AGENT_WORKFLOW_STATE_AUTHORITY_ROADMAP.md`
- Roadmap item: `agent-workflow-state-authority/agent-standards-migration-compatibility-cutover`
- Workboard Feature: `9d30120b-bd63-428f-8e74-5fbb08115d50`
- Delivery repositories: `agent-standards`, `concertable`, and an isolated `vel` acceptance checkout

The published Workboard Work items remain the durable execution records for these phases. This file is the
human-readable planning authority for their combined design and acceptance contract.

## Outcome

Prove Agent Workboard-managed Agent Standards workflows behave correctly with both Claude and Codex in
Concertable and in Vel as a genuinely unrelated consumer repository. Preserve deterministic unmanaged
compatibility and every legacy recovery asset until migration is genuinely complete. This is the live
compatibility and acceptance stage after the Workboard managed-lifecycle and Agent Standards dual-provider
plans; it does not recreate their typed operations or provider implementation.

## Authority and safety boundaries

- Freeze and characterize legacy compatibility before live cutover or cleanup.
- Exercise managed sessions through authenticated typed Workboard operations. Invalid, expired, unauthorized,
  or mismatched identities fail closed to reconciliation and never fall back to repository state.
- Use Concertable and Vel for the parity matrix. Agent Workboard, Agent Standards, and the planning store do
  not qualify as the unrelated-repository target.
- Preserve the unmanaged repository route while any legacy plan is unfinished or unmigrated.
- Keep approval, publication, execution-session creation, checkpointing, review, delivery, and terminal
  transitions distinct and durable.
- Managed runtime checkpoints do not incidentally mutate repository plans or progress ledgers.
- Preserve the pre-cutover Workboard database, planning store, Concertable planning/recovery artifacts, and
  Agent Standards legacy migration corpus throughout acceptance and cleanup.
- Parity acceptance and Concertable cleanup are separate transitions. Cleanup requires Tommy's explicit
  approval bound to the accepted evidence.

## Shared live acceptance matrix

Both providers and both repositories must exercise create, assigned-context read, checkpoint, interruption,
process restart, exact-session resume, review, delivery, recovery, identity mismatch, and no-incidental-plan-
mutation behavior using the same observable gates and terminal criteria. Evidence records operation versions,
repository heads, Agent Standards commit, generated mirror state, binding and checkout identities, failures,
known limitations, rollback point, and a pass/fail result for every row.

## Delivery phases

### Phase 1 — Capture the legacy compatibility and rollback baseline

Workboard item: `capture-legacy-compatibility-and-rollback-baseline`.

Inventory the Agent Standards legacy migration corpus, blocked plan-state work, prerequisite acceptance,
roadmap references, plan-aware skills, hooks, worktree/delivery tooling, Workflow v2 repository contracts,
generated mirrors, and deterministic tests. Classify every live legacy plan as finished, migrated, or still
requiring unmanaged compatibility. Capture immutable identities and a read-only restore/reconciliation
procedure for the Workboard database, planning store, Agent Standards head, and rollback-critical repository
artifacts. Define the shared Claude/Codex live matrix and explicitly preserve every legacy artifact.

Verify the inventory is complete and mechanically gates future compatibility contraction; both provider
matrices use the same criteria; restore evidence is usable; Agent Standards verification and hook tests pass;
generated copies are clean; and no legacy artifact is deleted, rewritten, or de-authorized.

### Phase 2 — Dogfood Claude and Codex against Concertable

Workboard item: `dogfood-managed-claude-and-codex-against-concertable`.

After both foundation plans and the baseline are accepted, launch equivalent disposable Work items through
native Claude and Codex in isolated Concertable checkouts. Prove exact authenticated owner, scoped documents
and repository instructions, effective checkout, proposal/publication separation, execution-session creation,
durable checkpoints, interruption, restart, exact resume, review, delivery, recovery, and completion. Inject
missing, expired, wrong-owner, wrong-role, and wrong-checkout identity cases. Confirm idempotent revision-
checked state, reconciled document/session/checkout identity, no incidental repository-plan mutation, and no
deletion of existing Concertable planning or recovery evidence.

### Phase 3 — Dogfood Claude and Codex against Vel

Workboard item: `dogfood-managed-claude-and-codex-against-vel`.

Register Vel using its real Git identity and obtain an isolated Workboard checkout. Keep acceptance-harness
changes in Agent Standards while running equivalent disposable Claude and Codex Work items in Vel. Prove
scoped instructions and checkout resolution under a distinct toolchain and repository surface, run the same
lifecycle and identity-failure matrix, and ensure no Workboard database path, planning-store path, identifier,
or acceptance-only state is committed to Vel. Run Vel's native verification and the Agent Standards checks
for harness changes.

### Phase 4 — Record managed-provider parity acceptance

Workboard item: `record-managed-provider-parity-acceptance`.

Compare every matrix row by provider, repository, interruption boundary, and failure mode. Route discrepancies
to the owning upstream plan and rerun affected rows; do not accept prose exceptions. Record exact operation
versions, commits, heads, generated mirrors, evidence references, limitations, rollback point, and legacy
plans still depending on unmanaged compatibility. Accept parity only when Claude and Codex have equivalent
observable behavior in both repositories. Do not perform cleanup or narrow the unmanaged provider here.

### Phase 5 — Cut Concertable over from migrated machinery

Workboard item: `cut-over-concertable-from-migrated-planning-machinery`.

Enter only after parity acceptance and Tommy's explicit durable approval match the evidence and effective
Concertable checkout. Otherwise checkpoint the blocker and make no repository change. Remove only proven
migrated planning, handoff, recovery, hook, instruction, and routing surfaces from active selection. Keep old
database, planning store, repository plans, progress ledgers, recovery artifacts, identities, and restore
procedure intact. Leave unfinished or unmigrated plans on the compatibility route and do not contract Agent
Standards unmanaged behavior.

After the routing change, rerun managed Claude/Codex smokes from Concertable, prove an explicit unmanaged
legacy case still works, run repository tests and generated checks, and rehearse rollback or provide an
equivalent read-only proof.

## Completion

Both repositories must pass the full managed lifecycle and failure matrix for Claude and Codex, parity must
be durably accepted, and separately approved Concertable cleanup must remove migrated machinery from active
routing without deleting or rewriting rollback evidence. Agent Standards unmanaged compatibility remains
available until every legacy plan is finished or migrated.

## Current delivery state

Planning is published and its five Work items are ready. It begins only after the Workboard managed-lifecycle
and Agent Standards dual-provider foundations are accepted. Concertable cleanup remains separately gated on
Tommy's explicit durable approval after parity evidence is accepted.

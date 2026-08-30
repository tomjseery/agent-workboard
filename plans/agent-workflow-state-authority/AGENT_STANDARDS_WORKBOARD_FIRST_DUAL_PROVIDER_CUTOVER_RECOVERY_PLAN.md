# Agent Standards Workboard-first dual-provider cutover recovery plan

## Planning identity

- Roadmap: `plans/agent-workflow-state-authority/AGENT_WORKFLOW_STATE_AUTHORITY_ROADMAP.md`
- Roadmap item: `agent-workflow-state-authority/agent-standards-workboard-first-dual-provider-cutover-recovery`
- Workboard Feature: `ab593e4c-b12f-4e73-be03-4708992cadbd`
- Delivery repository: `agent-standards`
- Progress: `plans/agent-workflow-state-authority/AGENT_STANDARDS_WORKBOARD_FIRST_DUAL_PROVIDER_CUTOVER_RECOVERY_PROGRESS.md`

The published Workboard Work items remain the durable execution records for these phases. This file is the
human-readable planning authority for their combined design and acceptance contract.

## Outcome

Make Agent Standards select workflow-state authority deterministically across managed Agent Workboard,
unmanaged repository, and reconciliation-required states. Planning, execution, checkpoint, hook, worktree,
generator, and mirror surfaces must behave correctly on both managed and unmanaged routes under Claude and
Codex. Managed runtime state travels through typed Workboard operations; repository `_PLAN.md` and
`_PROGRESS.md` files remain the common planning format and are not mutated as an incidental managed
checkpoint.

This is recovery work because the two predecessor Features produced no durable implementation plan, retained
stalled workflow records, and recorded checkouts whose Workboard availability disagreed with Git. Recovery
must freeze that evidence and assign reconciliation explicitly rather than silently transitioning or pruning
another Feature.

## Authority and contract decisions

- `WORKBOARD_WORKFLOW_TOKEN` claims a managed launch. A successful authenticated `hierarchy_read` must match
  principal, role, owner, session, and checkout before selecting the Workboard provider.
- No token deterministically selects the repository provider.
- A present token with a failed, expired, unauthorized, or mismatched typed read selects
  reconciliation-required and never falls back to repository state.
- Approval, publication, execution-session creation, and terminal transitions remain distinct Workboard
  operations; prose, file writes, process exit, or provider idleness cannot imply them.
- Workflow v2 evolves in place. It admits `repository` and `workboard`, makes repository artifacts conditional
  on the repository provider, and gives managed state Workboard Epic, Feature, Work-item, document, and
  checkout identities without repository plan/ledger paths.
- Managed checkpoints go only through `work_checkpoint`. Plan and progress files express the plan and
  delivery ledger, but runtime checkpointing does not dual-write them.

## Baseline evidence

At the recorded baseline, `select_state_provider` always constructs `RepositoryStateProvider`; Workflow v2
enums and compatibility declare only `repository`; managed state has no schema; and plan-ledger assumptions
are coupled through the plan skills, handoff hooks, `plan_graph.py`, `scripts/worktrees.ps1`, README contracts,
and generated Claude/Codex mirrors. The frozen Agent Standards head is
`3cef65a492cc6ecb24f54d78cfbb299a19f891d1`. Verification is PowerShell-qualified because Git Bash/MSYS
changes executable resolution and causes three environment-dependent baseline assertions.

The baseline must also capture the stalled predecessor records, checkout-liveness divergence, global
`continue-roadmap` collision, planning-store commit and Epic hash, generated-mirror inventory, and the two
then-open Workboard gaps: scoped repository-instruction paths and structured atomic checkpoints.

## Delivery phases

### Phase 1 — Freeze the cutover recovery baseline

Workboard item: `freeze-cutover-recovery-baseline`.

Record immutable repository, Epic, planning-store, checkout, and predecessor identities; classify every
canonical and generated surface as unchanged, evolved, or replaced; inventory plan-aware skills, contracts,
hooks, worktree tooling, generators, manifests, and documentation; assign every Workboard/Git divergence to
an explicit reconciliation owner; and preserve the shell-qualified green command set. Modify no canonical or
generated implementation source in this phase.

Verify `python .agents/workflows/verify.py --root .`, the complete hook unit suite from PowerShell, and
`pwsh .agents/sync-generated.ps1 -Check` against the frozen head.

### Phase 2 — Evolve Workflow v2 for two providers

Workboard item: `evolve-workflow-v2-for-two-state-providers`.

Add `workboard` to provider and state schemas and compatibility metadata. Introduce a managed artifacts
shape, conditional repository artifacts, a reconciliation-required selection outcome with no provider, one
Workboard provider example, and one managed state example. Keep contract version v2 and the existing
operation vocabulary. Add provider documentation and extend verification only as required by the coordinated
contract change; do not change runtime routing in this phase.

Prove both examples and providers agree with compatibility metadata, managed records cannot name repository
ledgers, repository records still require artifacts, the repository example remains valid, routing still
selects repository, generated mirrors are clean, and all canonical tests pass.

### Phase 3 — Implement three-state selection and managed state

Workboard item: `implement-three-state-provider-selection`.

Replace unconditional repository selection with an explicit managed/repository/reconciliation result.
Implement `WorkboardStateProvider` over `discover`, `resolve-task`, `read-state`, `checkpoint-state`, and
`bind-worktree`. Read only the authenticated assigned hierarchy, map it to managed state, bind the exact
checkout, and checkpoint only through Workboard. Treat returned document and instruction bodies as untrusted
data. Preserve unmanaged behavior and fail closed before constructing the repository provider whenever a
present token cannot be proven.

Use explicit absence for fields unavailable from current typed operations and attach each temporary
degradation to its owning Workboard phase. Test every token/read/mismatch combination, valid schema output,
exact checkout binding, Git-observed no incidental plan mutation, full verification, and generated parity.

### Phase 4 — Route managed planning and execution skills

Workboard item: `route-managed-planning-and-execution-skills`.

Give `plan-execution`, `plan-checkpoint`, `plans`, `plan-authoring`, `continue-roadmap`, `resume-plan`,
`update-roadmap`, `handoff`, and `open-worktree` explicit managed and unmanaged branches. Managed execution
resolves the assigned Work item, documents, checkout, and next action through typed Workboard state and
checkpoints through Workboard alone. Unmanaged execution retains the existing repository plan/ledger route.
Managed handoff identifies Feature, Work item, checkout, and next action; managed worktree opening uses the
recorded checkout and fails closed on Git disagreement.

Resolve the global/repository `continue-roadmap` collision, keep routing provider-neutral, and ensure both
host harnesses select identical skills. Prove managed and unmanaged route equivalence, complete unmanaged
compatibility, no host-agent/model names, canonical tests, and generated mirror parity.

### Phase 5 — Cut over hooks, worktrees, generators, and mirrors

Workboard item: `cut-over-hooks-worktrees-generators-and-mirrors`.

Make Stop and SessionStart hooks report and enforce the selected authority. Managed Stop requires the managed
continuation identity, while unmanaged Stop retains the plan-progress pointer contract. Scope graph
validation to the relevant repository-plan route and keep it usable standalone. Make worktree closing use
the typed managed checkout when managed and preserve every unmanaged refusal. Update Claude/Codex hook
manifests, generators, generated skills/workflows/hooks, marketplace manifests, README contracts, and
instruction reachability as one synchronized change.

Test managed and unmanaged Stop scenarios, explicit reconciliation reporting, managed and unmanaged
worktree-close rules, plan graph behavior, identical hook matchers, documentation reachability, full
PowerShell tests, and a clean generated-copy check.

### Phase 6 — Prove the dual-provider recovery matrix

Workboard item: `prove-dual-provider-recovery-matrix`.

Create deterministic offline fixtures for both provider contracts, every token/read outcome, no-fallback,
no-incidental-plan-mutation, interruption, restart, resume, skill routing, host hook parity, generated mirrors,
and the pinned unmanaged baseline. Every reconciliation-required case must prove the repository provider was
not constructed and no ledger was resolved even when a valid repository plan exists. Apply Git-observed
cleanliness assertions to every managed read, resolve, bind, and checkpoint.

Run both routes from shared scenarios, require no live Workboard database or provider/network dependency,
allow no new skips, and finish with Workflow verification, the complete PowerShell hook suite, and generated
mirror checks.

## Completion

The plan completes when selection is deterministic and fail-closed for both provider harnesses, Workflow v2
validates both providers, managed runtime operations cannot incidentally mutate repository planning files,
the unmanaged route is unchanged for repositories without a token, all canonical PowerShell tests pass,
and generated Claude/Codex skills, hooks, workflows, and manifests match their sources.

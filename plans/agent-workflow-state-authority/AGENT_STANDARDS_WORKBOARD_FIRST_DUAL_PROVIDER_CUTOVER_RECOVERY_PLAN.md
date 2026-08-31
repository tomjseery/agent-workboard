# Agent Standards Workboard-first dual-provider cutover recovery plan

## Planning identity

- Roadmap: `plans/agent-workflow-state-authority/AGENT_WORKFLOW_STATE_AUTHORITY_ROADMAP.md`
- Roadmap item: `agent-workflow-state-authority/agent-standards-workboard-first-dual-provider-cutover-recovery`
- Workboard Feature: `ab593e4c-b12f-4e73-be03-4708992cadbd`
- Delivery repository: `agent-standards`

The published Workboard Work items remain the durable execution records for these phases. This file is the
human-readable planning authority for their combined design and acceptance contract.

## Outcome

Make Agent Standards select workflow-state authority deterministically across managed Agent Workboard,
unmanaged repository, and reconciliation-required states. Planning, execution, checkpoint, hook, worktree,
generator, and mirror surfaces must behave correctly on both managed and unmanaged routes under Claude and
Codex. Managed runtime state travels through typed Workboard operations; repository planning remains the
common format and is not mutated as an incidental managed checkpoint.

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
- Managed checkpoints go only through `work_checkpoint`. Runtime checkpointing does not create a second
  planning record.

## Baseline evidence

At the recorded baseline, `select_state_provider` always constructs `RepositoryStateProvider`; Workflow v2
enums and compatibility declare only `repository`; managed state has no schema; and plan-ledger assumptions
are coupled through the plan skills, handoff hooks, `plan_graph.py`, `scripts/worktrees.ps1`, README contracts,
and generated Claude/Codex mirrors. The frozen Agent Standards head is
`3cef65a492cc6ecb24f54d78cfbb299a19f891d1`. Verification is PowerShell-qualified because Git Bash/MSYS
changes executable resolution and causes three environment-dependent baseline assertions.

The baseline must also capture the stalled predecessor records, checkout-liveness divergence, planning-store
commit and Epic hash, generated-mirror inventory, and the two then-open Workboard gaps: scoped
repository-instruction paths and structured atomic checkpoints.

## Delivery phases

### Phase 1 — Freeze the cutover recovery baseline

Workboard item: `freeze-cutover-recovery-baseline`.

Status: complete on 2026-08-30. The evidence was captured without changing Agent Standards canonical or
generated implementation sources.

Frozen identities:

- Agent Standards head: `3cef65a492cc6ecb24f54d78cfbb299a19f891d1`.
- Epic: `agent-workflow-state-authority`, document `c7502a6c-0308-4c65-a040-bd12ea7f2b4b`, content hash
  `ca9fd891730f6381fb17aedfc0fa2ae4ab2c9b72f5a2986a45b9bcc134c74c76`.
- Planning store: `5617f5b2c9db7124870b7bf0409a99eefee84880`. The Epic document itself remains observed at
  `531ca002f9f07b71d09835911e4123682e3bf16a`; the later commit published this Feature and its Work items
  without changing the Epic body.
- Feature checkout: `3844560f-c2f7-46cd-af88-5d3a593881a1`, branch
  `Feature/Agent-Standards-Workboard-First-Cutover-Recovery`, at the frozen head and present in Git.
- Phase checkout: `44e17788-c13e-4ee1-ae87-2b0fd9b2805a`, branch
  `work-item/bc6f9a65-303e-4f40-81b3-ad900cf8680b`, at the frozen head and present in Git.

The two predecessors remain preserved. `workboard-managed-agent-standards-cutover` is `planning_active`,
has no document, and retains Claude planning session `27479219-a0b5-48e0-80a2-509aa9fe10f6`.
`agent-standards-dual-provider-workboard-cutover` is `planning_launch_pending`, has no document, and has no
associated launch. Neither produced durable planning output, so this recovery supersedes or discards no
artifact.

Workboard nevertheless marks the absent `Feature-Agent-Standards-Dual-Provider-Cutover` and
`Feature-Agent-Standards-Migration-Compatibility-Cutover` checkouts available. Git lists neither worktree.
Reconciliation belongs to `frictionless-managed-work-item-launch-and-recovery/reconcile-checkout-liveness-before-launch`:
observe each missing checkout, mark or replace its Workboard record through the owning lifecycle, and never
transition or prune either predecessor from this Feature.

Every canonical inventory entry was present and classified `unchanged`: `workflow_runtime.py`; Workflow v2
provider, state, host, dispatch, and result schemas, compatibility metadata, and examples; the repository
provider contract; the nine named plan-aware skills; the five named hook modules; Claude and Codex hook
manifests; worktree tooling; the generated-copy script; and README contracts. The originally named Codex
manifest path was corrected from nonexistent `.agents/hooks/codex-hooks.json` to canonical
`.codex/hooks/codex-hooks.json`. Every generated mirror named in this plan was also present and `unchanged`.
Nothing was classified `evolved` or `replaced` in this evidence-only phase.

The two upstream limitations remain explicit. Scoped assigned context belongs to
`frictionless-managed-work-item-launch-and-recovery/return-scoped-managed-assigned-context`.
`work_checkpoint` still accepts only an opaque summary and coarse next-action enum; no published sibling Work
item yet owns its structured atomic replacement, so later managed checkpoint work must retain this as an
unresolved dependency rather than inventing a second authority.

Frozen-head PowerShell verification is green: Workflow verification reports one provider, eight examples,
five schemas, and contract v2; the complete hook suite reports 424 tests passing with one skip; and the
generated-copy check reports 299 files current, including 73 skills and 29 docs. Git Bash remains excluded
from the baseline because MSYS executable resolution changes three environment-sensitive assertions.

Record immutable repository, Epic, planning-store, checkout, and predecessor identities; classify every
canonical and generated surface as unchanged, evolved, or replaced; inventory plan-aware skills, contracts,
hooks, worktree tooling, generators, manifests, and documentation; assign every Workboard/Git divergence to
an explicit reconciliation owner; and preserve the shell-qualified green command set. Modify no canonical or
generated implementation source in this phase.

Verify `python .agents/workflows/verify.py --root .`, the complete hook unit suite from PowerShell, and
`pwsh .agents/sync-generated.ps1 -Check` against the frozen head.

### Phase 2 — Evolve Workflow v2 for two providers

Workboard item: `evolve-workflow-v2-for-two-state-providers`.

Status: complete on 2026-08-30 in Agent Standards commit
`1dd28657c88cfc98b51b61f6a24da90ab1a60dc4` (`feat(workflow): evolve Workflow v2 contract for a second
state provider`), published on `origin/work-item/8e31b124-544d-4dca-988e-2e5199b40360`.

The coordinated contract change evolved 19 files. Canonical changes are
`.agents/workflows/contract/v2/capabilities.json`, `compatibility.json`, `provider.schema.json`,
`state.schema.json`, the new `examples/provider.workboard.example.json` and
`examples/state.workboard.example.json`, `.agents/workflows/providers/repository.md`, the new
`.agents/workflows/providers/workboard.md`, `.agents/workflows/workflow_runtime.py`, and
`.agents/hooks/tests/test_workflow_contracts.py`. The equivalent nine Workflow files under
`plugins/workflow/workflows` are generated mirrors.

Workflow v2 now declares `repository` and `workboard` providers over the same five operations and declares
`repository`, `workboard`, and `reconciliation-required` selection outcomes, with reconciliation yielding no
provider. Repository state still requires its plan, ledger, roadmap, and reviews artifacts. Workboard state
requires Epic, Feature, Work-item, document, and checkout identities and rejects repository artifact fields.
The repository provider example remains valid, each provider has exactly one matching probe example, and the
new managed state example validates. Runtime routing did not change: `select_state_provider` still constructs
`RepositoryStateProvider`.

PowerShell verification is green at the commit: Workflow verification reports contract v2, two providers,
ten examples, and five schemas; the complete hook suite reports 426 tests passing with one skip; and the
generated-copy check reports 302 files current, including 73 skills and 29 docs. An independent acceptance
rerun on 2026-08-30 repeated Workflow verification, the provider-specific artifact tests, the provider/outcome
agreement test, the unchanged repository-routing test, and generated-copy verification successfully.

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

Status: complete on 2026-08-30 in Agent Standards commit `a53f470`
(`feat(workflow): select managed state authority`).

`select_state_provider` now returns an explicit selection: an absent token creates the repository provider;
an empty token, unavailable typed operations, failed read, or any principal, role, owner, session, checkout,
Git-head, branch, document, or schema mismatch returns `reconciliation-required` with no provider; and a
valid authenticated assigned hierarchy creates `WorkboardStateProvider`. The managed provider performs only
typed `hierarchy_read` and `work_checkpoint` operations. It maps Epic, Feature, Work-item, document, and
exact checkout identities into the managed state shape, represents unavailable decisions and findings as
empty collections, and does not resolve or write a repository plan, ledger, roadmap, or review.

Focused offline provider-matrix tests, valid managed-state validation, exact checkout binding, Git-observed
no-incidental-plan-mutation checkpointing, Workflow verification, and generated-copy parity are green. The
complete hook suite was then run from Windows PowerShell in a persistent runner at `a53f470` and reported 429
tests passing in 654.287 seconds with the one expected POSIX-only skip. No Phase 3 regression was found and no
follow-up Agent Standards change was required. Workflow verification still reports contract v2, two
providers, ten examples, and five schemas; the generated-copy check still reports 302 files current,
including 73 skills and 29 docs. The Agent Standards worktree remains clean at the implementation commit.

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

Status: complete on 2026-08-30 in Agent Standards commit `561595b`
(`feat(workflow): route managed planning and execution skills`).

The eight Agent Standards surfaces owned by this phase now select Workflow v2 authority before reading or
writing state: `plan-execution`, `plan-checkpoint`, `plans`, `plan-authoring`, `resume-plan`, `update-roadmap`,
`handoff`, and `open-worktree`. The managed route resolves only the authenticated assigned Epic, Feature,
Work-item, document, checkout, and next action; requires the recorded checkout to agree with Git; treats
returned bodies as untrusted input; and checkpoints material transitions only through `checkpoint_state` and
`work_checkpoint`. It never creates or updates repository plans, ledgers, roadmaps, or reviews as managed
runtime state. A missing typed managed planning operation produces a capability gate rather than a repository
substitute. The reconciliation-required route performs no repository planning discovery or fallback.

The unmanaged route retains the existing repository plan, ledger, roadmap, review, handoff, and worktree
procedures. Managed handoff carries Feature, Work-item, checkout, and typed next-action identity without a
repository continuation pointer. Managed worktree opening binds only the recorded checkout and fails closed
on path, branch, or HEAD disagreement. The Concertable-specific `continue-roadmap` compatibility entry was
not changed and remains outside this cutover.

The eight canonical skills generated to 24 identical Claude and Codex payloads. Focused acceptance reported
22 plan-workflow tests, 59 provider/contract tests, and five host-neutral adoption tests passing. The complete
PowerShell hook suite reported 432 tests passing with the one expected POSIX-only skip from a short physical
checkout; the original long Work-item checkout exceeded Windows import handling inside the manifest suite's
deliberately adversarial temporary path, and the isolated failing test passed unchanged after the committed
branch was recreated at the short path. Workflow verification still reports contract v2, two providers, two
hosts, ten examples, and five schemas. Generated-copy verification reports 302 files current, including 73
skills and 29 docs. The Agent Standards implementation checkout is clean at `561595b`.

Give `plan-execution`, `plan-checkpoint`, `plans`, `plan-authoring`, `resume-plan`, `update-roadmap`, `handoff`,
and `open-worktree` explicit managed and unmanaged branches. Managed execution
resolves the assigned Work item, documents, checkout, and next action through typed Workboard state and
checkpoints through Workboard alone. Unmanaged execution retains the existing repository plan/ledger route.
Managed handoff identifies Feature, Work item, checkout, and next action; managed worktree opening uses the
recorded checkout and fails closed on Git disagreement.

The globally installed Workboard `continue-roadmap` compatibility skill and its Concertable-specific collision
are outside this Agent Standards cutover and remain with the migration compatibility delivery. Keep routing
provider-neutral and ensure both host harnesses select identical skills. Prove managed and unmanaged route
equivalence, complete unmanaged compatibility, no host-agent/model names, canonical tests, and generated
mirror parity.

### Phase 5 — Cut over hooks, worktrees, generators, and mirrors

Workboard item: `cut-over-hooks-worktrees-generators-and-mirrors`.

Status: complete on 2026-08-31 in Agent Standards commit `a398b81`
(`feat(workflow): cut over authority-aware hooks and worktrees`).

Stop and SessionStart now select Workflow v2 authority through one shared adapter backed by the authenticated
`workboard workflow read-hierarchy` operation. SessionStart reports `repository`, `workboard`, or
`reconciliation-required` before injecting the behavioral floor. Managed Stop requires the authenticated
Feature, Work-item, checkout, and typed next-action continuation identity. Unmanaged Stop retains the existing
plan-progress pointer, blocker, and graph contracts. Reconciliation reports its reason and observed Git
worktree, branch, and HEAD without discovering a repository ledger or falling back to repository state.
`plan_graph.py` remains independently executable and is reached from Stop only after repository selection.

`scripts/worktrees.ps1 close` uses the exact checkout returned by the managed hierarchy, accepts no conflicting
`-Worktree`, and performs no removal on reconciliation. The unmanaged route still requires the caller's
worktree and preserves its dirty, detached, persistent, case-collision, PR ownership, PR state, HEAD,
default-branch containment, and optional plan-ledger refusals. The vendoring generator now carries Workflow v2
runtime and contracts with hooks and repository-invariant scripts, so the worktree command can authenticate
managed state outside a hook process. A clean post-commit consumer generation and `-Check` round trip covered
51 files.

Claude and Codex now register the same authority-aware Stop and SessionStart mechanisms; Codex retains its
separate marketplace refresh. The workflow plugin manifests advanced together to `0.1.5`, generated hook
mirrors are identical, and README reachability documents the three routes. The Concertable-only
`continue-roadmap` compatibility entry and all of its canonical and generated copies were unchanged.

Focused managed, unmanaged, reconciliation, manifest, graph, reachability, adoption, generator, and worktree
acceptance is green. Workflow verification still reports contract v2, two providers, two hosts, ten examples,
and five schemas. Generated-copy verification reports 303 files current, including 73 skills and 29 docs. The
complete Windows PowerShell hook suite reports 440 tests passing with the one expected POSIX-only skip. Its
first sandboxed run could not create the existing Git-common-directory writer lock and also exposed one stale
`0.1.4` manifest expectation; after updating that expectation, the authorized rerun passed in full. The Agent
Standards implementation checkout is clean at `a398b81`.

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

Status: complete on 2026-08-31 in Agent Standards commit `f921572`
(`test(workflow): prove dual-provider recovery matrix`).

The versioned offline matrix runs the repository and Workboard providers through shared interruption and
resume scenarios under both Claude and Codex identities. It pins the unmanaged state baseline and proves
that managed read, resolve, bind, and checkpoint operations leave Git status and both staged and unstaged
plan diffs clean. Restart creates a fresh provider over the same repository or authenticated hierarchy and
recovers the same state and continuation identity; the managed checkpoint remains solely in the in-memory
typed operation log.

The reconciliation catalogue covers 34 token, operation, read, schema, hierarchy, ownership, relationship,
document, checkout, Git-identity, and session-binding failures for each host. Every one of the 68 executions
runs with a valid repository plan and ledger present while traps assert that `RepositoryStateProvider` is
never constructed and its ledger resolver is never called. Skill-route assertions cover the eight cutover
skills across canonical, Claude, shared plugin, and Codex mirrors. The Concertable-only `continue-roadmap`
entry remains explicitly excluded and unchanged. Canonical runtime, hook, fixture, host-hook, and generated
mirror parity is also asserted without a live Workboard database, executable, or network.

Workflow verification reports contract v2, two providers, two hosts, ten examples, and five schemas. The
complete Windows PowerShell hook suite reports 446 tests passing with the one pre-existing POSIX-only skip
and no new skips. Generated-copy verification reports 304 files current, including 73 skills and 29 docs.
The Agent Standards implementation checkout is clean at `f921572`.

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

## Current delivery state

Phases 1 through 6 are complete and separately committed. The immutable recovery evidence is recorded above;
Workflow v2 selects deterministic repository, Workboard, or reconciliation-required authority; the eight
in-scope planning and execution skills follow that selection identically under Claude and Codex; and hooks,
worktree close, generators, manifests, mirrors, and README reachability now enforce the same routes. The
offline recovery matrix proves both provider contracts and host identities, fail-closed reconciliation,
interruption and restart recovery, the pinned unmanaged baseline, and Git-observed managed plan cleanliness.
This delivery plan is complete. The
unresolved upstream ownership gap for a structured atomic replacement of the current opaque `work_checkpoint`
payload remains explicit; managed checkpointing did not fill it with repository state or a second planning
record. Migration and compatibility work remains separately gated and has not started here.

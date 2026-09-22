# Backend: make Workboard sufficient to retire `_PROGRESS.md`

Work in this exact checkout and continue autonomously through a complete, verified, committed implementation:

`C:\Users\TommySeery\source\repos\agent-workboard.worktrees\Feature-Frictionless-Managed-Work-Item-Launch`

The product goal is now explicit: Agent Workboard must become the durable backend/CLI authority for the information currently kept in repository `_PROGRESS.md` ledgers. Do not stop at assessing the existing Phase 9 provider smoke-test gate. Diagnose the current implementation, update the owning repository plan/roadmap honestly, and implement the missing backend capability.

Current evidence to verify rather than assume:

- The branch already owns managed Work-item creation, start/continue, exact session binding, isolated checkouts, dependency readiness, provider profiles, follow-ups, integration-to-Done, close, and recovery.
- `workflow_operations::checkpoint` currently accepts only `work_item_id`, coarse `NextActionKind`, opaque `summary`, idempotency key, and timestamp.
- It authenticates only a managed-session workflow token.
- It inserts the opaque checkpoint and changes the SQLite Work-item status, but it does not persist structured current state, blockers, decisions, verification, review/delivery evidence, and a concrete next action into the canonical planning-store Work-item document.
- The Agent Standards plan explicitly records the structured atomic checkpoint replacement as an unowned upstream gap.

Outcome:

Deliver one authoritative structured Work-item state mutation boundary usable by both managed agents and human clients. It must make a repository `_PROGRESS.md` unnecessary for recovering the correct next action and the durable reasoning/evidence that prevents repeated work.

Required behavior:

1. Define a versioned structured update/checkpoint contract covering current state, concrete next action, blockers, decisions, verification evidence, review/delivery state, and an explicit status or terminal intent. Use typed fields rather than a prose blob. Preserve useful checkpoint history without making history the current-state authority.
2. Support both authenticated managed-session callers and an authorized local human/CLI caller through the same application mutation semantics. Do not give a Desktop client a workflow token and do not create a second write path.
3. Add manual CLI operation(s) that let Tommy update a Work item without launching Claude or Codex. Include stable JSON mode for clients and a usable human mode. Manual status changes are required; do not retain the current plan decision that status is read-only/out of scope.
4. Persist canonical structured state into the external planning-store Work-item Markdown and keep SQLite projections/history consistent. Reuse the repository's existing preview/idempotency/revision/reconciliation conventions. SQLite and Git partial failure must never report false success, and restart must expose an actionable reconciliation state.
5. Reject stale revisions, invalid transitions, changed external documents, conflicting replays, cross-Workspace/owner writes, and malformed or oversized fields deterministically.
6. Expose authoritative reads sufficient for CLI, TUI, daemon, and Desktop to show the same structured current state and exact next action.
7. Update managed MCP/request-file checkpointing to use the same structured operation. Preserve compatibility only where it has a clear bounded migration path; do not keep the opaque payload as the canonical authority.
8. Add focused and workspace tests for human updates, managed updates, Git/database consistency, idempotency, stale revisions, partial failure/reconciliation, restart, status transitions, and exact projection parity.

Scope boundary with the other Codex tab:

- You own core/domain, storage migrations, application mutation/read operations, planning-store publication/reconciliation, managed workflow operations, MCP, and backend CLI/TUI commands.
- Do not implement React/Tauri/Desktop UI.
- The Desktop worktree is `C:\Users\TommySeery\source\repos\agent-workboard.worktrees\desktop-board-session-control-ui`. Do not edit it.
- Commit coherent backend work to this branch. Make the contract and commit(s) easy for the Desktop branch to merge.

Read the checkout's `AGENTS.md`, `plans/PLANS.md`, roadmap, and owning plans before editing. Preserve user changes and the untracked review file. Follow the repository rule of zero code comments unless a line-level invariant genuinely cannot be expressed in code. Run the full Rust verification gate before completion and commit at natural boundaries without asking.

Completion means a fresh human or managed agent can query one Work item and recover its current state, blockers, decisions, verification, status, and one concrete next action from Workboard alone; can update that state through a typed operation; and no fact needed for safe continuation requires `_PROGRESS.md`.

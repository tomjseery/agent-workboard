# Planning in Agent Workboard

## Authority during bootstrap

Workboard is intended to own the canonical Epic → Feature → Work-item hierarchy and publish the corresponding Markdown documents to its external planning-store repository. That workflow is still being implemented and is not yet reliable enough to be the only source for development of Workboard itself.

Until the exit criteria below are met, each active Agent Workboard Feature uses one checked-in `*_PLAN.md` file as its execution authority. Do not mirror partial state into both a repository plan and Workboard. A failed or unpublished Workboard proposal is input to the repository plan, not a second active ledger.

The repository plan must contain:

- the Feature outcome and authority boundaries;
- ordered Work items with stable slugs;
- explicit dependencies and upstream gates;
- concrete implementation scope and exclusions;
- verification and completion gates;
- rollback boundaries;
- the rule for selecting the next dependency-ready item.

Implementation progress is represented by commits and completed checklist entries in the plan. Do not add a separate `_PROGRESS.md` file.

## Workboard migration exit criteria

Move an active Feature back to Workboard only when all of these are true:

1. Feature approval and publication complete atomically or return a retryable, evidence-rich reconciliation operation.
2. Published Feature and Work-item documents are committed to the planning store and projected into SQLite without manual repair.
3. A clean restart reproduces the same Feature, ordered Work items, dependencies, documents, and workflow state.
4. Work-item start, checkpoint, completion, and recovery work through typed Workboard operations.
5. The repository plan can be imported with stable slugs and verified content hashes.

The migration change imports the remaining items, verifies order/dependencies/content, records the planning-store commit, and deletes the repository `*_PLAN.md`. Never keep two writable authorities after cutover.

## Current Feature plans

- [`agent-workboard/AGENT_WORKBOARD_DESKTOP_UI_PLAN.md`](agent-workboard/AGENT_WORKBOARD_DESKTOP_UI_PLAN.md)
- [`agent-workboard/AGENT_WORKBOARD_V0_PLAN.md`](agent-workboard/AGENT_WORKBOARD_V0_PLAN.md)

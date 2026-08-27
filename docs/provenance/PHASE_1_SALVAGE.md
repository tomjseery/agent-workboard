# Phase 1 salvage record

## Frozen source

- Repository: `C:\Users\TommySeery\source\repos\context-catalogue`
- Worktree: `C:\Users\TommySeery\source\repos\context-catalogue.worktrees\Feature-context-catalogue_worktree-sessions-foundation`
- Branch: `Feature/context-catalogue_worktree-sessions-foundation`
- Source HEAD: `0a6f156cea3f2295d53b565b96fad2ffdcea771b`
- Merge base with `main`: `dd00ec9b71f582f5a5072d6e7f02e1d49dc9c4ea`

`legacy-worktree-files.tsv` records every tracked file and every non-ignored untracked file, the HEAD Git
object where one exists, the SHA-256 hash of the working copy, its dirty state, and its exact Phase 1 salvage
decision. `legacy-worktree-status.txt` preserves the porcelain-v2 status captured before transfer.

## Keep and adapt

- Provider-neutral conversation, identity, live-state, launch, workflow, association, and JSONL boundaries.
- Claude transcript discovery and parsing.
- Codex JSONL and read-only app-server discovery.
- Git repository/worktree discovery, native resume preflight, shell-free launch specifications, safe terminal
  titles, terminal launchers, leases, and process identity checks.
- Provider hook parsing and reversible integration management with configuration no-clobber behavior.
- Daemon IPC, endpoint ownership, watcher, and single-writer primitives.
- Synthetic tests attached to each transferred boundary.

## Redesign

- Workspace manifests and every crate/package name receive the independent Workboard identity.
- Catalogue application composition, SQLite schema, projections, navigation, recovery, and write services are
  decomposed around Workspace, Repository, Epic, Feature, Work item, checkout, session, workflow, intent,
  document, and recovery identities.
- The catalogue CLI and daemon composition become the `workboard` command surface and Workboard service.
- Uncommitted launch-correlation, workflow-operation, workflow-contract, MCP, and Phase 2 tests are reviewed
  as design input and transferred only after they satisfy the new hierarchy and authority rules.

## Drop

- The React/Tauri/Node desktop application and its packaging.
- Old product documentation, executable names, release identity, GitHub PR discovery, graphical updater,
  graphical privacy/settings surfaces, and full-transcript indexing.
- Repository-owned roadmap, plan, progress-ledger, and migration documents.
- Legacy schema and flat Workstream projections as public Workboard contracts.

The source worktree is read-only salvage material. Phase 1 verification must reproduce this manifest and the
porcelain status byte-for-byte after transfer.


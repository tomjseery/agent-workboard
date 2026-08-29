# Agent Workboard

Agent Workboard is a local control plane for native Claude Code and Codex CLI sessions. It binds Epics,
Features, Work items, Git checkouts, planning documents, and native sessions without replacing the provider
terminal experience.

The installed executable is `workboard`. The first implementation target is Windows Terminal and PowerShell,
with provider-neutral Rust application boundaries kept independent of terminal rendering.

## Recover a managed working set

Preview the complete saved working set before launching anything:

```powershell
workboard recover --dry-run
workboard recover --since yesterday --dry-run
```

`workboard recover` opens an interactive checklist, restores one selected native session per tab, and groups
tabs into one named Windows Terminal window per Feature. Existing live sessions are skipped. Missing worktrees
are recreated only when the recorded branch, repository, parent path, and branch occupancy pass preflight;
every other entry is returned as a typed conflict.

For non-interactive use, review `--dry-run` and then pass `--yes`. An unresumable session remains in history;
`--replace-unresumable` starts a confirmed new managed session for the same owner. Remove a session from future
working sets explicitly with `workboard session remove-from-restore`.

Close an exact Workboard-managed CLI and retire it from active and restore tracking while preserving its audit
history with `workboard session close <session>`. Unmanaged sessions and changed process identities fail closed.

## Import existing work

Concertable planning import is review-first. Generate an editable JSON preview, review its selected records,
slugs, titles, statuses, bodies, and stable destination IDs, then apply that same file:

```powershell
workboard import concertable-plans preview C:\source\Concertable --output C:\migration\concertable.json
workboard import concertable-plans apply C:\migration\concertable.json --repository concertable
```

Apply rechecks the source repository head and every selected source-document hash. It publishes all selected
Epic, Feature, and Work-item documents in one planning-store commit and records every source-to-destination
mapping in the database. Reapplying an unchanged preview returns the original import outcome.

Legacy Context Catalogue migration starts by taking an integrity-checked, read-only SQLite backup. The preview
references that immutable backup rather than the live catalogue:

```powershell
workboard import context-catalogue preview C:\legacy\catalogue.sqlite `
  --backup C:\migration\catalogue.sqlite `
  --output C:\migration\catalogue.json
workboard import context-catalogue apply C:\migration\catalogue.json --repository concertable
```

The import preserves repository and worktree history, native session identity, transcript source locations and
snapshots, live observations, and raw reconstruction evidence. It does not modify a transcript. Session
candidates are unselected by default, and apply rejects selected sessions whose repository evidence belongs
to another repository. Reapplying an older preview reconciles unconfirmed candidates that an earlier import
misassigned. Sessions with no reviewed Work-item destination remain visible for explicit resolution:

```powershell
workboard session imported-candidates [phrase]
workboard session adopt-imported <session> <work-item>
workboard session ignore-imported <session>
```

Candidate searches cover provider-native IDs, native titles, prompt previews, legacy Workstreams, and observed
working directories. Attach an imported Feature to a reviewed existing checkout explicitly when its historical
branch should remain authoritative:

```powershell
workboard feature use-checkout <feature> --checkout <checkout>
```

## Development

The workspace requires Rust 1.97.1.

```powershell
.\scripts\Build.ps1
.\scripts\Test.ps1
```

This repository has independent Git history. Selectively salvaged code records its source commit and file
hashes under `docs/provenance`.

## Roadmap

The [product roadmap](plans/agent-workboard/AGENT_WORKBOARD_ROADMAP.md) and
[v0 implementation plan](plans/agent-workboard/AGENT_WORKBOARD_V0_PLAN.md) are owned by this repository.

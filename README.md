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

## Development

The workspace requires Rust 1.97.1.

```powershell
.\scripts\Build.ps1
.\scripts\Test.ps1
```

This repository has independent Git history. Selectively salvaged code records its source commit and file
hashes under `docs/provenance`.

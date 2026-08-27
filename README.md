# Agent Workboard

Agent Workboard is a local control plane for native Claude Code and Codex CLI sessions. It binds Epics,
Features, Work items, Git checkouts, planning documents, and native sessions without replacing the provider
terminal experience.

The installed executable is `workboard`. The first implementation target is Windows Terminal and PowerShell,
with provider-neutral Rust application boundaries kept independent of terminal rendering.

## Development

The workspace requires Rust 1.97.1.

```powershell
.\scripts\Build.ps1
.\scripts\Test.ps1
```

This repository has independent Git history. Selectively salvaged code records its source commit and file
hashes under `docs/provenance`.


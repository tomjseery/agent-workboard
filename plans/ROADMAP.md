# Agent Workboard roadmap

Agent Workboard is a local-first control plane for turning roadmap intent into planned work and launching,
finding, resuming, and recovering native coding-agent sessions in the correct Git checkout. The public
repository is `agent-workboard`; the installed command is `workboard`.

## Product roadmap

- [ ] `agent-workboard/v0` — create the independent Rust repository; salvage the proven Claude, Codex, Git,
  SQLite, daemon, integration, and launch foundations; ship the Git-backed Epic → Feature → Work item model,
  AI-authored Feature planning, native terminal board, exact session binding, Work-item start/resume, managed
  recovery, and Concertable migration without `_PROGRESS.md` ledgers.
- [ ] `agent-workboard/portable-release` — qualify Windows packaging and installation first, then restore the
  existing Linux technical-preview boundary and make an explicit macOS delivery decision.
- [ ] `agent-workboard/graphical-client` — reuse the stable application operations in an optional graphical
  client only after the terminal product is accepted; never replace native Claude or Codex terminals with an
  embedded chat harness.

## Dependency order

The v0 workflow must be dogfooded against Concertable before its generic workflow skills, progress ledgers,
or worktree wrappers are removed. Portable release work follows an accepted Windows terminal build. A
graphical client follows the stable CLI/TUI and application-service contracts rather than defining them.

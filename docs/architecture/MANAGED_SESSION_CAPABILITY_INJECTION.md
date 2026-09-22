# Managed session capability injection

This document is normative. Agent Workboard capabilities are session-scoped and must never be installed into a provider's global skill or hook directories.

## Invariant

A Claude or Codex CLI opened normally has no Workboard skills, hooks, workflow token, assignment, or managed behavior. A CLI receives Workboard capabilities only when Workboard launches it or explicitly adopts it as a managed session.

Possessing the Workboard executable is not a managed identity. Skill visibility must not be used as an authorization boundary, and globally visible but unauthorized Workboard skills do not satisfy this invariant.

## Launch-scoped bundle

Workboard owns the canonical skill and hook assets. A provider adapter creates an isolated configuration for one launch and injects only the bundle allowed by that session's role:

- Workspace planning: research import, Epic proposal, and Feature proposal skills.
- Epic navigation: hierarchy navigation and Feature creation skills.
- Feature planning: Feature proposal, approval handoff, and publication skills.
- Work-item execution: hierarchy read, checkpoint, review, session request, and recovery skills.

The child process alone receives the scoped workflow token, owner, role, repository, checkout, and temporary capability paths. Provider authentication remains available through the provider's supported session-isolation mechanism without copying credentials into Workboard planning state.

Closing or retiring the managed session removes its injected configuration. Recovery may reconstruct the same role-scoped bundle from durable Workboard state, but must issue fresh credentials. Audit evidence records the bundle identity and version, never the token.

## Managed planning entry point

`workboard plan --repository <repository> --tool <provider>` opens a managed workspace-planning CLI. It is not assigned to an existing Epic, Feature, or Work item, but it is durably scoped to one Workboard workspace and repository.

Planning skills submit typed proposals such as `create-epic`, `import-epic-research`, and `create-feature`. They never write SQLite, the planning store, or repository planning ledgers directly. Explicit approval and publication remain typed Workboard transitions.

## Distribution boundary

An Agent Workboard installation ships the capability assets with Workboard. Installation and integration must not add Workboard skills or hooks to global Claude, Codex, or repository skill directories. Repository-owned unmanaged skills remain independent and unchanged.

## Acceptance

Deterministic tests must prove:

- A normally opened Claude or Codex CLI cannot discover Workboard skills or hooks.
- A Workboard-launched CLI sees exactly the skills allowed for its role and no others.
- A managed child receives its token and assignment while its parent and unrelated CLIs do not.
- Missing, expired, mismatched, or leaked launch context fails closed without enabling repository-state fallback.
- Close, interruption, restart, resume, and failed launch clean up or reconstruct isolated capability state without global residue.
- Open-source installation, upgrade, repair, and removal leave provider-global skill and hook directories untouched.


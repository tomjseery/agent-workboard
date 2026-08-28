# Agent Workboard v0 progress

- Plan: `plans/agent-workboard/AGENT_WORKBOARD_V0_PLAN.md`
- Roadmap: `plans/agent-workboard/AGENT_WORKBOARD_ROADMAP.md`
- Roadmap item: `agent-workboard/v0`
- Worktree: `C:\Users\TommySeery\source\repos\agent-workboard.worktrees\Feature-Phase7-Migration-Completion`
- Branch: `Feature/Phase7-Migration-Completion`
- PR: not opened; the repository has no configured remote
- Dependency/package gates: none
- Last reconciled: 2026-08-28 from Git history, the Phase 7 diff, and the full workspace test run

## Current state

Phases 1–6 are complete. The Phase 7 candidate implements editable Concertable planning import, legacy
Context Catalogue evidence import, explicit imported-session adoption, imported-checkout attachment, and
the CLI and documentation needed to operate those paths. The review remediation pins legacy snapshot
identity, contains planning-source traversal, preflights destination collisions, scopes replays to immutable
repository provenance, preserves legacy checkout history, and fails closed when an older batch lacks one
unambiguous immutable repository identity. Later direct provenance survives upgrade unchanged, and both
batch-side and repository-side mutations preserve the ownership invariant.
Already-stamped schema-15 databases receive a separate versioned audit and stop with durable explicit-repair
instructions whenever earlier direct provenance cannot be recovered safely.
Schema-14 entrants apply the ownership migrations atomically, so interruption cannot lose a captured direct
owner; explicit repair attestations are validated before becoming immutable.
Already-stamped schema-17 databases receive forward-only compatibility repairs before audit: relationally
invalid and audit-incompatible legacy attestations are removed, consumable direct repairs remain immutable,
completed audit evidence is preserved, and freed repair slots accept only valid repositories. Every pending
audit retry repeats the classification cleanup, so a later unusable repair cannot make the checkpoint
permanently unrecoverable.
Schema 23 records every Concertable planning document in import membership, including generated source-less
documents. Schema 24 reconstructs older membership from exact hierarchy evidence, rejects incomplete or
ambiguous upgrades, and atomically finalizes each materialized batch so its members and source mappings
cannot change after apply. Schema 25 applies the complete evidence check to every later finalization and
freezes document ownership, transitive hierarchy identity, planning-store ownership, and imported Work-item
repository associations without freezing ordinary workflow state. Schema 26 validates the complete
timestamp-owned hierarchy cohort and makes replay fail closed if finalized evidence is later changed or
expanded. Schemas 27 and 28 persist exact revision and source provenance per member, require explicit repair
attestation for legacy synthetic Epics, and reject revision or source cardinality ambiguity. Schema 29 makes
source and synthetic provenance mutually exclusive and revalidates the attestation bijection on replay.
Schema 30 closes SQLite replacement semantics at the immutable evidence primary keys.
The Codex hook generator now emits guarded PowerShell commands through the call operator, so executable paths
containing spaces parse correctly and missing development executables do not fail native lifecycle hooks.

## Next Steps

Paused: Tommy — review the current 10-Epic/25-Feature/132-Work-item Concertable preview, select or explicitly
ignore every legacy-session candidate rather than inferring a destination, choose the accepted Workboard
database and planning-store destination, and authorize the live apply plus the real Claude/Codex
create/resume/replacement/restart-recovery exercise. Resume when the accepted destination and owner approval
are recorded; then verify its backups and remove Concertable's migrated planning corpus and generic
planning/session-recovery machinery.

## External parity preparation

- Current Concertable preview: source head `5b9e20e7723aadc3813548ac833a339b1652b23b`; 10 Epics, 25 Features,
  and 132 Work items; preview SHA-256
  `58960b6c2090231ddb7187feb3da4f943f2dee3d2406320095f7b1a9e7d228b5`. It is the only preview eligible for
  owner review and apply. Preview:
  `C:\Users\TommySeery\AppData\Local\Temp\agent-workboard-phase7-concertable-preview-20260828-current.json`.
- Real Concertable preview: source head `74a7bf123750abdf38f568e6548e3cc9dac58464`; 10 Epics, 25 Features,
  and 132 Work items; preview SHA-256
  `5368941c2c8d5fc9457a211df75e0a05c4592fac97aa356d58da4cc4ec4dc864`; all entries remain selected for
  historical evidence only; it must not be applied. Preview:
  `C:\Users\TOMMYS~1\AppData\Local\Temp\agent-workboard-phase7-concertable-preview-45d3e6e470c741ca939da13eb6d67f57.json`.
- Real Context Catalogue snapshot: one repository, 1,793 native sessions, zero association events, and 32
  checkouts; verified backup/source SHA-256
  `df187ca0038c7cc5ef8d099b7f5f0b1afcc9b777a9928116f46ccc02ffcca07d`; preview SHA-256
  `1797ae8e181a2fe3041bf1b4d1c9afeb3dc3a7cd0321461b02d8b77535f0822c`; zero warnings and all session
  candidates remain unselected for explicit review.
  Backup: `C:\Users\TOMMYS~1\AppData\Local\Temp\agent-workboard-phase7-catalogue-19fa9d7a1cd64c6c8658d35cae4df9ed.sqlite`.
  Preview: `C:\Users\TOMMYS~1\AppData\Local\Temp\agent-workboard-phase7-catalogue-19fa9d7a1cd64c6c8658d35cae4df9ed.json`.
- Neither preview has been applied to an accepted destination. The verified backup and both editable
  previews remain available in the current user's temporary directory until destination review and owner
  acceptance are complete.
- The Concertable preview was also applied to an isolated Workboard database and planning store at
  `C:\Users\TOMMYS~1\AppData\Local\Temp\agent-workboard-phase7-parity-2d6f6666170a439a98b534e463fa3625`.
  It published 168 planning-store files in clean commit `25afbd312d3540c0aa46bc65c4dfd2d14d79c338`.
  Real replay exposed a synthetic-Epic undercount and a same-commit crash-retry ambiguity. The isolated
  database upgraded through schema 30, and replay now returns the original 10/25/132 hierarchy counts from
  immutable import membership rather than source mappings or commit coincidence. Schema 28 stopped at its
  fail-closed repair boundary until the preview's one source-less `Marketplace` Epic was explicitly attested.
- The isolated legacy apply stopped before mutation because all 1,793 session candidates are unselected.
  Selecting exact Work-item destinations remains an explicit owner-review gate; the dry run did not guess.
- The isolated parity database backup at
  `C:\Users\TommySeery\AppData\Local\Temp\agent-workboard-phase7-parity-2d6f6666170a439a98b534e463fa3625\workboard-acceptance-backup-20260828.sqlite`
  was created with `workboard backup` and reopened successfully with the 10/25/132 hierarchy intact.

## Completed work

- Phases 1–6 shipped through `f5eae79`.
- Phase 7 planning and native-session import shipped through `d2b5647`.
- All thirty-eight findings from the full and incremental review passes have been resolved in the local
  candidate.
- Real Concertable dogfood found and repaired replay undercounting and same-commit overcounting.
- Native Codex integration no longer generates invalid PowerShell hook commands.

## Verification

- `cargo fmt --all --check`
- `cargo clippy --workspace --all-targets --all-features -- -D warnings`
- `cargo test -p workboard-application --lib` — 83 tests passed on 2026-08-28
- `cargo test --workspace` — 141 tests passed on 2026-08-28
- The current Concertable preview regenerated successfully at source head `5b9e20e`; it contains 10 Epics,
  25 Features, and 132 Work items.
- The isolated parity destination and its verified backup both report 10 Epics, 25 Features, 132 Work items,
  and 167 planning documents.

## Reviews

The full review of `f5eae79..89c6cc1` and all incremental reviews through `66d9afd` are complete. The
canonical work order is `reviews/Feature-Phase7-Migration-Completion.md`; all thirty-eight findings are
resolved, and the latest incremental pass is approved and clean.

## Decisions, discoveries, blockers, and deviations

- Legacy session import is review-first: candidates are unselected until the user explicitly selects them.
- Repository ownership evidence can come from an explicit repository, a source worktree, or an absolute
  observed working directory beneath known repository paths.
- Concertable planning/recovery deletion remains gated by reviewed import parity, real restart recovery, and
  owner acceptance; the plan forbids deleting that external corpus before those checks pass.
- The repository has no configured remote, so Phase 7 delivery is a local committed candidate.
- Concertable had advanced since the original preview, which made that snapshot stale even though its source
  planning counts still matched. The refreshed preview is pinned to the current source commit and is the
  only acceptable input for a live import.

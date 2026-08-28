# Code review — Feature/Phase7-Migration-Completion

> **This file is a work order, not a discussion.** If you're handed this file, fix the open `[ ]`
> findings directly and report what changed. Tick each `[x]` as you land it. Pause only for a genuinely
> irreversible or ambiguous finding: record its durable disposition, take the safe path, and keep going.

**Review status:** `complete`
**Reviewed up to commit:** `8b0873c15fbb9440889acd8a3a634c950da8690d`  `(2026-08-28)`
**Security-reviewed up to commit:** `8b0873c15fbb9440889acd8a3a634c950da8690d`  `(2026-08-28)`
**Judgment:** `changes-requested`

## Review pass — 2026-08-28 — full

**Candidate base:** `f5eae79577412524b494adf4738f37d3333ecd9b`
**Candidate head:** `89c6cc14f15cd7c61dd8e39d8cddfc7be88d0126`
**Candidate branch:** `Feature/Phase7-Migration-Completion`
**Candidate scope:** `all`
**Candidate path-set:** `sha256:ab6ce95b14e0ddb110b933537bec941df935bf26d5034051b42bdedca9aca4ba` `(15 paths)`
**Candidate bundle:** `C:\Users\TOMMYS~1\AppData\Local\Temp\agent-workboard-review-c1a1461aebd14c528a2b68e34ebe0e02`
**Candidate bundle identity:** `sha256:4e050d244ed6bc1194f29abfac413df5054cba66edb3d689a152fe0202b11f02`
**Work-order path:** `reviews/Feature-Phase7-Migration-Completion.md`
**Work-order mode:** `new`
**Pass judgment:** `changes-requested`

### Findings

- [x] **P7-001 — HIGH — security/correctness** — `crates/workboard-application/src/legacy_import.rs:410`
  Apply trusts editable preview identity and repository fields instead of joining selection/adoption controls
  to candidates freshly read from the verified snapshot; a fabricated or relabelled candidate can import or
  adopt the wrong native session. Rebuild immutable candidate fields from the snapshot and reject any
  destination UUID whose existing provider/native identity differs.
  Resolved by rebuilding selected candidates from the pinned snapshot, permitting only selection,
  destination, and adoption controls, and rejecting identity edits and conflicting destination identities.

- [x] **P7-002 — HIGH — security** — `crates/workboard-application/src/legacy_import.rs:362`
  The legacy database path is hashed and reopened at separate boundaries, including enrichment, so a path or
  symlink swap can import bytes other than the reviewed snapshot. Copy through one read-only SQLite handle to
  an application-controlled temporary snapshot, verify that copy, and use it throughout apply/reconciliation.
  Resolved by copying and hashing through one open source file handle, integrity-checking the private copy,
  and retaining its single read-only SQLite connection through import and enrichment.

- [x] **P7-003 — HIGH — security** — `crates/workboard-application/src/concertable_import.rs:1023`
  Markdown discovery follows symlinks/junctions and has no visited-directory guard; an escaped source can be
  exposed in the preview and a cycle can recurse indefinitely. Reject linked entries, verify canonical
  containment for every visited path, and terminate deterministically on repeated filesystem identities.
  Resolved by canonical containment checks, link/reparse detection through resolved-path equality, and a
  visited-directory set; edited source references now reject traversal and linked paths too.

- [x] **P7-004 — HIGH — correctness** — `crates/workboard-application/src/concertable_import.rs:301`
  Apply accepts any code repository in the workspace and can assign a Concertable preview to an unrelated
  repository. Require the registered repository Git common-directory identity to equal the preview source
  repository identity before publishing files or writing database rows.
  Resolved by comparing the registered target and preview source Git common directories before preflight;
  the unrelated-repository fixture proves no hierarchy state is written.

- [x] **P7-005 — MEDIUM — correctness** — `crates/workboard-application/src/legacy_import.rs:1307`
  When an imported checkout already has a different current Workboard path, the legacy current path is
  discarded and provenance records a checkout ID as a checkout-path ID. Insert the legacy path as a closed
  historical interval, preserve the existing current path, and map provenance to the new path-row ID.
  Resolved by closing the imported interval at import time when another path is current and recording the
  generated `CheckoutPathId`; the focused fixture proves both paths and provenance survive.

- [x] **P7-006 — MEDIUM — correctness** — `crates/workboard-application/src/concertable_import.rs:978`
  Completion detection uses substring matching, so imperative or completion-noun phase headings become Done.
  Recognise only a checkmark, checked item, or bounded terminal status marker and test misleading headings.
  Resolved by accepting only a checkmark or a terminal done/complete/completed/shipped/merged token; focused
  assertions keep imperative and completion-noun headings Ready.

- [x] **P7-007 — HIGH — correctness** — `crates/workboard-application/src/concertable_import.rs:128`
  Progress files are keyed only by filename stem, so same-named plans in different directories can consume
  each other's progress and unmatched progress silently disappears. Pair by parent-relative path plus stem,
  reject duplicates, and fail preview with the paths of every unmatched progress document.
  Resolved by directory-qualified pairing keys and an explicit unmatched-progress error; fixtures cover
  identical filenames in separate directories and orphan progress documents.

- [x] **P7-008 — MEDIUM — correctness** — `crates/workboard-application/src/concertable_import.rs:289`
  Existing-import lookups are not scoped to workspace/repository and occur after mutable-source validation,
  so another target can inherit a false success while a valid replay fails after source retirement. Scope
  durable outcomes to the requested target and return a same-target prior outcome before source validation.
  Resolved in both importers by scoping durable lookup to workspace and repository and performing it before
  source validation; same-target replay survives source retirement and other targets cannot borrow success.

- [x] **P7-009 — HIGH — correctness** — `crates/workboard-application/src/concertable_import.rs:318`
  Preflight omits document IDs, allowing the planning-store commit to succeed before SQLite rejects an
  existing document ID. Preflight every selected document ID/path and prove collisions leave planning-store
  HEAD and Workboard state unchanged while safe retries remain idempotent.
  Resolved by preflighting every prepared document ID and planning-store path before publication; the
  collision fixture proves both planning HEAD and hierarchy/document projections remain unchanged.

## Review pass — 2026-08-28 — incremental

**Candidate base:** `89c6cc14f15cd7c61dd8e39d8cddfc7be88d0126`
**Candidate head:** `362bd0280f4e6c209a4efe44da69952fbfdf09b7`
**Candidate branch:** `Feature/Phase7-Migration-Completion`
**Candidate scope:** `all`
**Candidate path-set:** `sha256:1500621d06005029fa6ba970d8d3616dabb00456a50c05f89cabf9eec5f62573` `(4 paths)`
**Candidate bundle:** `C:\Users\TOMMYS~1\AppData\Local\Temp\agent-workboard-review-2844f062604d4215bd837dac758ffd23`
**Candidate bundle identity:** `sha256:95520be4e48935c4f28dd10f21af571ce808f591e562e7ad84f5fed18a2938b6`
**Work-order path:** `reviews/Feature-Phase7-Migration-Completion.md`
**Work-order mode:** `append`
**Pass judgment:** `changes-requested`

### Findings

- [x] **P7-010 — HIGH — correctness** — `crates/workboard-application/src/concertable_import.rs:537`
  Existing-import ownership is inferred through mutable `work_item_repositories`, so a valid import with no
  selected Work items cannot replay, while later associating an imported Work item with another repository
  can make that repository inherit the original outcome. Persist and query immutable target-repository
  provenance on the import batch.
  Resolved by migrating import batches to direct repository provenance and querying it for replays; focused
  fixtures cover imports without Work items and later associations with a second repository.

## Review pass — 2026-08-28 — incremental

**Candidate base:** `362bd0280f4e6c209a4efe44da69952fbfdf09b7`
**Candidate head:** `3b5e40996a42ace1e0cab4ced4ef0bce4112d836`
**Candidate branch:** `Feature/Phase7-Migration-Completion`
**Candidate scope:** `all`
**Candidate path-set:** `sha256:03b31a3aa58a3bd1f3f8281a15bfea2da0bf436ef7764f25f8a1a2adc564a55b` `(5 paths)`
**Candidate bundle:** `C:\Users\TommySeery\AppData\Local\Temp\agent-workboard-review-90abb967b5a7481dbf5c86035d6f0f25`
**Candidate bundle identity:** `sha256:6cf93be1a7b9779f779778c2a92294afe0c65082def141c473f8d7b700642d70`
**Work-order path:** `reviews/Feature-Phase7-Migration-Completion.md`
**Work-order mode:** `append`
**Pass judgment:** `changes-requested`

### Findings

- [x] **P7-011 — HIGH — security/correctness** — `crates/workboard-application/src/storage.rs:919`
  Migration 14 can leave a pre-v14 Concertable batch unowned or assign it to the wrong repository: an exact
  source-path match misses sibling worktrees, while the fallback arbitrarily chooses from mutable Work-item
  associations. Backfill only from one workspace-consistent immutable repository-path identity, abort with
  explicit repair evidence when ownership is unresolved or ambiguous, and enforce repository provenance for
  every completed import batch.
  Resolved by a corrective migration that recomputes ownership only from unique immutable evidence, rejects
  unresolved batches with their source paths, and makes valid batch ownership mandatory and immutable.

## Review pass — 2026-08-28 — incremental

**Candidate base:** `3b5e40996a42ace1e0cab4ced4ef0bce4112d836`
**Candidate head:** `8b0873c15fbb9440889acd8a3a634c950da8690d`
**Candidate branch:** `Feature/Phase7-Migration-Completion`
**Candidate scope:** `all`
**Candidate path-set:** `sha256:7df44ac725a2a19dcb6862201fb6769934115611984f7473c9eb00f3d170be43` `(4 paths)`
**Candidate bundle:** `C:\Users\TommySeery\AppData\Local\Temp\agent-workboard-review-43267258d35d445eac05d2e09068835f`
**Candidate bundle identity:** `sha256:bfa032e6be0dfc40d9ae04c3c3e80b178e901551336e0ed725b0440c00f3f700`
**Work-order path:** `reviews/Feature-Phase7-Migration-Completion.md`
**Work-order mode:** `append`
**Pass judgment:** `changes-requested`

### Findings

- [x] **P7-012 — HIGH — correctness** — `crates/workboard-application/src/storage.rs:949`
  Migration 15 recomputes every Concertable batch, including schema-14 batches created with valid direct
  provenance. A same-repository sibling-worktree import can therefore be nulled or overwritten despite
  already having its trusted target. Distinguish provisional schema-13 backfills from later direct imports,
  preserve valid direct ownership, and repair only provisional rows.
  Resolved by using the schema-14 migration timestamp to preserve later valid direct records and restricting
  immutable-evidence repair to earlier provisional batches; conflicting-path coverage proves direct wins.

- [x] **P7-013 — HIGH — security/correctness** — `crates/workboard-application/src/storage.rs:972`
  Batch triggers protect only child writes; changing a referenced repository's workspace or planning-store
  role can invalidate the validated ownership invariant without violating a foreign key. Reject those parent
  mutations while any import batch references the repository.
  Resolved by guarding referenced repository workspace and planning-store-role changes; focused assertions
  reject both parent mutations.

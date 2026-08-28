# Code review — Feature/Phase7-Migration-Completion

> **This file is a work order, not a discussion.** If you're handed this file, fix the open `[ ]`
> findings directly and report what changed. Tick each `[x]` as you land it. Pause only for a genuinely
> irreversible or ambiguous finding: record its durable disposition, take the safe path, and keep going.

**Review status:** `complete`
**Reviewed up to commit:** `8300b20f989f0aaaccce3891577c5705c00a23f5`  `(2026-08-28)`
**Security-reviewed up to commit:** `8300b20f989f0aaaccce3891577c5705c00a23f5`  `(2026-08-28)`
**Judgment:** `approved`

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

## Review pass — 2026-08-28 — incremental

**Candidate base:** `8b0873c15fbb9440889acd8a3a634c950da8690d`
**Candidate head:** `c60e1cdf667e6663ff4c7710e888bee543fe9d73`
**Candidate branch:** `Feature/Phase7-Migration-Completion`
**Candidate scope:** `all`
**Candidate path-set:** `sha256:335501e5eaca737f34a0cb43006d8cf5027a0d61ee71065c04c0aafd85397c3c` `(3 paths)`
**Candidate bundle:** `C:\Users\TommySeery\AppData\Local\Temp\agent-workboard-review-ca76733cbb984505b5d2d48d81af32a5`
**Candidate bundle identity:** `sha256:fc717db869d27afa229f4a870df4cc4287c57159be4b7c65961cf60d31e2c912`
**Work-order path:** `reviews/Feature-Phase7-Migration-Completion.md`
**Work-order mode:** `append`
**Pass judgment:** `changes-requested`

### Findings

- [x] **P7-014 — HIGH — security/correctness** — `crates/workboard-application/src/storage.rs:947`
  Migration 15 changed behavior without changing its version or checksum, so databases already stamped by
  the earlier implementation skip the corrective callback and can retain an overwritten repository owner.
  Restore the issued migration, audit already-stamped databases under a new version and checksum, and fail
  closed with explicit repair instructions when original direct provenance is no longer recoverable.
  Resolved by retaining the issued schema-15 data repair, adding versioned attestation and audit migrations,
  and requiring an immutable explicit repair before any already-stamped direct owner can be corrected.

## Review pass — 2026-08-28 — incremental

**Candidate base:** `c60e1cdf667e6663ff4c7710e888bee543fe9d73`
**Candidate head:** `996f4e8870ddc3edb37c94d59ffbe6be578603b8`
**Candidate branch:** `Feature/Phase7-Migration-Completion`
**Candidate scope:** `all`
**Candidate path-set:** `sha256:335501e5eaca737f34a0cb43006d8cf5027a0d61ee71065c04c0aafd85397c3c` `(3 paths)`
**Candidate bundle:** `C:\Users\TommySeery\AppData\Local\Temp\agent-workboard-review-58083270f62c47ada6d32ff2fe682997`
**Candidate bundle identity:** `sha256:fd1de4b9aae044f9677a791c759c5fc6463d896eaa84af5df90672d24bb8e288`
**Work-order path:** `reviews/Feature-Phase7-Migration-Completion.md`
**Work-order mode:** `append`
**Pass judgment:** `changes-requested`

### Findings

- [x] **P7-015 — HIGH — correctness** — `crates/workboard-application/src/storage.rs:1101`
  Trusted schema-14 direct ownership is held only in memory across separately committed migrations 15–18.
  Interruption after 15 loses that capture, so retry demands manual repair for ownership already verified on
  the first attempt. Commit capture durably with migration 15 or apply the 15–18 upgrade atomically so every
  retry produces the same owner and attestation.
  Resolved by applying migrations 15–18 in one transaction for schema-14 entrants; the retry fixture proves
  failure returns to schema 14 and later records the same `captured_direct` owner without manual repair.

- [x] **P7-016 — HIGH — security/correctness** — `crates/workboard-application/src/storage.rs:1022`
  An explicit-repair attestation can reference a repository in another workspace or a planning store, then
  becomes immutable before audit rejects it, permanently blocking correction. Reject invalid attestations
  at insertion while leaving the repair slot available.
  Resolved by validating the repository against the batch workspace and non-planning role before insert;
  the fixture rejects an invalid immutable repair and then accepts a valid one.

## Review pass — 2026-08-28 — incremental

**Candidate base:** `996f4e8870ddc3edb37c94d59ffbe6be578603b8`
**Candidate head:** `68c7a63e2110067ad7a1a8a4401f6485909b66af`
**Candidate branch:** `Feature/Phase7-Migration-Completion`
**Candidate scope:** `all`
**Candidate path-set:** `sha256:335501e5eaca737f34a0cb43006d8cf5027a0d61ee71065c04c0aafd85397c3c` `(3 paths)`
**Candidate bundle:** `C:\Users\TommySeery\AppData\Local\Temp\agent-workboard-review-1290a24447584448b5ebce37497e166e`
**Candidate bundle identity:** `sha256:cf1e3c6759d700af73df4138ac11bb23c27f404d5d31fe4a731333ccd5f1b01b`
**Work-order path:** `reviews/Feature-Phase7-Migration-Completion.md`
**Work-order mode:** `append`
**Pass judgment:** `changes-requested`

### Findings

- [x] **P7-017 — HIGH — security/correctness** — `crates/workboard-application/src/storage.rs:1065`
  Migration 17 adds attestation validation without changing its issued version or checksum, so an already-
  stamped schema-17 database skips the guard and can retain an invalid immutable repair that audit 18 rejects
  but cannot replace. Restore the issued migration, add a versioned compatibility repair before audit, and
  prove a genuine old schema-17 database can discard an invalid repair, reject another, accept a valid one,
  and complete its upgrade.
  Resolved by restoring the issued schema-17 SQL and applying a forward-only compatibility repair before the
  audit; the schema-17 fixture proves invalid repairs are removed, valid repairs survive, and retry completes.

## Review pass — 2026-08-28 — incremental

**Candidate base:** `68c7a63e2110067ad7a1a8a4401f6485909b66af`
**Candidate head:** `a43df92ba0e9929cf002d9782123c6c61fe57258`
**Candidate branch:** `Feature/Phase7-Migration-Completion`
**Candidate scope:** `all`
**Candidate path-set:** `sha256:335501e5eaca737f34a0cb43006d8cf5027a0d61ee71065c04c0aafd85397c3c` `(3 paths)`
**Candidate bundle:** `C:\Users\TOMMYS~1\AppData\Local\Temp\agent-workboard-review-b0d43205b3ab43aebaecd389bcf3f712`
**Candidate bundle identity:** `sha256:f8cd4814c166f498c9e67b03a058f67737573fe2577972f51db989f355f93068`
**Work-order path:** `reviews/Feature-Phase7-Migration-Completion.md`
**Work-order mode:** `append`
**Pass judgment:** `changes-requested`

### Findings

- [x] **P7-018 — HIGH — security/correctness** — `crates/workboard-application/src/storage.rs:117`
  Migration 19 preserves structurally valid schema-17 attestations that audit 18 cannot consume. A direct
  batch with a non-`explicit_repair` authority remains immutable and occupies the repair primary key, while
  any pre-audit legacy-batch attestation conflicts with the evidence row the audit must create. Add a new
  forward-only pre-audit compatibility migration that removes every attestation unusable for its batch type,
  preserves consumable direct repairs and completed schema-18 evidence, and restores all three guards.
  Resolved by adding a new pre-audit compatibility migration that classifies attestations against the issued
  audit contract, removes only unusable pre-audit rows, preserves completed evidence, and reinstalls insert,
  update, and delete guards; the expanded schema-17 fixture proves each path and a healthy retry.

## Review pass — 2026-08-28 — incremental

**Candidate base:** `a43df92ba0e9929cf002d9782123c6c61fe57258`
**Candidate head:** `118cd48287ede1451bf898c4ef2952340a6fa384`
**Candidate branch:** `Feature/Phase7-Migration-Completion`
**Candidate scope:** `all`
**Candidate path-set:** `sha256:335501e5eaca737f34a0cb43006d8cf5027a0d61ee71065c04c0aafd85397c3c` `(3 paths)`
**Candidate bundle:** `C:\Users\TOMMYS~1\AppData\Local\Temp\agent-workboard-review-6bd8d6e430434b769e8504d7592957ca`
**Candidate bundle identity:** `sha256:31bdc48cb7ee4f49807ce3e8afbd086bbdf8d8cc3baaa828676afeacaa51d284`
**Work-order path:** `reviews/Feature-Phase7-Migration-Completion.md`
**Work-order mode:** `append`
**Pass judgment:** `changes-requested`

### Findings

- [x] **P7-019 — HIGH — security/correctness** — `crates/workboard-application/src/storage.rs:1334`
  Migration 21 performs its classification cleanup only once. After audit 18 fails at that checkpoint, a
  same-workspace direct attestation with a non-`explicit_repair` authority or any pre-audit legacy attestation
  passes the repository-only insert guard, becomes immutable, and is never cleaned because migration 21 is
  already stamped. Re-run classification cleanup before every pending audit retry or install a repair-aware
  guard, and prove post-checkpoint unusable rows remain recoverable while a genuine schema-20 audited row is
  preserved unchanged through the final guard upgrade.
  Resolved by re-running the transactional classification cleanup before every pending audit attempt. The
  regression injects unusable rows after schema 21, proves retry removes them, and verifies a complete
  schema-20 attestation set and all three guards survive upgrade to schema 22 unchanged.

## Review pass — 2026-08-28 — incremental

**Candidate base:** `118cd48287ede1451bf898c4ef2952340a6fa384`
**Candidate head:** `68b68fa77303c782ecb67876d138c2aee00cfb3c`
**Candidate branch:** `Feature/Phase7-Migration-Completion`
**Candidate scope:** `all`
**Candidate path-set:** `sha256:335501e5eaca737f34a0cb43006d8cf5027a0d61ee71065c04c0aafd85397c3c` `(3 paths)`
**Candidate bundle:** `C:\Users\TOMMYS~1\AppData\Local\Temp\agent-workboard-review-aa364eb9bc6c46058fca1f4e6f45594f`
**Candidate bundle identity:** `sha256:53d706ac12023779f403d040d3d34570ed340679211045c259c90e669b09984c`
**Work-order path:** `reviews/Feature-Phase7-Migration-Completion.md`
**Work-order mode:** `append`
**Pass judgment:** `approved`

### Findings

No findings.

## Review pass — 2026-08-28 — incremental

**Candidate base:** `68b68fa77303c782ecb67876d138c2aee00cfb3c`
**Candidate head:** `a6c8dee348f58311c36caa127b7974b3fadd7628`
**Candidate branch:** `Feature/Phase7-Migration-Completion`
**Candidate scope:** `all`
**Candidate path-set:** `sha256:e8367bddbe959b15618850b768fa3ad4596dc79493d3fccc4331021b9d55e690` `(4 paths)`
**Candidate bundle:** `C:\Users\TOMMYS~1\AppData\Local\Temp\agent-workboard-review-2de27941d86f45f8836338a049855307`
**Candidate bundle identity:** `sha256:403f92e26cd1c30d152551c8556eaeaa5eeca417063c94fac91d9c2d420fd4e4`
**Work-order path:** `reviews/Feature-Phase7-Migration-Completion.md`
**Work-order mode:** `append`
**Pass judgment:** `changes-requested`

### Findings

- [x] **P7-020 — HIGH — integrity/test** — `crates/workboard-application/src/concertable_import.rs:554`
  Replay treats every planning-store document revision at the batch's Git commit as an import member. A
  publish-before-database failure followed by unrelated planning work can make an unchanged retry record that
  later commit, so replay overcounts unrelated documents. Persist membership for every imported document,
  including source-less documents, and prove same-commit unrelated revisions cannot change the outcome.

  Resolved by schema 23's immutable per-import document membership. Schema-22 imports backfill membership
  from their original revision timestamp and commit; new imports record every document directly. The replay
  regression upgrades a schema-22 import, adds an unrelated same-commit revision, and preserves the original
  outcome exactly.

## Review pass — 2026-08-28 — incremental

**Candidate base:** `a6c8dee348f58311c36caa127b7974b3fadd7628`
**Candidate head:** `2a9c6233ca368e431051e1553692be56c447f921`
**Candidate branch:** `Feature/Phase7-Migration-Completion`
**Candidate scope:** `all`
**Candidate path-set:** `sha256:ee1d4e28380659a67492628d9d3728cfc1c21bcb266f3d6de81dbe3f80572119` `(4 paths)`
**Candidate bundle:** `C:\Users\TommySeery\AppData\Local\Temp\agent-workboard-review-87b342294f2440c6a98005a28fd19f29`
**Candidate bundle identity:** `sha256:929dcf81196d1a5f171401ae556a3843b9d5d070fe04fa80915f973fb33a9d4e`
**Work-order path:** `reviews/Feature-Phase7-Migration-Completion.md`
**Work-order mode:** `append`
**Pass judgment:** `changes-requested`

### Findings

- [x] **P7-021 — HIGH — integrity** — `crates/workboard-application/src/storage.rs:56`
  Membership rows are individually immutable, but a completed batch still accepts new same-workspace,
  same-kind documents. A later insert can therefore freeze unrelated membership into a completed import and
  inflate every replay. Finalize membership atomically with the import and reject later membership inserts
  and repository/kind reassignment of member documents.

  Resolved by schema 24's durable finalization row, written in the same database transaction as new import
  membership. Database guards freeze finalized batches, members, member identity fields, and source mappings;
  replay fails closed if an older matching batch lacks finalization.

- [x] **P7-022 — HIGH — migration/test** — `crates/workboard-application/src/storage.rs:72`
  Schema 23 stamps a commit-and-timestamp reconstruction without validating hierarchy completeness or
  ambiguity, while its regression inserts the unrelated collision only after backfill. Reconstruct from
  batch-owned hierarchy evidence, fail closed on missing or ambiguous membership, and seed the unrelated
  same-commit revision before upgrade so the test proves exact backfill.

  Resolved by the forward-only schema 24 rebuild and validation gate. It reconstructs membership from the
  batch's exact hierarchy timestamp, planning repository, document revision, commit, and source evidence;
  incomplete or ambiguous materialized batches roll back without a schema stamp. Regression coverage proves
  valid schema-22 backfill, missing-revision rejection, copied timestamp/commit rejection, and successful
  retry after evidence repair.

## Review pass — 2026-08-28 — incremental

**Candidate base:** `2a9c6233ca368e431051e1553692be56c447f921`
**Candidate head:** `0a9a67865225623cd9181ecf2f8d571280639393`
**Candidate branch:** `Feature/Phase7-Migration-Completion`
**Candidate scope:** `all`
**Candidate path-set:** `sha256:ee1d4e28380659a67492628d9d3728cfc1c21bcb266f3d6de81dbe3f80572119` `(4 paths)`
**Candidate bundle:** `C:\Users\TommySeery\AppData\Local\Temp\agent-workboard-review-b6bc61bb86e94348b93a83d14687f381`
**Candidate bundle identity:** `sha256:5d4ed4313e6be6dc110c22c454f0e401a20f305ef6c0269c2e81887d5f3a17be`
**Work-order path:** `reviews/Feature-Phase7-Migration-Completion.md`
**Work-order mode:** `append`
**Pass judgment:** `changes-requested`

### Findings

- [x] **P7-023 — HIGH — integrity** — `crates/workboard-application/src/storage.rs:178`
  The finalization trigger requires only one membership, so a partially populated legacy batch can be
  finalized and then trusted by replay. Apply the complete hierarchy, source-mapping, and generated-Epic
  evidence checks to every finalization insert, not only the schema-24 upgrade transaction.
  Resolved by schema 25's shared evidence-failure view, which backs both the forward migration validation
  and every later finalization insert; a partial two-document batch is rejected atomically.

- [x] **P7-024 — HIGH — integrity** — `crates/workboard-application/src/storage.rs:211`
  Finalization freezes a member document's repository and kind but not its owner columns or transitive
  hierarchy and repository links. Freeze imported document ownership, hierarchy parent/workspace links,
  planning-store ownership, and imported Work-item repository associations while leaving ordinary status
  and workflow-state updates available.
  Resolved by freezing every document owner field, hierarchy parent, workspace planning repository,
  planning-repository parent, and imported Work-item repository association after finalization. The focused
  regression rejects each identity mutation while allowing normal Feature workflow and Work-item status
  updates.

- [x] **P7-025 — MEDIUM — test** — `crates/workboard-application/src/concertable_import.rs:1674`
  The regression proves finalized membership insertion is blocked but never exercises the separate
  source-destination insert guard. Attempt a unique post-finalization source mapping and prove its row count
  and the replay outcome remain unchanged.
  Resolved by attempting a unique late source mapping, asserting the dedicated finalization error and an
  unchanged mapping count, then proving the replay still returns the original outcome.

## Review pass — 2026-08-28 — incremental

**Candidate base:** `0a9a67865225623cd9181ecf2f8d571280639393`
**Candidate head:** `6c711084c83b5654820eb357664b321dca192626`
**Candidate branch:** `Feature/Phase7-Migration-Completion`
**Candidate scope:** `all`
**Candidate path-set:** `sha256:ee1d4e28380659a67492628d9d3728cfc1c21bcb266f3d6de81dbe3f80572119` `(4 paths)`
**Candidate bundle:** `C:\Users\TommySeery\AppData\Local\Temp\agent-workboard-review-42ead55de9f5443ba1482a10997b7640`
**Candidate bundle identity:** `sha256:31589f1cd888d0df6bd5215a7a2030f06c0b600f260396f44a051e1fb43c5828`
**Work-order path:** `reviews/Feature-Phase7-Migration-Completion.md`
**Work-order mode:** `append`
**Pass judgment:** `changes-requested`

### Findings

- [x] **P7-026 — HIGH — integrity** — `crates/workboard-application/src/storage.rs:262`
  The expected-document view starts from existing documents and revisions, so a hierarchy entity missing
  either artifact disappears from both sides of the set comparison and a partial batch can still finalize.
  Derive completeness from the whole timestamp-owned hierarchy cohort and retain the application invariant
  that every Concertable import contains at least one Feature.
  Resolved by forward-only schema 26's cohort-evidence view and finalization guard. Focused regressions reject
  a complete Feature whose generated Epic document is missing and a source-backed Epic-only batch.

- [x] **P7-027 — HIGH — integrity** — `crates/workboard-application/src/storage.rs:273`
  Revision and hierarchy-timestamp evidence remains mutable after finalization, while replay checks only the
  finalization row. Revalidate complete evidence on replay so changing or removing original evidence, or
  adding same-batch evidence, returns an integrity error instead of trusted counts.
  Resolved by joining both membership and cohort failures into replay. Revision-timestamp changes,
  hierarchy-timestamp changes, and extra same-batch documents all fail closed; restoring the exact evidence
  restores the original replay outcome.

- [x] **P7-028 — MEDIUM — test** — `crates/workboard-application/src/concertable_import.rs:1884`
  The runtime finalization regression does not prove migration 25 rejects a partial finalization already
  accepted by schema 24. Build that schema-24 state, prove upgrade rollback preserves its version and rows,
  repair the missing evidence, and prove retry reaches schema 25 without changing a valid replay outcome.
  Resolved by a schema-24 fixture that accepts one of two memberships, proves migration 25 rolls back without
  a stamp or lost rows, repairs the missing membership and source mapping, and then reaches healthy schema 26
  with the original valid replay counts unchanged.

## Review pass — 2026-08-28 — incremental

**Candidate base:** `6c711084c83b5654820eb357664b321dca192626`
**Candidate head:** `75298f3a57263cd613a42d4aba5b122ada0c07b5`
**Candidate branch:** `Feature/Phase7-Migration-Completion`
**Candidate scope:** `all`
**Candidate path-set:** `sha256:ee1d4e28380659a67492628d9d3728cfc1c21bcb266f3d6de81dbe3f80572119` `(4 paths)`
**Candidate bundle:** `C:\Users\TommySeery\AppData\Local\Temp\agent-workboard-review-02217b8f3c594509b4f6fae07d7d4e76`
**Candidate bundle identity:** `sha256:054d698dd915b22d28826b4c3ea50a67fe14525519d3020562989134b2995321`
**Work-order path:** `reviews/Feature-Phase7-Migration-Completion.md`
**Work-order mode:** `append`
**Pass judgment:** `changes-requested`

### Findings

- [x] **P7-029 — MEDIUM — test** — `crates/workboard-application/src/concertable_import.rs:2347`
  The upgrade regression deletes migration 25 and therefore cannot prove the forward-only schema-26 path
  rejects invalid cohort evidence in a genuinely stamped schema-25 database. Preserve the canonical schema-25
  stamp, prove schema 26 rolls back without lost rows, repair the cohort, and prove healthy retry and replay.
  Resolved by extending the migration fixture with a canonical schema-25 Epic-only finalization. Schema 26
  rejects it without a stamp or lost rows; adding its missing Feature evidence reaches healthy schema 28 and
  preserves the original replay outcome.

- [x] **P7-030 — HIGH — integrity** — `crates/workboard-application/src/storage.rs:273`
  Finalized revision evidence is identified only by document, commit, and timestamp, so a matching revision
  can be duplicated, mutated, or replaced while replay remains trusted. Persist the exact revision identity
  and hash for every member and fail closed on any matching-row ambiguity or evidence mismatch while allowing
  ordinary later revisions with a different observation tuple.
  Resolved by schemas 27 and 28's immutable per-member revision number and content hash. Replay rejects hash
  mutation, duplicate observation tuples, and revision replacement while accepting a later revision with a
  different commit and timestamp.

- [x] **P7-031 — HIGH — integrity** — `crates/workboard-application/src/storage.rs:368`
  Source provenance has no immutable per-member cardinality or synthetic marker. A missing source-backed Epic
  mapping can be mistaken for a generated Epic, and multiple mappings can inflate replay. Persist explicit
  source versus synthetic provenance, require exactly one mapping for sourced members and zero only for an
  attested synthetic Epic, and derive replay counts from that durable evidence.
  Resolved by immutable source/synthetic evidence, preview-time synthetic attestations, explicit repair
  attestations for legacy imports, exact source cardinality validation, and replay counts derived from the
  evidence table. Missing and duplicate mappings fail finalization while normal synthetic import succeeds.

## Review pass — 2026-08-28 — incremental

**Candidate base:** `75298f3a57263cd613a42d4aba5b122ada0c07b5`
**Candidate head:** `cac358f0e1201222dd94d1454effde531ed297f9`
**Candidate branch:** `Feature/Phase7-Migration-Completion`
**Candidate scope:** `all`
**Candidate path-set:** `sha256:ee1d4e28380659a67492628d9d3728cfc1c21bcb266f3d6de81dbe3f80572119` `(4 paths)`
**Candidate bundle:** `C:\Users\TommySeery\AppData\Local\Temp\agent-workboard-review-c77cd782844e4e97aefac6e38c7802ac`
**Candidate bundle identity:** `sha256:f09ff04deb7f6e06e15e0a202450c05843a96ade11626d8ac57966b00ae67e3f`
**Work-order path:** `reviews/Feature-Phase7-Migration-Completion.md`
**Work-order mode:** `append`
**Pass judgment:** `changes-requested`

### Findings

- [x] **P7-032 — MEDIUM — integrity** — `crates/workboard-application/src/storage.rs:733`
  A preview-time synthetic attestation can be inserted before a source mapping, after which source evidence
  and the contradictory immutable attestation can both survive finalization and replay. Add reciprocal source
  guards and a forward-only validation that requires source evidence to have no synthetic attestation and
  synthetic evidence to retain exactly one valid attestation.
  Resolved by schema 29's reciprocal source/attestation triggers and provenance-consistency view, which is
  enforced during finalization, upgrade, and replay. Focused regressions reject both late source forms and
  prove a missing finalized synthetic attestation stops schema 29 until explicitly repaired.

- [x] **P7-033 — MEDIUM — test** — `crates/workboard-application/src/concertable_import.rs:1719`
  Schema 28 lacks a direct canonical schema-26 upgrade fixture containing source or revision ambiguity.
  Prove the migration stops at schema 27 without a schema-28 stamp or partial evidence, preserves every row,
  then succeeds with exact evidence and unchanged replay after the ambiguity is repaired.
  Resolved by a canonical schema-26 fixture with duplicate source cardinality. It proves schema 27 remains
  stamped, schema 28 and partial evidence remain absent, all legacy rows survive, and exact evidence plus the
  original replay result are restored after repair.

- [x] **P7-034 — MEDIUM — test** — `crates/workboard-application/src/storage.rs:723`
  The new evidence and attestation update/delete guards have no direct regression assertions. Prove all four
  mutations are rejected, the rows remain byte-for-byte unchanged, and replay remains equal afterward.
  Resolved by direct update/delete assertions for both durable evidence tables, byte-for-byte row comparison,
  and the unchanged idempotent replay assertion in the same test.

## Review pass — 2026-08-28 — incremental

**Candidate base:** `cac358f0e1201222dd94d1454effde531ed297f9`
**Candidate head:** `ed015849ac9ecb38ddea67d1af48e3c44751d783`
**Candidate branch:** `Feature/Phase7-Migration-Completion`
**Candidate scope:** `all`
**Candidate path-set:** `sha256:ee1d4e28380659a67492628d9d3728cfc1c21bcb266f3d6de81dbe3f80572119` `(4 paths)`
**Candidate bundle:** `C:\Users\TommySeery\AppData\Local\Temp\agent-workboard-review-127a805e1aa242eaa2fbefbf786382cc`
**Candidate bundle identity:** `sha256:604ece9b44c7d45e22f66b38f44432596429b5b35f7ed695695b1b83b66dd331`
**Work-order path:** `reviews/Feature-Phase7-Migration-Completion.md`
**Work-order mode:** `append`
**Pass judgment:** `changes-requested`

### Findings

- [x] **P7-035 — HIGH — integrity** — `crates/workboard-application/src/storage.rs:723`
  With SQLite recursive triggers disabled, `INSERT OR REPLACE` can replace durable evidence or an attestation
  without firing their delete guards. Add insert-time primary-key conflict guards so coordinated revision and
  evidence replacement, or attestation replacement, aborts and preserves the original rows and replay trust.
  Resolved by schema 30's insert-time primary-key conflict guards. With recursive triggers asserted off, the
  regression proves coordinated revision/evidence replacement and attestation replacement both roll back,
  leave the original rows byte-for-byte unchanged, and preserve the accepted replay.

- [x] **P7-036 — MEDIUM — test** — `crates/workboard-application/src/storage.rs:987`
  Schema 29's direct upgrade fixture covers missing synthetic attestation but not the legacy contradiction of
  source evidence plus a synthetic attestation. Build that exact stamped schema-28 state, prove migration 29
  rolls back without lost evidence or a stamp, then repair it and prove healthy replay equality.
  Resolved by a stamped schema-28 fixture that creates the formerly legal source-evidence-plus-attestation
  state, proves schema 29 leaves both later stamps absent and preserves mapping, evidence, attestation,
  membership, and finalization rows, then reaches healthy schema 30 with unchanged replay after repair.

## Review pass — 2026-08-28 — incremental

**Candidate base:** `ed015849ac9ecb38ddea67d1af48e3c44751d783`
**Candidate head:** `7a083fd4641d754945e392940aa152d1374e11d8`
**Candidate branch:** `Feature/Phase7-Migration-Completion`
**Candidate scope:** `all`
**Candidate path-set:** `sha256:ee1d4e28380659a67492628d9d3728cfc1c21bcb266f3d6de81dbe3f80572119` `(4 paths)`
**Candidate bundle:** `C:\Users\TommySeery\AppData\Local\Temp\agent-workboard-review-4122d2c716344679bb2ea4436d53f814`
**Candidate bundle identity:** `sha256:99e8f7cd907312e2b9632838d31eaba0bdee2527ac895e9b32afcf5ba488fddb`
**Work-order path:** `reviews/Feature-Phase7-Migration-Completion.md`
**Work-order mode:** `append`
**Pass judgment:** `changes-requested`

### Findings

- [x] **P7-037 — LOW — documentation** — `plans/agent-workboard/AGENT_WORKBOARD_V0_PROGRESS.md:88`
  The verification ledger still reports 79 application and 137 workspace tests, while this candidate contains
  82 and 140. Rerun both documented test commands against `7a083fd` and record the actual passing counts.
  Resolved by rerunning both documented commands against `7a083fd`: all 82 application tests and all 140
  workspace tests pass, and the ledger now records those current counts and the exact stronger gate commands.

## Review pass — 2026-08-28 — incremental

**Candidate base:** `7a083fd4641d754945e392940aa152d1374e11d8`
**Candidate head:** `8300b20f989f0aaaccce3891577c5705c00a23f5`
**Candidate branch:** `Feature/Phase7-Migration-Completion`
**Candidate scope:** `all`
**Candidate path-set:** `sha256:a031cc4c7726b5c9bacc924301d181b763d57ac7ed702d8b133adb04abc81782` `(2 paths)`
**Candidate bundle:** `C:\Users\TommySeery\AppData\Local\Temp\agent-workboard-review-dbf0c1db05954b50bedcaa314a6230d1`
**Candidate bundle identity:** `sha256:f1d1e860ae9f43e2a70471f943810b2195ec92fa1ee0e0a744f3e9c44cec8671`
**Work-order path:** `reviews/Feature-Phase7-Migration-Completion.md`
**Work-order mode:** `append`
**Pass judgment:** `approved`

### Findings

No findings.

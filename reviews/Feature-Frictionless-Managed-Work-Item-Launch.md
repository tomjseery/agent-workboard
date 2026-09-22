# Code review — Feature/Frictionless-Managed-Work-Item-Launch

> **This file is a work order, not a discussion.** If you're handed this file, fix the open `[ ]`
> findings directly and report what changed. Tick each `[x]` as you land it. Pause only for a genuinely
> irreversible or ambiguous finding: record its durable disposition, take the safe path, and keep going.

**Review status:** `complete`
**Judgment:** `approved`
**Reviewed up to commit:** `ac66fe20f80b0e506a557cd73aaacc8ea57f4afe`

## Review pass — 2026-08-29 — full

**Candidate base:** `3ad84574fb726994aa5f9fdd331785cc7776cefc`
**Candidate head:** `0d644122e039e7f84cfb920ba97b4bab24ce88b5`
**Candidate branch:** `Feature/Frictionless-Managed-Work-Item-Launch`
**Candidate scope:** `all`
**Candidate path-set:** `sha256:94b15c9e2ea0418c30edf0bf62a9ae2d4ff4a407322735791125f2e82ec9ab5e` `(11 paths)`
**Candidate bundle:** `C:\Users\TOMMYS~1\AppData\Local\Temp\agent-workboard-review-0c7f0b34dc9e45c6b5a59fba65bc980f`
**Candidate bundle identity:** `sha256:f96c083bf57a1dec3d3610a1766c005306ae870667c07f9077338059b40172cb`
**Work-order path:** `reviews/Feature-Frictionless-Managed-Work-Item-Launch.md`
**Work-order mode:** `new`
**Pass judgment:** `changes-requested`

### Findings

- [x] **Major — Publish repository selection on the managed-session MCP contract**
  `crates/workboard-cli/src/mcp.rs:130` exposes `repositoryId` on `work_checkpoint` but omits it from the `session_request` schema, whose `additionalProperties: false` rejects it. Multi-repository managed launches therefore cannot supply the repository required by the typed request.
  Disposition: moved the optional property to `session_request` and added a schema regression test.

- [x] **Major — Keep the selected repository through direct Work-item launch**
  `crates/workboard-cli/src/lib.rs:883` prepares the selected repository and then calls repository-agnostic `effective_work_item_checkout`, which rejects every multi-repository Work item. Launch from the exact readiness returned by checkout preparation.
  Disposition: direct launch now uses the checkout ID and path returned by the selected repository's readiness result.

- [x] **Major — Bind idempotent session retries to their durable checkout**
  `crates/workboard-application/src/workflow_operations.rs:261` hides stored requests when their checkout becomes unavailable, then reconstructs an outcome from the current effective checkout without validating the repository. A retry can reuse one request ID against a different repository or checkout. Read and validate the durable request independently of availability and fail reconciliation when its recorded checkout is unavailable.
  Disposition: durable requests now resolve their recorded checkout regardless of availability, and transaction races validate both stored repository and checkout before returning. Regression coverage keeps a missing checkout bound to its request and rejects cross-repository reuse.

- [x] **Major — Correct persisted availability for occupied recorded targets**
  `crates/workboard-application/src/checkout.rs:99` rejects an existing recorded target that becomes a non-Git directory or non-directory without calling `record_unavailable`. Launch fails closed, but the durable readiness remains falsely `available`.
  Disposition: occupied recorded targets now persist missing availability and reconciliation evidence before returning the conflict; a regression test covers the non-Git directory case.

## Review pass — 2026-08-29 — incremental

**Candidate base:** `0d644122e039e7f84cfb920ba97b4bab24ce88b5`
**Candidate head:** `41096d2ef769e2e8dc4aa784fb1d58bb89d29823`
**Candidate branch:** `Feature/Frictionless-Managed-Work-Item-Launch`
**Candidate scope:** `incremental`
**Candidate path-set:** `sha256:65926ec0b22d8924c61972920fa079584df9a2c17e58494cc9efd5a8caae4f6e` `(4 paths)`
**Candidate bundle:** `C:\Users\TOMMYS~1\AppData\Local\Temp\agent-workboard-incremental-review-d315ed9ab7834b5f9af645523280743c`
**Candidate bundle identity:** `sha256:4c6e5c990e51043632b93b2cc4e6f96bbcae90688db789d84dad0eebb036a06c`
**Work-order path:** `reviews/Feature-Frictionless-Managed-Work-Item-Launch.md`
**Work-order mode:** `append`
**Pass judgment:** `approved`

### Findings

No findings.

## Review pass — 2026-08-29 — incremental

**Candidate base:** `41096d2ef769e2e8dc4aa784fb1d58bb89d29823`
**Candidate head:** `cdf30888ecdfea47f1a07b5075c98c91cc4ec55a`
**Candidate branch:** `Feature/Frictionless-Managed-Work-Item-Launch`
**Candidate scope:** `incremental`
**Candidate path-set:** `sha256:1a4d5c03f8fb2fe3b8e3db939ca129858faf5bdd8a348de97417afc96983db4e` `(52 paths)`
**Candidate bundle:** `C:\Users\TOMMYS~1\AppData\Local\Temp\agent-workboard-review-01ecc559a997453e8588278239b79afb`
**Candidate bundle identity:** `sha256:8d60a488aeedfc0d6b81057c7079ebdb3699c99f1c5114bcdecc0446864f4fc7`
**Work-order path:** `reviews/Feature-Frictionless-Managed-Work-Item-Launch.md`
**Work-order mode:** `append`
**Pass judgment:** `changes-requested`

### Findings

- [x] **Major — Bind review integration to the managed session's checkout**
  `crates/workboard-application/src/work_item_state.rs:292` publishes every review transition from `effective_work_item_checkouts` and discards the authenticated principal's checkout. A debugging or review session can therefore checkpoint one checkout while queuing a different effective checkout for integration; reconciliation also has no durable checkout identity to recover. Persist the managed actor's checkout with the staged update and select that exact checkout during finalization.
  Disposition: staged history now records the authenticated checkout, reconciliation restores it, and review finalization selects it exactly while retaining completed integration evidence for an unchanged head. A managed-session regression test proves both history and integration use that checkout.

- [x] **Major — Fail closed when the canonical Work-item document is missing**
  `crates/workboard-application/src/work_item_state.rs:394` and `crates/workboard-application/src/workspace.rs:424` translate every `WorkItemNotFound` from authoritative state reads into an empty revision-1 state. A Work-item whose canonical planning document or planning-store path is missing is consequently reported as a valid legacy empty state. Keep fixture compatibility in the document-revision query, but propagate genuinely missing authority instead of manufacturing state.
  Disposition: authoritative reads now propagate missing document and planning-path failures, while a left join defaults only absent revision-history rows to revision 1. A regression test retires the planning-store path and proves the projection fails closed.

## Review pass — 2026-09-12 — incremental

**Candidate base:** `cdf30888ecdfea47f1a07b5075c98c91cc4ec55a`
**Candidate head:** `80b12a3f2df04a8c127e185399f832dcefd2b808`
**Candidate branch:** `Feature/Frictionless-Managed-Work-Item-Launch`
**Candidate scope:** `incremental`
**Candidate path-set:** `sha256:54967c31ce5d128f10164b472852fba264c36f5b225861afc224f11490a30c20` `(6 paths)`
**Candidate bundle:** `manual frozen git range`
**Candidate bundle identity:** `sha256:b554d4416ef5e58705f327153d6226659ead5a7fe3b805bc7f15730676bd0ee4`
**Work-order path:** `reviews/Feature-Frictionless-Managed-Work-Item-Launch.md`
**Work-order mode:** `append`
**Pass judgment:** `changes-requested`

### Findings

- [x] **Major — Require every configured repository before marking an item done**
  `crates/workboard-application/src/feature_integration.rs:432` marks a reviewed Work item done when none of its existing integration rows remain incomplete. Managed structured updates intentionally stage only the authenticated checkout, so a multi-repository item can have no row for another configured repository and satisfy this condition vacuously after the first merge. Compare against `work_item_repositories` and require an integrated row for every configured repository.
  Disposition: completion now anti-joins every configured repository against integrated evidence, and a multi-repository regression keeps the item in review when one repository has no integration record.

## Review pass — 2026-09-12 — incremental

**Candidate base:** `80b12a3f2df04a8c127e185399f832dcefd2b808`
**Candidate head:** `e8b5c335cc1f7cca026bc3c6bb5ace1696ce6307`
**Candidate branch:** `Feature/Frictionless-Managed-Work-Item-Launch`
**Candidate scope:** `incremental`
**Candidate path-set:** `sha256:2d06de87196b6542d68ae4d406f51425310d200f975bac24cefa4cb212b949c2` `(2 paths)`
**Candidate bundle:** `manual frozen git range`
**Candidate bundle identity:** `sha256:ff2662c78716e57e9a73c60e5060ee65b37fa9010c10cb062359a687af8ce19e`
**Work-order path:** `reviews/Feature-Frictionless-Managed-Work-Item-Launch.md`
**Work-order mode:** `append`
**Pass judgment:** `changes-requested`

### Findings

- [x] **Minor — Keep the verification checkpoint accurate and inside its list item**
  `plans/agent-workflow-state-authority/FRICTIONLESS_MANAGED_WORK_ITEM_LAUNCH_AND_RECOVERY_PLAN.md:315` drops the continuation indentation before `checkout integration`, breaking the verification bullet, and still reports 136 application tests although the new repository-completeness regression raises the suite to 137. Restore the indentation and recorded count.
  Disposition: restored the list continuation indentation and recorded the 137-test application result.

## Review pass — 2026-09-12 — incremental

**Candidate base:** `e8b5c335cc1f7cca026bc3c6bb5ace1696ce6307`
**Candidate head:** `ac66fe20f80b0e506a557cd73aaacc8ea57f4afe`
**Candidate branch:** `Feature/Frictionless-Managed-Work-Item-Launch`
**Candidate scope:** `incremental`
**Candidate path-set:** `sha256:fe87f0df8153026ddcd5b341b70643848724c3144cecce2b4d7f0bec1df4ae2a` `(1 path)`
**Candidate bundle:** `manual frozen git range`
**Candidate bundle identity:** `sha256:32404a4752816a2d4126f0c8624a11b6859f897ac4979b6267abb9bf14bcea48`
**Work-order path:** `reviews/Feature-Frictionless-Managed-Work-Item-Launch.md`
**Work-order mode:** `append`
**Pass judgment:** `approved`

### Findings

No findings.

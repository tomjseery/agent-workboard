# Agent Workboard desktop UI plan

## Planning status

- Status: active bootstrap plan
- Authority: this file, until the Workboard migration gate in [`../PLANS.md`](../PLANS.md) is satisfied
- Feature: `desktop-board-session-control-ui`
- Feature ID reserved by Workboard: `9f8878d2-e723-45f0-9308-dc83f954e1bf`
- Failed planning run: `6b86cc73-2454-44d2-ac2b-8b3471f41973`
- Source proposal: `desktop-board-session-control-ui-proposal-v1-20260829`
- Delivery branch at plan creation: `feature/desktop-board-session-control-ui`
- Last reconciled with `origin/main`: `6ea1b76`

The source proposal was approved, but publication moved from `publishing` to `reconciliation_required`. No Feature document, Work-item document, Work-item row, dependency row, or planning-store commit was created. The ten proposed items are materialized here so delivery is not blocked by Workboard bootstrapping itself.

Do not manually repair the live SQLite database or planning-store repository from this plan. When Workboard publication is reliable, import the remaining items by stable slug, verify content and dependencies, then delete this file as described in [`../PLANS.md`](../PLANS.md).

## Outcome

Deliver an optional Windows-first Tauri v2 desktop client that makes Workboard hierarchy, readiness, approvals, durable Work-item state, repositories/checkouts, and authoritative Claude/Codex sessions understandable and controllable.

Desktop is a client of Workboard, not another workflow authority. The daemon/application remains the only process that opens SQLite, reads or commits the planning store, reconciles Git, mutates workflow state, binds sessions, and launches or resumes providers.

The first accepted increment is read-only observability. A control appears only when the daemon advertises a compatible typed operation and its current authoritative availability. Missing capabilities produce an explicit read-only gate.

## Non-goals

- Do not embed Claude or Codex chat.
- Do not let React, Tauri commands, or a desktop plugin open SQLite or the planning store.
- Do not shell out to the `workboard` CLI from Desktop.
- Do not let Desktop inspect Git, worktrees, processes, transcripts, or provider state independently.
- Do not reproduce workflow transition, readiness, dependency, approval, checkout, or session-liveness rules in TypeScript.
- Do not expose daemon, workflow, or provider credentials to JavaScript.
- Do not add remote content, remote capabilities, a generic shell/filesystem bridge, or a second UI framework.
- Do not block read-only delivery on later workflow-control Features.

## Authority and trust boundaries

1. `workboard-application` owns business operations and authoritative projections.
2. `workboard-daemon` owns the live application instance, SQLite connections, planning-store access, provider refresh, ordered event publication, and request serialization.
3. `workboard-client-protocol` owns versioned transport-neutral wire contracts only.
4. `workboard-client` owns endpoint discovery, authentication, framing, negotiation, requests, subscriptions, reconnect, and cursor replay.
5. CLI, TUI, and Tauri consume `workboard-client`; presentation code does not open application/storage infrastructure.
6. The Tauri Rust shell retains the daemon endpoint credential and exposes only typed allowlisted IPC commands and channels.
7. React owns rendering and ephemeral interaction state. Server facts remain in React Query and are never mirrored into Zustand.

Every mutation carries an idempotency key, expected revision, explicit intent, and any required preview or confirmation token. The daemon reauthorizes and revalidates at execution time. Markdown, prompt previews, paths, branches, provider metadata, and diagnostics are untrusted display content.

## Target repository shape

```text
crates/
├── workboard-core/
├── workboard-application/
├── workboard-client-protocol/
├── workboard-client/
├── workboard-daemon/
└── workboard-cli/
apps/
└── workboard-desktop/
    ├── package.json
    ├── src/
    │   ├── app/
    │   ├── components/ui/
    │   ├── core/
    │   ├── features/
    │   ├── lib/
    │   └── routes/
    └── src-tauri/
```

The protocol crate may depend on serialization and identity primitives but never on application, storage, Git, native-provider, CLI, Tauri, or React code. Published projections are dedicated DTOs; internal database/domain entities do not become accidental wire contracts.

## Frontend rules

- React Query owns every daemon query and mutation.
- Zustand owns only unsaved filters, draft lane layout, focused card, panel state, and equivalent client-only state through feature facade hooks.
- TanStack Router owns typed routes and zod-validated search parameters.
- zod parses environment, route, and user-editable form boundaries. Negotiated first-party daemon responses use generated contract types rather than a handwritten validation mirror.
- Feature slices own one `types.ts`, plus `api`, `hooks`, `components`, `pages`, `model`, and `schemas` as needed. Generated contract names never reach a feature; `core/contracts.ts` re-exports domain names over `core/generated`.
- Raw React Query hooks use `Query`/`Mutation` suffixes. Facade hooks return domain-shaped state and operations. Components render.
- Tailwind provides styling through one `cn` helper (`src/lib/utils.ts`) and `cva` variants. Owned accessible primitives live in `src/components/ui/`. Use one icon set (`lucide-react`).
- dayjs is hidden behind one named date-formatting module (`src/lib/dates.ts`).
- Closed protocol discriminants render through one exhaustive table per concern.
- Effects are reserved for external subscriptions and DOM integration, not derived state or request orchestration.

## Verification ownership

The plan explicitly adopts the following test tiers:

- Rust unit tests: codecs, reducers, bounded framing, compatibility tables, projection mapping, and deterministic policies.
- Rust integration tests: daemon/application/storage transactions, authentication, idempotency, revisions, event replay, restart, and partial outcomes.
- Vitest Node: schemas, request shaping, query keys, store actions, pure derived UI models, and exhaustive presentation tables.
- Vitest Browser Mode with Playwright: component interaction, accessibility, focus, real browser APIs, CSS/layout behavior, and hard-to-reach states.
- Tauri mock-runtime integration: command allowlists, IPC validation, secret containment, channel lifecycle, and daemon error mapping.
- Packaged Windows WebDriver: installed routed workflows, daemon composition, provider/terminal fakes, upgrade/repair/uninstall, and recovery.

Do not repeat the same behavior at every tier. Each test belongs to the narrowest tier that can prove the application-owned contract honestly.

## Developer loop

`apps/workboard-desktop/scripts/install-desktop.ps1` is the one command for the installed build. It stops
the app, its own WebView2 children and the daemon, runs `npm run app`, copies the result into
`%LOCALAPPDATA%\Programs\Agent Workboard Desktop`, creates the desktop shortcut if it is missing, and
relaunches. `-SkipBuild`, `-DaemonToo` and `-NoLaunch` cover the variants. A failed build aborts before any
copy, so a stale binary never lands and reports success. `workboard.exe` is copied only when its hash
changes, because the daemon is a separate `cargo build --release -p workboard-cli` artifact that moves only
when the Rust client protocol does.

The script stops only this application's WebView2 processes: the descendants of its own process, plus any
`msedgewebview2.exe` whose command line carries `dev.agentworkboard.desktop`. Never widen that to every
`msedgewebview2.exe` — Teams, WhatsApp, Widgets, Copilot and Windows Search each host their own, and a
blanket stop takes those applications down with it.

For UI iteration prefer `npm run app:dev`, which serves the frontend from vite with hot module reload and
removes the rebuild-and-copy cycle. Two constraints come with it:

- Set `WORKBOARD_DAEMON` to an existing `workboard.exe` (the installed one will do). A dev binary has no
  sibling daemon to spawn, so without it the window cannot connect.
- `tauri dev` compiles `--no-default-features --cfg dev` and shares no artifacts with `npm run app`, so it
  carries its own multi-gigabyte `target/debug` on first run. Check free disk before starting.

The crate sets `default-run` so that the bare `cargo run` inside `tauri dev` resolves the application
binary; without it the two `[[bin]]` targets make `tauri dev` fail outright. Keep it in step if a third
binary is ever added.

**Dev mode is for iteration speed, not acceptance.** It serves over http with no CSP where the packaged
build enforces one, and the Tauri mock-runtime tests never exercise the real webview, the CSP or the
isolation frame — a fully green suite has previously coexisted with a permanently stuck "Connecting to
Workboard" window. Verify every change on the installed build before treating it as done.

## Framework references

The Tauri decisions in this plan are pinned to the current official v2 guidance:

- [Capabilities](https://v2.tauri.app/reference/acl/capability/) define the IPC boundary per window/webview.
- [Isolation pattern](https://v2.tauri.app/concept/inter-process-communication/isolation/) intercepts frontend IPC before Tauri Core.
- [Content Security Policy](https://v2.tauri.app/security/csp/) should be enabled and restricted to trusted bundled content.
- [Tauri Channels](https://v2.tauri.app/develop/calling-frontend/#channels) provide ordered high-throughput Rust-to-frontend delivery.
- [Windows installer](https://v2.tauri.app/distribute/windows-installer/) documents NSIS, per-user installation, and WebView2 installation modes.

## Dependency graph

```text
1 protocol and stream
└── 2 secure Tauri shell
    └── 3 generated TypeScript contract
        └── 4 hierarchy and saved views
            ├── 5 board and attention
            │   └── 7 proposals and approvals
            └── 6 repository, checkout, and session observability
                └── 8 Work-item detail and checkpoints
            5 + 6 + 8 ── 9 session controls
            5 + 7 + 8 + 9 ── 10 hardening and packaging
```

Items 5 and 6 may proceed in parallel after item 4. Read-only portions of items 7–9 may land before their upstream mutation contracts; controls remain absent or disabled until the daemon advertises them.

## Upstream integration policy

The following active Features are upstream inputs, not branches to merge speculatively:

- Frictionless managed Work-item launch and recovery
- Autonomous managed workflow continuation and publication policy
- Dependency-aware Epic continuation
- Managed-session capability injection reconciliation

Each Work item starts from the latest accepted `origin/main`. Consume only merged contracts. If an upstream capability is absent, implement the read-only projection/gate and record the missing stable capability code in the acceptance fixture. Never copy an active branch's private type or infer its eventual contract.

## Work-item execution rules

1. Select only an unchecked item whose dependencies and stated upstream gates are satisfied.
2. Reconcile the branch with `origin/main` before implementation.
3. Keep one coherent implementation boundary per PR. Split an item only by updating this plan first with explicit new slugs and dependencies.
4. Run the narrow local checks during development and `scripts/Test.ps1` before completion.
5. Mark an item complete only when every completion gate is met on the exact delivered head.
6. Record durable progress in commits and this checklist; do not add `_PROGRESS.md`.

## Work items

### 1. Establish the versioned Workboard client protocol and ordered daemon stream

- Status: [x]
- Slug: `establish-versioned-workboard-client-protocol`
- Dependencies: none
- Delivery type: Rust protocol/application/daemon foundation

#### Objective

Make the daemon/application the single runtime authority behind a versioned transport-neutral API used by CLI, TUI, Desktop, tests, and future clients.

#### Existing seam

`workboard-daemon` already provides authenticated loopback TCP, endpoint registration, bounded JSON requests, a serialized writer, and native-session watcher refresh. Its v1 protocol exposes only `Ping` and `RefreshNativeSessions`; responses contain untyped `serde_json::Value`; clients read until EOF; subscriptions, revisions, request IDs, compatibility negotiation, projections, and replay do not exist. CLI/TUI still instantiate `WorkboardApplication` directly.

#### Deliverables

1. Add `workboard-client-protocol` with version constants and typed published contracts:
   - handshake request/response;
   - request, response, typed error, diagnostic, available-action, and partial-outcome envelopes;
   - workspace/repository/Epic/Feature/Work-item/session identity references;
   - initial read query catalogue;
   - command capability catalogue with unavailable reasons for operations not yet accepted;
   - ordered workspace event envelope and cursor.
2. Add `workboard-client` with endpoint discovery, authenticated bounded framing, timeouts, one-shot requests, subscriptions, reconnect, cursor replay, and resync signaling.
3. Change daemon transport from EOF-delimited bodies to a length-prefixed bounded frame. Use one request/response connection for one-shot operations and a long-lived framed connection for subscriptions.
4. Negotiate current and previous read protocol versions. Commands require an explicitly compatible version and advertised capability. Preserve a bounded v1 compatibility path for existing daemon ping/native-refresh callers during rollout.
5. Add authoritative projection operations in `workboard-application` that map internal models to protocol DTOs. Initial accepted reads are handshake, workspace summary, hierarchy children, and the existing board snapshot replacement needed by CLI/TUI.
6. Add per-Workspace monotonic projection revisions and a durable bounded event journal. A committed application mutation and its event/outbox evidence must be atomic. Subscribers receive ordered events only after commit.
7. Define gap, expired cursor, daemon restart, incompatible event, and heartbeat-loss behavior. Each produces a typed resync requirement; the client performs an authoritative scoped requery rather than filling gaps locally.
8. Move bare-board and structured-show reads in CLI/TUI onto `workboard-client`. Keep command surfaces that have not migrated explicit rather than allowing a silent direct-storage fallback.
9. Add architecture checks preventing the client/protocol crates from depending on storage/application implementations and preventing migrated presentation paths from opening rusqlite or planning-store adapters.

#### Protocol contract

- Every request includes protocol version, request ID, operation, Workspace scope where applicable, expected revision where applicable, and idempotency key for mutation.
- Authentication is a transport concern retained by the Rust client; tokens never appear in result DTOs, events, diagnostics, logs, fixtures, or TypeScript output.
- Every response includes negotiated protocol version, request/correlation ID, Workspace ID where applicable, authoritative revision, server timestamp, result or typed error, diagnostics, available actions, and partial outcomes where applicable.
- Typed errors include stable code, safe message, severity, retryability, validation fields, stale/current revisions, reconciliation owner, and correlation ID. Human text is not a programmatic discriminator.
- Events include protocol/event version, Workspace ID, monotonic sequence, event ID, occurrence time, owner/entity and revision, kind, typed payload or invalidation scope, and operation correlation.
- Frame and collection limits are explicit and tested. Unknown commands, oversized frames, invalid IDs, control characters, and incompatible versions fail before application dispatch.

#### Persistence and ordering

- Add schema for Workspace projection revision and client-event journal/outbox.
- Allocate the next Workspace sequence in the same SQLite transaction as the authoritative mutation.
- Publish only committed events. A crash before commit publishes nothing; a crash after commit remains replayable.
- Retention records the oldest replayable cursor. Earlier cursors return `cursor_expired` plus the required scoped resync.
- Planning-store or Git partial outcomes remain application reconciliation results; the event stream reports them and never converts them to success.

#### Verification

- Golden serialization tests cover every envelope/discriminant and current/previous read compatibility.
- Framing tests cover partial reads/writes, multiple frames, exact limit, over-limit, timeout, disconnect, and malformed length/body.
- Integration tests cover correct/wrong token, one writer under concurrent clients, idempotent retry, stale revision, atomic event commit, ordered replay, cursor expiry, dropped connection, heartbeat loss, daemon restart, resync, and partial outcomes.
- CLI/TUI tests prove migrated reads use the client and render the same authoritative data.
- Architecture tests fail on forbidden dependency edges or direct migrated presentation access to rusqlite/planning store/provider launch.
- Secret canaries are absent from serialized results, events, logs, and fixtures.
- `scripts/Test.ps1` passes.

#### Completion gate

The daemon is the only owner used by migrated CLI/TUI reads; a typed client can negotiate, query, subscribe, reconnect, and replay; event ordering survives restart; unsupported writes are advertised as unavailable; no presentation fallback opens storage directly.

#### Rollback

Keep the v1 daemon compatibility adapter until the first Desktop read-only milestone is accepted. A client negotiation failure becomes read-only/refused and never falls back to storage access.

### 2. Secure the Tauri v2 runtime and daemon IPC bridge

- Status: [x]
- Slug: `secure-tauri-runtime-and-ipc-bridge`
- Dependencies: item 1
- Delivery type: Rust/Tauri security shell

#### Objective

Create a least-privilege Tauri v2 shell that is only a Workboard client and can connect to a fake or real compatible daemon without giving JavaScript Workboard authority.

#### Deliverables

1. Add `apps/workboard-desktop` with a conventional `src-tauri` Rust app and bundled Vite React TypeScript frontend.
2. The Rust shell owns endpoint discovery/startup, the `workboard-client` instance, daemon credential, reconnect state, and subscription tasks.
3. Expose four typed IPC families: handshake, query, execute, and subscribe. Parse a closed generated discriminated request before forwarding. Do not expose raw sockets, URLs, tokens, command lines, or a generic invoke-by-name bridge.
4. Forward ordered daemon messages through Tauri Channels. Cleanly cancel the daemon subscription when the channel/window closes and prove reconnect does not leak duplicate subscriptions.
5. Configure the isolation pattern, one exact `main` window/webview capability, local content only, explicit custom-command permissions, and no remote capability URLs or wildcard labels.
6. Keep `withGlobalTauri` false. Configure a restrictive CSP for bundled self-hosted assets and required Tauri IPC sources only; no CDN, remote script, `unsafe-eval`, or asset protocol.
7. Register no SQL, filesystem, shell, HTTP, process, opener, updater, or dialog plugin. Opening a resource remains a future daemon operation.
8. Validate request byte size, discriminant, identifiers, revisions, and subscription lifecycle in Rust before forwarding. The daemon still performs authoritative authorization and validation.
9. Render only bootstrap states: connecting, disconnected, incompatible, read-only, resyncing, and ready. Do not add workflow controls.

#### Verification

- Tauri mock-runtime tests prove only the exact main window and declared commands can invoke the bridge.
- Static configuration tests pin isolation, capabilities, app-manifest commands, CSP, local-only assets, `withGlobalTauri: false`, and absence of forbidden plugins.
- Invalid/oversized/unrecognized requests fail before daemon forwarding.
- Channel tests cover ordered delivery, cancellation, reconnect, daemon restart, and no duplicate listener.
- Secret-canary tests prove endpoint/workflow/provider credentials never reach JavaScript, devtools output, events, URLs, errors, or logs.
- Architecture checks prove `src-tauri` cannot depend on `workboard-application`, rusqlite, planning-store, Git, native-provider, or launcher modules.
- Windows development smoke connects to a fake daemon without blocking the UI thread.

#### Completion gate

A secure empty desktop negotiates with the daemon and receives ordered typed status. Removing or disabling the app leaves CLI/TUI/daemon behavior unchanged.

#### Rollback

The app is additive. On configuration, security, or negotiation failure it shows a closed read-only/refused state and performs no mutation.

### 3. Generate TypeScript contracts and conformance fixtures

- Status: [x]
- Slug: `generate-typescript-contracts-and-conformance-fixtures`
- Dependencies: items 1 and 2
- Delivery type: deterministic contract generation

#### Objective

Make the canonical Rust protocol consumable from TypeScript without maintaining a handwritten second contract.

#### Deliverables

1. Pin one `ts-rs` generation toolchain in the protocol crate and export request, response, error, capability, projection, action, event, and cursor types.
2. Generate artifacts into `apps/workboard-desktop/src/core/generated`; generated files contain types and discriminants only, not workflow rules or React state.
3. Generate current/previous protocol conformance JSON fixtures from Rust.
4. Add a typed frontend daemon facade around the Tauri bridge. It exposes domain-specific methods and hides raw `invoke`/Channel use from features.
5. Add deterministic generation and check commands to the Rust workspace and npm scripts. `scripts/Test.ps1` runs drift and round-trip checks.
6. Reject token, credential, unrestricted path, provider command, or internal diagnostic fields from generated contracts through a static forbidden-field gate.

#### Verification

- Two clean generation runs are byte-identical.
- Rust fixtures deserialize through the generated TypeScript types/facade and TypeScript requests round-trip through Rust.
- Fixtures cover every discriminant, current/previous reads, incompatible commands, typed errors, event gaps, partial outcomes, and unknown future optional read fields.
- TypeScript contains no handwritten duplicate of a published protocol type.
- Generation drift fails `scripts/Test.ps1`.

#### Completion gate

React imports one generated first-party contract surface; protocol drift and forbidden fields fail deterministically.

#### Rollback

Generated output is reproducible from retained Rust sources. Removing Desktop does not remove or rewrite the daemon protocol.

### 4. Deliver Workspace hierarchy navigation and saved service views

- Status: [x]
- Slug: `deliver-workspace-hierarchy-and-saved-service-views`
- Dependencies: item 3
- Delivery type: first usable read-only UI

#### Objective

Present one Workboard Workspace containing many repositories/services and make cross-repository hierarchy navigation fast, durable, keyboard-first, and honest.

#### Deliverables

1. Add workspace, hierarchy, and saved-view feature slices with their own API modules, query-key factories, raw query/mutation hooks, facade hooks, components, pages, schemas, and exhaustive presentation tables.
2. Add TanStack Router routes for Workspace, repository, Epic, Feature, Work item, and saved view. Route search parameters are zod-validated.
3. Render breadcrumbs, searchable hierarchy navigation, recent/focused entities, repository participation, cross-repository Feature scope, deep links, and empty/missing/incompatible states using stable IDs/keys rather than filesystem paths.
4. Add daemon-owned `BoardViewDefinition` persistence with ID, Workspace, title, filters, grouping/lanes, sort, density, and revision. Repository/service filters remain views over one Workspace.
5. Keep unsaved filter/layout changes in a private Zustand feature store exposed through a facade hook. Do not copy server hierarchy or saved views into Zustand.
6. Use React Query for every daemon read/write. Ordered events invalidate or patch only the affected canonical query.
7. If saved-view mutation is unavailable, render the daemon-provided read-only reason and retain the unsaved local view without pretending it was saved.

#### Verification

- Rust tests prove one Workspace can contain 100 repositories and cross-repository Epics/Features without duplicate hierarchy identities or databases.
- Vitest Node covers query keys, route/search validation, request shaping, store actions, and exhaustive owner/repository discriminants.
- Browser components cover mouse/keyboard navigation, focus restoration, deep links, empty/missing/stale/disconnected/incompatible states, 200% zoom, high contrast, reduced motion, and screen-reader landmarks.
- Event tests prove one hierarchy change updates only affected queries.
- An architecture fixture proves no repository filter creates another Workspace/database and no UI path becomes hierarchy authority.

#### Completion gate

A user can traverse the complete hierarchy and save per-service views without fragmenting Workboard authority. This is the first Desktop milestone eligible for dogfood.

#### Rollback

Saved views are additive daemon-owned preferences. The default unsaved hierarchy remains usable if persistence is disabled.

### 5. Deliver the large board, dependency readiness, and What needs me

- Status: [x]
- Slug: `deliver-large-board-dependencies-and-attention-view`
- Dependencies: item 4
- Delivery type: scalable read-only delivery UI

#### Objective

Provide a responsive Kanban-like board and an authoritative queue of work requiring human attention.

#### Deliverables

1. Add paged/cursor `board.query` projections for card summaries, authoritative status, dependency readiness, blocked-by evidence, parallel readiness, repository scope, session summary, attention reasons, revisions, and available actions.
2. Add `attention.query`; the daemon owns reason codes and ordering for approvals, revision requests, reconciliation, blockers, checkpoints, interrupted operations, recovery conflicts, and stale/unknown session evidence.
3. Implement virtualized configurable lanes with `@tanstack/react-virtual`, stable deterministic sort, search, filters, selection, detail routing, and a full non-drag keyboard path.
4. Preserve focused content and accessible position/count through virtualization and event updates.
5. Apply scoped event invalidation/patching so a one-card update does not refetch or rerender the entire board.
6. Add a deterministic fixture with 100 repositories, 1,000 Features, 10,000 Work items, dependency edges, mixed providers, and attention states.

#### Verification

- Rust projection tests cover dependency DAG/readiness, parallel groups, attention classification/order, repository filters, pagination, and revisions.
- Vitest Node covers lane definitions, sorting, query invalidation, selection, and absence of duplicated workflow derivation.
- Browser components cover loading/empty/stale/disconnected/resync/partial/error states, keyboard lanes/cards, focus, accessible position/count, zoom, high contrast, and reduced motion.
- Large-fixture tests assert bounded mounted nodes and scoped render/query work. Record a release performance trace without a brittle CI timing quota.
- Cross-repository cards retain one Work-item identity while showing every participating repository.

#### Completion gate

The large mixed-repository fixture remains responsive and What needs me is a daemon-owned auditable projection, not a React heuristic.

#### Rollback

Fall back to scoped refreshed queries or explicit incompatible/read-only state. Never infer a board locally when the projection version is unavailable.

### 6. Deliver repository, checkout, worktree, and session observability

- Status: [x]
- Slug: `deliver-repository-checkout-and-session-observability`
- Dependencies: item 4
- Delivery type: authoritative read-only operational detail

#### Objective

Expose repository/checkouts and bound provider state, including uncertainty and recovery evidence, without direct filesystem, Git, transcript, or process access.

#### Deliverables

1. Add repository, checkout, session, and recovery-preview projections with current/historical paths, remotes/default-branch evidence, Feature integration and Work-item effective checkouts, inherited/override purpose, branch/head/generation, availability, dirty/collision/reconciliation evidence, and revisions.
2. Show each bound session's Workboard ID, provider, role, authoritative profile/model where available, binding/live/restore state, last activity, checkout, resumability, primary-writer evidence, and safe diagnostics.
3. Render Active, Idle, Stopped, Unknown, SystemError, and NotLoaded distinctly. Missing evidence never becomes Stopped.
4. Add repository, checkout, and session feature slices and deep links from hierarchy/board cards.
5. Keep resource-open and recovery controls unavailable until their typed daemon operations are accepted.
6. Ensure provider-native identifiers remain internal unless a reviewed display contract exposes a redacted support value.

#### Verification

- Rust projection tests cover current/historical/missing/replaced checkouts, many repositories, zero/one/many sessions, mixed providers/roles/profiles, stale evidence, already-live, unresumable, and recovery conflicts.
- Browser components cover dense/narrow layouts, keyboard selection, accessible evidence, stale/disconnected banners, and independent panel retry.
- Event tests prove liveness and checkout changes update only affected cards/details.
- Security tests prove React has no filesystem, Git, process, transcript, or credential API.
- Fake providers prove direct native sessions remain unmanaged and are never bound by cwd/PID inference.

#### Completion gate

Desktop truthfully represents repository, checkout, recovery, and bound-session evidence while remaining incapable of discovering or mutating it independently.

#### Rollback

Disable the observability routes independently; no checkout/session state is stored in Desktop.

### 7. Deliver Feature proposal detail and approval queues

- Status: [x]
- Slug: `deliver-feature-proposal-and-approval-queues`
- Dependencies: items 4 and 5
- Mutation gate: satisfied for Approve and Request revision; Reject remains gated

Approve and publish, and Request revision, execute from Desktop as of
`9ceb03a`. Both commit through `write_projected`, so the workflow transition,
the Workspace revision and the client event share one SQLite transaction.
Publication does planning-store Git I/O outside that transaction and reports
failure as a partial outcome rather than success. Availability is derived from
workflow state, so a proposal that is not awaiting approval reports why.

Reject stays unavailable with `terminal_rejection_unavailable`. It currently
shares one transition with Request revision; making it a terminal cancel
changes CLI-visible semantics and needs its own decision.

#### Objective

Make pending Feature proposals reviewable and, only through accepted policy, actionable without recreating publication transitions in React.

#### Deliverables

1. Add proposal/approval queue projections with generation/revision/hash, proposed Work items, dependencies, repositories, verification gates, warnings, planner sessions, diagnostics, current workflow state, and available actions.
2. Render long proposal detail safely with escaped text or sanitized owned Markdown; no executable links/content.
3. Include approval items in What needs me using daemon-owned attention reasons.
4. Begin read-only. Add Approve and publish, Request revision, and Reject only when individually advertised by the daemon with authoritative order, disabled reason, expected revision, feedback/confirmation requirements, and preview token where required.
5. Submit parsed zod form results through facade hooks. Render operation progress, hook warnings, partial publication, reconciliation owner, stale revision, and safe retry using the returned correlation/idempotency evidence.
6. Never call compatibility CLI commands or map workflow state to a local action menu.

#### Verification

- Rust contract tests prove proposal/queue completeness, revision/hash evidence, planner binding, diagnostics, and action ordering.
- Read-only tests prove controls are absent before capability acceptance and no alternate storage/CLI path exists.
- Browser components cover long/hostile content, cross-repository scope, changed proposal, zero/one/many planners, keyboard review, focus restoration, feedback/confirmation, warning, progress, partial failure, reconciliation, and retry.
- Fake-daemon routed tests prove exact advertised actions, idempotency, stale rejection, and no optimistic planned state.

#### Completion gate

A user can review complete proposals and perform only the exact accepted actions without Desktop becoming a publication authority.

#### Rollback

Disable mutation capabilities and retain read-only proposal/queue views. Daemon reconciliation remains usable elsewhere.

### 8. Deliver Work-item detail and gated structured checkpoints

- Status: [x]
- Slug: `deliver-work-item-detail-and-structured-checkpoints`
- Dependencies: items 4 and 6
- Mutation gate: accepted revision-checked atomic structured checkpoint operation

#### Objective

Present complete durable Work-item state and add editing only when Workboard exposes a structured checkpoint contract.

#### Deliverables

1. Add `work_item.detail` with outcome/design summary, current state, dependency readiness, blockers, decisions, verification, next action, review/delivery state, status, repositories/checkouts, revision/hash, checkpoint history, sessions, diagnostics, and available actions.
2. Add deep links from board, hierarchy, attention, Feature, repository, checkout, and session views.
3. Start read-only while the current opaque summary checkpoint remains the only operation.
4. When accepted, add focused editors for current-state update, blockers, decisions, verification, next action, review/delivery, and terminal intent.
5. Controlled buffers validate through zod at submit; facade hooks send only parsed changed typed fields with expected revision and idempotency key.
6. Render returned authoritative projection/actions. Do not edit Markdown, concatenate summaries, or synthesize status transitions.

#### Verification

- Rust tests cover durable sections, revisions/hashes, external edit conflict, idempotency, atomic database/planning-store outcomes, restart recovery, and no dual write.
- Capability tests prove editors remain absent against the opaque checkpoint contract.
- Vitest Node covers schemas, changed-field request mapping, and error mapping.
- Browser components cover section navigation, long/untrusted content, blockers, evidence, stale revision, inline validation, partial/reconciliation state, keyboard form flow, announcements, and focus restoration.
- Integration tests prove one accepted checkpoint updates CLI/TUI/Desktop identically and a failed commit never claims success.

#### Completion gate

Work-item recovery knowledge is visible, and any edit is an atomic typed Workboard checkpoint rather than client-owned document/state.

#### Rollback

Disable editors while retaining read-only detail. Accepted checkpoints remain canonical Workboard history.

### 9. Deliver gated zero/one/many Claude and Codex session controls

- Status: [ ]
- Slug: `deliver-zero-one-many-session-controls`
- Dependencies: items 5, 6, and 8
- Mutation gate: blocked on schema reconciliation with Frictionless, not on capability acceptance

The gate wording was investigated on 2026-09-01. The named operations are not
absent: `Feature/Frictionless-Managed-Work-Item-Launch` is at schema 41 and
owns `feature-proposal-decision` (35), `session-binding-generation` (36),
`session-follow-up` (37), `writer-session-reservation` (38),
`managed-launch-batch` (39), `feature-branch-integration` (40) and
`feature-work-item-proposal` (41).

The live Workspace database has already been migrated to schema 41 by that
branch. This branch is at 34 and fails to open it with
`schema migration 32 checksum mismatch`, so Desktop on this branch cannot read
the real Workspace at all. This branch's own migration 35
(`proposal-revision-request`) also collides with the live 35.

Reconciliation was deferred by explicit instruction on 2026-09-01: the Desktop
increment is being completed first. Item 9 therefore proceeds on this branch for
the operations this branch can actually back, and Desktop remains unable to open
the live Workspace until the schemas are reconciled.

Delivered here: Start and Resume execute against the existing checkout and
launch services, and the Work-item view renders the zero, one and many
cardinalities with deterministic ordering, exact session selection, an
already-live session shown disabled rather than duplicated, and a repository
choice only where the Work item targets more than one. Work-item
`available_actions` are derived from observed session evidence rather than a
fixed list.

Not delivered here, because this branch has no backing application capability:
OS window focus, follow-up delivery and its receipt evidence, second-writer
fan-out onto an isolated checkout, provider-profile selection, and a user-actor
structured checkpoint (`workflow_operations::checkpoint` authenticates a
managed-session workflow token, which a human Desktop client does not hold).
Each renders its own typed reason rather than being hidden. Recovery is
implementable here and is the next item on this branch.

#### Objective

Control authoritative bound native sessions while preserving checkout isolation, provider profiles, recovery, and managed-only capability injection.

#### Deliverables

1. Consume daemon-owned AvailableActions, SessionChoice, checkout/fan-out, lifecycle, follow-up, recovery, and provider-profile projections.
2. Zero sessions shows Start or its typed blocker. One shows exact Resume and Start another. Many shows deterministic current/live/activity ordering, exact selection, per-session Resume/focus, and Work-item Start another.
3. Dialogs show only advertised provider, role, access mode, model/reasoning profile, checkout plan, readiness blockers, and confirmation.
4. Default to one primary writer. Another writer requires explicit confirmation and an isolated checkout returned by Workboard.
5. Resume targets the exact Workboard session. Already-live focuses/reports rather than duplicating. Follow-up displays queued/delivered/failed receipt evidence. Recovery always previews before execute and preserves child partial outcomes.
6. The daemon remains the only component constructing provider commands/environments and injecting role-scoped Workboard skills, hooks, MCP configuration, and workflow token.
7. Direct Claude/Codex sessions receive no Workboard capability merely because Desktop is installed or running.

#### Verification

- Rust tests cover zero/one/many, mixed provider/role/profile, already-live, stopped, unknown, unresumable, historical, primary/secondary writer, blocker, isolated checkout, batch partial, follow-up receipt, recovery conflict, and restart.
- Fake terminal/provider tests prove exact cwd/profile/role, no duplicate live launch, isolated additional writer, FIFO follow-up, and truthful partial outcomes.
- Managed-boundary tests prove only Workboard-launched sessions receive authorized scoped integration.
- Browser components cover keyboard/dialog/focus flow, status, confirmations, disabled reasons, progress, partial failure, retry, narrow layout, and all cardinalities.
- Packaged Windows tests start/resume/focus/follow-up/recover fake Claude and Codex sessions and prove credentials never reach React or logs.

#### Completion gate

Desktop controls zero/one/many sessions solely through authoritative typed operations and never widens managed integration to direct provider sessions.

#### Rollback

Disable command capabilities to return Desktop to read-only. Daemon recovery/idempotency evidence remains usable from compatible clients.

### 10. Harden, package, and accept the Windows-first Desktop client

- Status: [ ]
- Slug: `harden-package-and-accept-desktop-client`
- Dependencies: items 5, 7, 8, and 9
- Delivery type: cross-cutting release qualification

#### Objective

Complete security, accessibility, performance, compatibility, packaging, rollback, and installed lifecycle acceptance.

#### Deliverables

1. Audit the Tauri capability/app-manifest/isolation/CSP/dependency/IPC threat model and redact sensitive logs/diagnostics.
2. Complete keyboard navigation, screen-reader semantics/live regions, visible focus, high contrast, 200% zoom, reduced motion, non-color status, and accessible virtualization.
3. Complete connection, stale, disconnected, resync, incompatibility, reconciliation, operation progress, partial outcome, diagnostics, and retry UX.
4. Run deterministic Rust, TypeScript, browser component, Tauri integration, and Windows WebDriver suites with isolated fake database, planning Git repository, providers, and terminal.
5. Exercise the 100-repository/1,000-Feature/10,000-Work-item fixture, scoped update assertions, compatibility skew matrix, and channel reconnect/leak soak.
6. Build a per-user Windows NSIS installer containing compatible Desktop and Workboard CLI/daemon components. Use a supported WebView2 installation policy.
7. Verify clean install, upgrade, repair, uninstall, spaces/non-ASCII paths, daemon singleton/restart, Start-menu launch, CLI/TUI recovery, offline/error messaging, signing inputs, and preservation of SQLite/planning store/checkouts/sessions.
8. Keep OS launch/reveal/notification/packaging behind platform adapters. Prove Linux/macOS compilation plus shared protocol/component tests; their packaging remains later work.
9. Dogfood the installed application against the real Concertable Workspace, exercising Claude and Codex read-only evidence and every enabled typed action.
10. Record final acceptance and rollback evidence in Workboard if its migration exit gate is met; otherwise update this plan in the delivery commit.

#### Verification

- `scripts/Test.ps1` runs Rust fmt/clippy/tests plus frontend typecheck, unit/component, generation, and Tauri integration gates.
- Packaged Windows tests cover hierarchy, views, board, attention, proposals, Work-item detail, zero/one/many sessions, controls, profiles, follow-up, recovery, partial failure, restart/resync, upgrade, repair, and direct-provider isolation.
- Security review proves least privilege, strict CSP/local content, isolation interception, secret redaction, hostile display safety, and no SQL/filesystem/shell/provider bridge.
- Accessibility review covers automated scans and manual keyboard, screen-reader, high-contrast, zoom, and reduced-motion scenarios.
- Performance evidence shows bounded rendering/scoped updates and meets the documented release interaction budget.
- Rollback rehearsal removes/disables Desktop or reinstalls the previous compatible pair while preserving all user work data and CLI/TUI access.

#### Completion gate

The installed Windows Desktop is secure, accessible, responsive, compatible, recoverable, and accepted as a client of the single Workboard authority, with shared-contract compilation on Linux/macOS.

#### Rollback

The installer never owns user work data. Uninstall preserves durable state. A capability kill switch returns Desktop to read-only; protocol mismatch fails closed; package rollback restores the previous compatible Desktop/daemon pair.

### 11. Rebuild the Desktop information architecture

- Status: [x] — landed as `3067fd6`. Delivered functionally but off-standard; item 12 brought it onto
  `react-standards`.
- Slug: `rebuild-desktop-information-architecture`
- Dependencies: items 4, 5, 7, and 8
- Delivery type: navigation and layout correction

#### Objective

Items 4-6 delivered navigation as a search box over flat lists of every entity kind. Replace it with a
persistent structural shell so a user reaches work by containment rather than by recall.

#### Decisions

These were specified on 2026-09-01 and are not reopened by this plan:

1. A persistent left sidebar lists repositories, expanding to Epics, then Features, and survives navigation.
2. Each level offers per-level view switching. A repository offers a board and a list of its Features.
3. A Feature opens a kanban of its Work items with `Detail` and `Proposal` as tabs on the same page.
4. A Work item opens a full page with `repository > Epic > Feature > Work item` breadcrumbs, not a drawer.
5. Lanes are Work-item status: Backlog, Ready, In progress, Blocked, Review, Done. Cancelled is hidden
   behind a filter.
6. Dependency readiness renders as a card badge, never as lanes. `WorkItemStatus::Blocked` is stored by a
   checkpoint; `DependencyReadiness::Blocked` is derived from the dependency DAG. Both can differ on one card.

#### Deliverables

1. Add `feature_ids` to `BoardQuery`, honour it in `board_projection.rs`, and regenerate contracts and
   conformance fixtures. Repository scoping already exists through `repository_ids`.
2. Add a navigation feature slice owning the tree model, its expansion store, and the sidebar. Expansion is
   unsaved client state behind a facade hook; the tree is derived from the authoritative hierarchy read.
3. Give the Workspace, repository, Epic, and Feature pages scoped boards and view switching through
   zod-validated route search parameters.
4. Replace the embedded hierarchy search on every entity page with the sidebar and a containment breadcrumb
   trail.
5. Render lane order and Cancelled visibility, and dependency-readiness badges, through exhaustive tables
   keyed by the published discriminants.

#### Verification

- Rust tests prove Feature-scoped board filtering and its interaction with repository and status filters.
- Vitest Node covers the tree model, the breadcrumb trail, and the exhaustive lane and readiness tables
  against the published discriminants.
- Browser components cover sidebar expansion, view switching, Feature tabs, and the scoped board.
- The installed Windows application is attached over CDP and its DOM read before the item is claimed done.

#### Completion gate

A user navigates repository to Epic to Feature to Work item without a search box, and every board is scoped
to the level it is shown at.

#### Rollback

The sidebar and scoped boards are presentation over existing authoritative reads. Removing them restores the
prior routes without protocol change.

### 12. Adopt shadcn and bring the frontend onto `react-standards`

- Status: [x] — landed as `83a3f5f`
- Slug: `adopt-shadcn-and-react-standards`
- Dependencies: item 11
- Delivery type: presentation-layer remediation

#### Objective

Item 11 shipped the information architecture without the styling and structure rules in
`Frontend rules` above. This item pays that back and makes the rules mechanical where they can be.

#### Delivered

1. `clsx` + `tailwind-merge` behind one `cn()` in `src/lib/utils.ts`; `class-variance-authority` for
   every visual variant; `lucide-react` as the single icon set; `dayjs` behind `src/lib/dates.ts`.
2. The palette moved onto shadcn token names in `styles.css`, keeping the existing dark and light
   values and the `forced-colors` / `prefers-reduced-motion` blocks, mapped through `@theme inline`.
   No component uses an arbitrary `var(--…)` utility.
3. Owned primitives in `src/components/ui/`: `button`, `card`, `badge`, `alert`, `retry-alert`,
   `input`, `textarea`, `label`, `checkbox`, `radio`, `select`, `collapsible`, `nav-tabs`.
4. Page derivation lifted into facade hooks over pure functions in `hierarchy/model/overview.ts`.
5. `HierarchyModel` / `HierarchyEntityModel` deleted. `core/contracts.ts` re-exports domain names over
   the quarantined generated module and `architecture.test.ts` fails the build if a feature names a
   generated `…Projection` or `ResponseEnvelope` type again.
6. Per-level view lists derived from `satisfies`-checked label tables, so a new view member breaks the
   build.
7. The proposal revision-feedback form parses through
   `proposal/schemas/requestFeatureRevisionSchema.ts` in a facade hook.

#### Decisions

1. **`select` and `radio` wrap the native controls, not Radix.** The platform control is already
   correct in a WebView2 shell and Radix would trade that for a portal and a scroll lock. They are
   still owned primitives with `cva` styling, which is what the standard asks for.
2. **The routed view switchers stay a `nav` of typed links, not Radix `Tabs`.** They change the URL,
   and the ARIA practices guide treats a set of separate pages as links rather than tabs. Making them
   tabs would have weakened the roles the browser suite asserts rather than updated them.
3. **`scroll-area` and `dialog` were not generated.** Nothing uses a dialog, and a Radix scroll area
   would fight `@tanstack/react-virtual` for the lane scroll element.
4. **`http-layer` does not apply.** There is no HTTP. The app reaches the daemon over Tauri IPC through
   the single owned `core/daemon.ts` facade, which is this app's analogue of the one-instance rule. Do
   not "fix" this by adding axios.
5. **`data-tables` does not apply yet.** The Feature index and Work-item lists are lists, not grids of
   comparable rows. A genuinely comparable-rows grid would be TanStack Table behind one `DataTable`.
6. **File-based routing is accepted debt.** `src/routes/router.tsx` still declares all thirteen routes
   with `createRoute`. The rule's substance is already met — each URL is declared once and every
   navigation is a typed `Link` or `navigate` — and converting would add a build-time route-tree
   generator and a second generated artifact to keep in sync beside the contract fixtures. `AppLayout`
   was extracted to `src/app/AppLayout.tsx` behind a pathless layout route, which was the part of the
   rule that was actually being violated.

#### Defects found and fixed

- The hierarchy pages tested "unavailable" before "missing", so a deep link to a Feature that no longer
  exists reported a broken daemon instead of a missing entity.
- The browser tier ran without Tailwind — the plugin was never in `vitest.browser.config.ts` — so every
  utility class was inert and the tier proved nothing about the CSS/layout behavior it owns. Enabling it
  exposed a real reflow bug: at 200% zoom the fixed-width sidebar drove `main` to zero width. The
  sidebar is now capped at 45%.

#### Verification

Full gate green: `cargo fmt --all --check`, `cargo clippy --workspace --all-targets --all-features -D
warnings`, `cargo test --workspace --all-targets` (265 tests), `npm run generate:check`,
`npm run typecheck`, 47 Vitest Node tests, 22 browser tests, `npm run build`.

## Feature completion

This Feature is complete only when:

- all twelve items are checked and their exact-head gates passed;
- CLI, TUI, and Desktop consume the same accepted client protocol for migrated operations;
- the daemon/application is the only runtime and persistence authority;
- Desktop cannot obtain or manufacture Workboard/provider authority;
- read-only behavior remains useful when mutation capabilities are missing;
- Windows installed acceptance and rollback are proven;
- remaining plan state is migrated to Workboard under the exit criteria, or this plan records why bootstrap authority still applies.

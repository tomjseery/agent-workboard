# Agent Workboard v0 plan

## Outcome

Create a new independent open-source Rust repository named `agent-workboard` whose installed executable is
`workboard`. It will be a thin local control plane around native Claude Code and Codex CLI sessions: it turns
high-level Epic roadmaps into AI-authored Feature plans and Work items, creates or restores the correct Git
worktrees before substantive work starts, launches the real provider CLI in Windows Terminal, binds the
native session to its work from birth, and makes every plan, checkout, and session easy to find and resume.

The product is not a renamed conversation catalogue, an embedded agent chat application, a clone of a full
project-management suite, or a passive plan organiser. Its defining workflow is:

```text
Epic intent
  -> managed native planning agent
  -> approved Feature Markdown and Work items
  -> correct Feature worktree
  -> managed native execution/review agents
  -> searchable board and exact resume
  -> safe terminal-layout recovery after interruption
```

The existing `context-catalogue` repository remains intact as the provenance and salvage source. The new
repository imports only proven Rust boundaries and their tests. It starts with a fresh public identity,
domain, schema, command surface, documentation set, and terminal-first interaction model instead of carrying
the old Tauri/React product and accumulated catalogue-first design.

## The problem this product owns

Concertable currently uses roadmap, plan, progress, handoff, and worktree skills to compensate for missing
session and checkout ownership. A roadmap-selection conversation may begin in the normal repository, create
or describe a delivery worktree later, and finish by telling another agent to change directory and follow a
Markdown `## Next Steps` section. The native conversation is still recorded against the checkout in which it
was born, so locating the right conversation, proving which checkout owns it, and reopening several pieces of
work after a restart require manual reconstruction.

This creates the recurring failures Agent Workboard must remove:

- the planning CLI starts in the base checkout before its delivery identity and worktree exist;
- a plan describes a future worktree rather than the active session already running inside it;
- native sessions are searchable by provider or path but not by the Epic, Feature, or Work item they own;
- several Claude and Codex sessions belonging to one piece of work are difficult to recognise and resume;
- deleted, replaced, moved, or sequential worktrees make older sessions appear detached from their work;
- `_PROGRESS.md` carries checkout, branch, session, handoff, and recovery narration that should be durable
  application state;
- repository-specific skills hard-code a generic planning and worktree lifecycle that should work for any
  configured project;
- closing terminals or restarting Windows loses the practical working set even though native sessions still
  exist;
- existing agent workboards tend to own the terminal or chat experience, while the required product must
  preserve ordinary PowerShell, Windows Terminal, Claude Code, and Codex CLI usage.

The product succeeds only when the managed path makes the wrong-checkout state structurally difficult: the
hierarchy, document location, effective checkout, and pending session association exist before the planner or
executor begins substantive reasoning.

## Product boundary and principles

- `workboard` owns work identity, planning workflow state, Markdown locations, Git checkout operations,
  launch intents, native-session associations, recovery membership, and terminal restoration.
- Claude Code and Codex remain native provider CLIs. Workboard launches and resumes them; it does not render
  their conversations or proxy normal interaction through a custom harness.
- AI creates the Feature plan. Workboard does not ask the user to fill a blank form or manually reproduce the
  current `/continue-roadmap` workflow after creating a database row.
- A generic provider-neutral planning contract supplies reasoning instructions. Typed Workboard CLI/MCP
  operations own durable mutations and validation. Skills never become the only source of workflow truth.
- The primary human interface is a Rust terminal UI running inside PowerShell or another terminal. It may
  resemble the clean native selectors used by Codex and Claude. PowerShell is the host, not the language in
  which the application UI or state machine is implemented.
- A future graphical client calls the same Rust application operations. It does not cause the CLI to become
  a shell wrapper or establish a second source of workflow rules.
- Markdown stays visible, editable, Git-versioned, and portable. SQLite stores operational identity and
  projections; it is not an opaque replacement for the plan documents.
- Every externally mutating operation is previewable, idempotent where retries are possible, and recoverable
  when Git, filesystem, database, provider, process, or terminal steps fail between boundaries.

## First public domain model

The hierarchy is intentionally first-class rather than represented only by optional labels:

```text
Workspace
├── Planning store repository
├── Code repositories
└── Epic
    └── Feature
        └── Work item
```

### Workspace

A Workspace is the local configuration and storage scope shown by the TUI. It maps one managed planning-store
Git repository to one or more code repositories. A simple project usually has one code repository, while an
Epic may eventually span several repositories without moving its planning identity.

### Repository

A Repository has a stable Workboard identity, current and historical local paths, Git common-directory
identity, remotes, default branch evidence, and worktrees. Repository identity never depends only on the
current filesystem path or remote URL.

### Epic

An Epic owns complete high-level Markdown, such as Concertable's `LAUNCH_ROADMAP.md`. Its document describes
the intended product outcome, ordering, dependencies, and outstanding Feature candidates. It is not reduced
to a title, label, or pointer to a file that Workboard does not understand.

Epic-level native sessions are allowed. A managed `epic continue` session is an explicit roadmap navigator,
not an orphaned base-checkout conversation. When it selects a Feature, Workboard launches the Feature planner
in the proper managed checkout.

### Feature

A Feature is the planned delivery capability beneath an Epic. It owns complete Feature Markdown containing
the approved design, constraints, phases, verification gates, and child Work-item definitions.

A Feature owns the default current delivery worktree for each participating code repository. Child Work
items inherit that effective checkout. A Work item can explicitly override the inherited checkout when it
must run independently or concurrently; the override is recorded rather than inferred.

### Work item

A Work item is one executable slice or durable Kanban card under a Feature. It owns complete Markdown with
its outcome, scoped design, current state, verification, decisions, and one next action. It can own many
planning, implementation, debugging, test, and review sessions across Claude and Codex.

Work-item status supplies the board columns. Labels are optional. `parent_id` remains explicit and validated:
Features parent Work items, Epics parent Features, and invalid cycles or hierarchy jumps are rejected.

### Native session

A native session is identified by provider plus the provider's immutable native session or thread ID.
Associations to Epic, Feature, or Work item are append-only intervals so reassignment and correction retain
history. The ordinary execution path associates sessions to Work items; Feature planning and Epic navigation
are deliberate higher-level roles.

### Checkout and association history

A checkout records repository identity, Git worktree identity, path interval, branch/ref evidence, creation
intent, current availability, and replacement history. Deleting a physical worktree never erases its prior
Feature, Work-item, or native-session associations. A restored or replacement checkout appends another
interval.

### Workflow and recovery records

Launch intents, workflow runs/events, document revisions, Git operation intents, live observations, leases,
managed-session recovery membership, and terminal-layout snapshots are application-owned durable facts. They
are separate from the Markdown body and can rebuild current projections.

## Canonical Markdown planning store

The default storage mode is a user-visible Git repository managed by Workboard and separate from the code
repository. It is selected during `workboard init` and can be created locally or linked to an existing remote.
It must never be hidden inside SQLite or an application cache directory with no ordinary Git access.

An illustrative layout is:

```text
workboard-store/
└── workspaces/
    └── concertable/
        ├── workspace.toml
        └── epics/
            └── launch/
                ├── EPIC.md
                └── features/
                    └── venue-availability/
                        ├── FEATURE.md
                        └── work-items/
                            ├── domain-model.md
                            ├── availability-api.md
                            └── availability-ui.md
```

Each document has stable machine-readable front matter and a complete human-readable body. Stable UUIDs and
slugs survive renames. The database indexes `(workspace, document ID, repository, path, observed Git commit,
content hash)` and detects external edits before executing stale instructions.

The launcher grants the managed native CLI explicit read/write access to the planning store as well as the
code worktree and passes the exact Epic, Feature, and Work-item document paths in its bootstrap context. The
provider adapter must use supported argument/configuration mechanisms; it must not weaken the provider's
filesystem policy globally.

Workboard creates local commits in the planning store at approved plan publication and material Work-item
checkpoints. It never pushes automatically by default. A failed commit or concurrent edit produces a typed
reconciliation state without pretending the database and Git repository agree.

An optional in-code-repository storage mode can follow after v0, but the domain and API must represent a
document by repository identity and path so either mode is possible without changing Epic, Feature, or Work
item semantics.

## Document lifecycle and removal of `_PROGRESS.md`

The external store uses one canonical document at each hierarchy level:

- `EPIC.md` is the living high-level roadmap;
- `FEATURE.md` is the approved Feature plan and child Work-item map;
- each Work-item Markdown is the durable execution and recovery document.

A Work-item document contains at least:

```markdown
---
id: <stable UUID>
key: launch/venue-availability/availability-api
status: in_progress
repositories:
  - concertable
---

# Availability API

## Outcome

## Design

## Current state

## Verification

## Decisions

## Next action
```

There is no separate `_PROGRESS.md` in the target product. Workboard owns the mechanical recovery facts that
currently bloat that ledger: worktree, branch, provider, native session ID, launch status, terminal restore
membership, and checkout replacement history. The Work-item Markdown retains only durable knowledge whose
loss could make the next agent take a wrong action or repeat costly investigation.

Existing Concertable `_PLAN.md` and `_PROGRESS.md` files remain untouched until import and parity are proven.
Migration converts their useful content into Feature and Work-item documents, verifies the result, and only
then removes the old generic ledger machinery in a normal Concertable change.

After accepted migration, Concertable contains no canonical Epic roadmap, Feature plan, Work-item plan, or
progress ledger. All executable planning Markdown moves to the configured Workboard planning store.
Concertable retains code, repository instructions, and genuine long-lived codebase documentation that is not
an Epic, Feature, Work item, plan, roadmap, or execution ledger.

## AI-authored Feature workflow

`workboard feature create` is an AI-assisted orchestration command, not CRUD. With sufficient arguments it
runs non-interactively until the native CLI opens; with missing arguments it opens scoped selectors.

The managed flow is:

1. Resolve or select the Workspace, code repository, Epic, provider, Feature title/request, base ref, and
   proposed worktree location.
2. Reject duplicate ownership by inspecting existing Features, Work items, documents, branches, worktrees,
   sessions, workflow runs, and provider evidence.
3. Create a durable draft Feature and workflow run in Workboard.
4. Create or reuse the Feature's code worktree before launching the planner. A backlog-only Feature may be
   lazy, but active AI planning always has an effective checkout.
5. Create a pending launch intent containing a random correlation token, intended provider, Feature,
   repository, workflow role, and exact checkout.
6. Open a native Claude or Codex CLI in Windows Terminal with that checkout as its initial working directory,
   the planning store explicitly available, and a generated bootstrap prompt.
7. Bind the first exact supported provider lifecycle observation to the launch intent and Feature. A missing,
   mismatched, or expired binding enters reconciliation; it never silently falls back to cwd inference.
8. The AI reads the complete Epic, repository instructions, relevant code, and existing Features. It asks the
   user necessary product questions and authors an implementation-ready `FEATURE.md` plus proposed child
   Work-item documents.
9. The AI submits the proposal through typed Workboard MCP/CLI operations. Prose, process exit, file creation,
   or provider idleness alone never means the plan was approved.
10. After the user approves the plan in the native conversation, Workboard validates hierarchy, document
    paths, Git head/hash preconditions, worktree ownership, Work-item uniqueness, phase gates, and provider
    binding; publishes and commits the documents; then materialises the Work items.
11. Workboard displays the planned Feature and offers to start the first Work item, choose another item, or
    return to the board.

The v0 default keeps the Feature planner as a Feature-associated session and launches a fresh Work-item
execution session. This preserves a reusable design conversation and gives execution an exact identity from
birth. The event model may later allow an explicit planner-to-Work-item promotion, but v0 does not depend on
changing a live process's checkout or reclassifying it implicitly.

## Skills, MCP, hooks, and provider integration

The CLI remains the authority regardless of how a workflow is invoked.

- `workboard integration install --tool claude|codex` installs previewable, reversible, product-owned hooks
  and the provider's generic Workboard planning instructions.
- `workboard mcp` exposes provider-neutral tools for reading the assigned hierarchy, submitting a Feature
  proposal, publishing approved documents, creating Work items, checkpointing a Work item, and requesting a
  new managed session.
- A small generic planning skill teaches the AI how to turn Epic intent into an implementation-ready Feature
  and Work items. It contains no Concertable paths, branch names, plan suffixes, or Git orchestration.
- Provider lifecycle hooks report exact native identity, cwd, process/lifecycle evidence, and launch token to
  Workboard. Hooks do not select roadmap scope, declare plan completion, or perform unrestricted mutations.
- The bootstrap prompt can carry the complete generic contract when a provider has no skill mechanism. The
  core product therefore does not fail merely because a skill was not installed.

The current `/continue-roadmap` behavior becomes a compatibility path rather than the primary implementation.
Preferred usage is `workboard epic continue` or `workboard feature create`. If `/continue-roadmap` is
invoked inside an unmanaged base-checkout conversation, its generic shim immediately hands the request to
Workboard and opens a new managed planner in the correct worktree; it does not create the substantive plan in
the already-wrong session.

Workboard launches native CLIs instead of calling Anthropic or OpenAI model APIs invisibly. Users retain the
normal provider authentication, model selection, permissions, transcript storage, commands, and interactive
experience.

## Terminal UI

Running `workboard` with no subcommand opens a Rust-native TUI inside the current PowerShell, Windows Terminal,
or supported terminal. No React, Node.js, WebView, or PowerShell UI script is required for v0.

The home screen provides:

- Workspace and repository selection;
- Epic and Feature navigation;
- a Kanban-like Work-item board grouped by Feature and status;
- alternate grouping or filtering by Epic, Feature, Work item, repository, provider, status, label, session
  activity, or recovery state;
- fuzzy search across titles, keys, document headings, repository names, native-session metadata, and recent
  user prompt previews within the configured privacy policy;
- visible warnings for missing worktrees, stale documents, failed launch binding, conflicts, or interrupted
  workflows.

Selecting a Work item shows its current document summary, effective and historical checkouts, branch/ref,
current status, next action, and every associated Claude or Codex session. The primary choices are **Resume
session**, **Start new session**, **Open document**, **Open worktree**, **Recover checkout**, and **Close Work
item**.

Selectors follow one consistent rule: when a command omits an identifier, Workboard opens a clean scoped
fuzzy picker rather than failing or requiring the user to copy a UUID. Exact IDs, stable keys, or unambiguous
search text remain available for scripts.

## Public command design

```text
workboard
    Open the terminal board.

workboard init [--store <path>]
    Create or connect a Workspace and its external Git-backed planning store.

workboard repository add [<path>]
    Register a code repository; with no path, open a repository selector.

workboard epic import [<markdown>]
    Import a complete existing roadmap document as an Epic.

workboard epic create [<title>]
    Create a new Epic document, using selectors or prompts for omitted values.

workboard epic continue [<epic>]
    Launch or resume a managed Epic-level roadmap navigator.

workboard feature create [<request>] [--epic <epic>] [--tool claude|codex]
    Create the draft Feature, prepare its worktree, and launch the AI planning agent.

workboard feature open [<feature>]
    Open Feature details; no identifier opens the Feature picker.

workboard work start [<work-item>] [--tool claude|codex]
    Select or resolve a Work item, prepare its effective checkout, and launch a new native agent.

workboard work open [<work-item>]
    Show its document, checkouts, status, next action, and associated sessions.

workboard session resume [<session>]
    Resume the selected exact native Claude or Codex session in its intended checkout.

workboard session adopt [<work-item>]
    Attach an externally launched current session through explicit provider evidence and confirmation.

workboard recover [--since <period>] [--dry-run]
    Preview and restore the previous managed terminal working set or a selected period such as yesterday.

workboard integration status|preview|install|repair|disable|remove
    Manage Workboard-owned provider integrations without overwriting unrelated configuration.

workboard import context-catalogue <database>
    Preview and import reusable repositories, native sessions, associations, and checkout history.

workboard import concertable <repository>
    Preview the Epic/Feature/Work-item conversion of existing roadmap, plan, and progress Markdown.
```

Every mutating command also exposes stable structured output and explicit non-interactive arguments for
automation. Human defaults optimise for names and search, not UUID memorisation.

## Work-item start, resume, and adoption

`workboard work start` without an argument opens the board's fuzzy Work-item picker. After selection it shows
the effective Feature worktree and existing sessions, then offers to resume one or start a new Claude/Codex
session. Starting a session creates and commits the pending association before opening the provider process.

`workboard session resume` performs native-source preflight, checks current provider live evidence, resolves
the intended current or historical checkout, acquires a duplicate-launch lease, and invokes the provider's
supported resume command with an exact argument vector. It does not construct a shell command from transcript
or document text.

Sessions created outside Workboard remain discoverable through the existing native adapters. Adoption
requires an explicit selected Work item and exact current provider identity where available. Inference may
suggest candidates but never overrides a managed or user-confirmed association.

## Managed recovery and Windows Terminal layout

Every Workboard-created CLI enters a durable restore set independent of whether its process is currently
running. Terminal closure, Workboard failure, provider exit, or computer restart changes live evidence but
does not erase intended workspace membership.

`workboard recover` defaults to the last saved active working set. `workboard recover --since yesterday`
selects relevant managed sessions active during that period. Before launch it presents a clean checklist and
an exact recovery plan.

The default Windows layout is:

- one Windows Terminal window per Feature;
- one tab per selected native session;
- tab title derived from Work item and provider;
- initial directory set to the session's effective worktree;
- existing live sessions skipped rather than duplicated;
- missing worktrees safely recreated or shown as explicit conflicts;
- completed, archived, removed-from-restore, or unresumable sessions excluded unless selected.

Workboard records logical grouping and launch intent rather than relying on opaque terminal process state.
Platform launchers receive argument vectors. Linux terminal restoration can use the existing portable edge
after the Windows workflow is accepted; v0 does not require an embedded multiplexer.

## Git and worktree model

- Active Feature planning and execution require an effective checkout before the native agent launches.
- A Feature owns the default worktree for its code repository. Child Work items inherit it.
- A Work item may own an override checkout when parallelism, repository boundaries, or isolation require it.
- Every create, restore, replace, close, or retire operation has a preview with repository identity, base ref,
  branch, path, collisions, dirtiness, and current-head preconditions.
- Git success and database failure, or database success and process failure, produce durable reconciliation
  records and idempotent repair choices rather than duplicate branches, worktrees, documents, or sessions.
- Merged or removed worktrees remain historical. A later slice can create a replacement from the current
  default branch without changing the Feature or Work-item identity.
- Workboard never recursively deletes a checkout without exact validated repository ownership and explicit
  scope.

## Fresh repository and salvage strategy

Create `C:\Users\TommySeery\source\repos\agent-workboard` as a completely new independent Git repository,
not a rename, fork, worktree, submodule, or continuation of `context-catalogue`. The old repository stays
readable throughout migration and provides exact source commit identities for selectively imported code.

The active legacy worktree at
`C:\Users\TommySeery\source\repos\context-catalogue.worktrees\Feature-context-catalogue_worktree-sessions-foundation`
contains a committed Phase 1 foundation and uncommitted Phase 2 launch/MCP work. Freeze it as a salvage
source; inventory and verify both the committed and working-tree states before transferring anything. Never
clean, reset, or delete it as part of bootstrapping.

Import code through reviewable commits grouped by subsystem, recording the source repository and commit in
each import commit message. Do not copy the old `.git` directory, product documentation, release identity,
React/Tauri application, Node dependencies, or entire schema blindly.

### Preserve and adapt

- provider-neutral conversation references, native IDs, live state, launch specs, safe terminal titles, and
  append-only association authority;
- bounded shared JSONL parsing;
- Claude native transcript adapter;
- Codex JSONL and read-only app-server adapter;
- Git common-directory/worktree discovery, historical path intervals, branch evidence, and checkout creation
  primitives;
- native resume preflight, shell-free command specs, Windows Terminal and portable terminal launchers,
  duplicate-launch leases, and process identity validation;
- Claude/Codex hook parsing, reversible integration preview/install/repair/remove, and configuration
  no-clobber behavior;
- daemon single-writer, authenticated local IPC, watcher, durable migration/backup/repair foundations;
- Phase 1 workflow identities/events/intents/document references/session bindings/restore membership and the
  useful uncommitted Phase 2 launch-correlation/MCP work after independent review;
- synthetic fixtures and tests proving each preserved boundary.

### Redesign or omit initially

- replace flat Workstream with the explicit Workspace/Repository/Epic/Feature/Work-item hierarchy;
- start a new Workboard schema and import legacy databases instead of carrying every old public name and
  migration into the new product;
- rewrite the catalogue-centred Clap surface around `workboard` commands and no-argument selectors;
- build a Rust TUI rather than transferring the React/Tauri desktop;
- replace repository-owned plan/progress assumptions with the external planning store and single Work-item
  document;
- defer GitHub pull-request discovery, updater/signing infrastructure, graphical privacy/settings surfaces,
  and full-transcript indexing until required by the accepted terminal workflow;
- retain bounded native prompt/title previews only where needed for session recognition and fuzzy search;
  preserve the existing privacy principle that full transcript indexing is explicit.

Suggested initial workspace:

```text
crates/
├── workboard-core
├── workboard-native
├── workboard-adapter-claude
├── workboard-adapter-codex
├── workboard-application
├── workboard-daemon
└── workboard-cli
```

The TUI belongs to `workboard-cli` or a presentation crate consuming `workboard-application`. Provider
adapters, application services, and storage never depend on terminal rendering.

## Architecture and state boundaries

```text
Human command/TUI
       │
       ▼
Workboard application service ───────────────┐
       │                                     │
       ├── Planning-store Git adapter        ├── Typed workflow/MCP operations
       ├── Code-repository Git adapter       ├── Provider integration manager
       ├── SQLite durable state              ├── Native lifecycle ingestion
       ├── Native session adapters           └── Terminal/process launcher
       │
       ▼
Claude Code or Codex CLI in the resolved worktree
```

The application service is the only business-operation boundary. CLI, TUI, MCP, hooks, daemon, and a future
graphical client submit typed commands and consume typed snapshots. TUI code never parses Markdown into
authority, constructs Git commands, or decides whether a session binding is safe.

Important workflow states include Draft, WorktreePending, PlanningLaunchPending, PlanningActive,
ProposalReady, AwaitingApproval, Publishing, Planned, WorkItemLaunchPending, WorkItemActive,
ReconciliationRequired, Blocked, Paused, Completed, and Cancelled. Append-only workflow events rebuild the
current state. External operations use intents and outcomes so interruption at any boundary is inspectable.

## Invariants and failure behavior

- A managed planning or execution session never launches before its hierarchy owner and effective checkout
  are durably resolved.
- A managed session never relies on cwd inference for its primary association.
- A native provider ID is never merged with another ID, even when prompts, paths, names, or timestamps match.
- Exact/user-confirmed associations outrank inference and remain historical after correction.
- Native transcripts and provider databases are read-only.
- Markdown or transcript content is never executed and never interpolated through a command shell.
- Worktree deletion, movement, merge, branch deletion, or checkout replacement never erases session history.
- A missing, mismatched, late, or duplicate launch observation fails into reconciliation rather than Inbox or
  a guessed association.
- Retries cannot create duplicate Features, Work items, documents, branches, worktrees, sessions, or commits.
- A live session is not resumed twice. Uncertain liveness requires explicit user choice.
- Planning-store external edits win over stale cached projections after validation.
- No automatic remote push, PR creation, provider credential storage, or cloud service is required.
- Diagnostic and export surfaces exclude transcript text and sensitive native identity by default.
- Existing user provider configuration is preserved byte-for-byte outside Workboard-owned fragments.

## Delivery phases

The planning estimate assumes focused reuse of the existing Rust foundations: approximately three to five
working days for a rough personal dogfood path, one to two focused weeks for the complete useful v0 workflow,
and two to three weeks for a credible open-source Windows release. These are sequencing ranges rather than
delivery promises; each phase replaces them with measured scope and verification evidence.

## Bootstrap next action

Inventory and hash the committed and uncommitted files in
`C:\Users\TommySeery\source\repos\context-catalogue.worktrees\Feature-context-catalogue_worktree-sessions-foundation`,
record the exact salvage map against Phase 1, then create
`C:\Users\TommySeery\source\repos\agent-workboard` with the Rust workspace and import the first verified
provider-neutral core/adapters/tests commit without changing, cleaning, or deleting the source worktree.

### Phase 1 — freeze, scaffold, and salvage the proven foundation

Status: complete.

- Inventory the committed and uncommitted active legacy worktree and produce an exact keep/redesign/drop map.
- Create the independent `agent-workboard` Git repository, Rust workspace, `workboard` binary, licence, readme,
  formatting/lint/test baseline, and Windows-first development scripts.
- Import the provider adapters, Git/native launch primitives, integration boundary, daemon/storage foundation,
  and their tests in reviewable provenance-labelled commits.
- Rename crates and public product identity without retaining old user-facing Context Catalogue or Worktree
  Sessions vocabulary.
- Keep the old repository and active worktree untouched after the import; record their exact source heads and
  dirty-file manifest.

Verification gate: the new workspace builds, formats, lints, and passes every transferred synthetic test;
the old active worktree has the same status and file hashes as before import; no Tauri/React/Node dependency
or old product executable is present.

Consumption contract: later phases consume provider-neutral adapters, Git and terminal services, integration
management, SQLite/daemon primitives, and typed Rust errors without depending on old Workstream projections.

### Phase 2 — establish the Workboard domain and external planning store

Status: complete.

- Implement Workspace, Repository, Epic, Feature, Work item, Markdown document, checkout history, native
  session association, workflow run/event, operation intent, launch intent, restore membership, and terminal
  layout identities.
- Create a fresh checked migration chain and rebuildable projections with hierarchy constraints, append-only
  association intervals, Feature checkout inheritance, Work-item overrides, and idempotency.
- Implement `workboard init`, planning-store creation/linking, repository registration, Epic create/import,
  document front matter, content-hash reconciliation, local commit policy, and backup/export.
- Add a read-only legacy Context Catalogue database importer preview without mutating the source database.

Verification gate: fixtures round-trip the entire hierarchy and Markdown store; invalid parentage/cycles,
concurrent edits, interrupted publication, path traversal, duplicate keys, moved repositories, and deleted or
replacement worktrees fail deterministically without losing history.

Consumption contract: callers receive one typed Workspace snapshot and submit idempotent commands; document
bodies remain in Git while SQLite exposes identity, status, path, hash, and operational projections.

### Phase 3 — ship the terminal board and searchable selectors

Status: complete.

- Make bare `workboard` open the Rust TUI.
- Add Workspace/repository navigation, Epic/Feature hierarchy, Feature-grouped Kanban, Work-item details,
  associated session lists, current/historical checkout display, warnings, and keyboard-first fuzzy search.
- Make every optional command identifier follow the exact-ID/unambiguous-query/picker fallback rule.
- Add scriptable structured output alongside human presentation without leaking TUI decisions into the
  application service.

Verification gate: terminal fixtures cover empty, single-match, ambiguous, large catalogue, historical,
missing-checkout, interrupted-workflow, and no-colour/noninteractive states; keyboard navigation remains
usable in PowerShell and Windows Terminal.

Consumption contract: the TUI consumes typed snapshots and returns selected stable IDs; later graphical UI
can consume the same snapshots and operations without parsing terminal output.

### Phase 4 — launch and bind new native sessions from birth

Status: complete.

- Implement Claude and Codex new-session launch capabilities in addition to existing exact resume.
- Complete launch-token propagation, supported hook/app-server correlation, process identity, expiry,
  duplicate protection, reconciliation health, and managed restore membership.
- Implement Feature worktree create/reuse and Work-item inherited/override checkout resolution before launch.
- Implement `workboard work start`, `work open`, `session resume`, and `session adopt` with fuzzy selectors.
- Prove normal managed launches never need cwd inference or manual Inbox assignment.

Verification gate: isolated native homes and fake terminals cover both providers, new and resumed sessions,
missing hooks, wrong/expired tokens, wrong cwd, PID reuse, launch crash, duplicate resume, user cancellation,
and database/process interruption.

Consumption contract: a successful launch returns a confirmed `(hierarchy owner, provider, native ID,
checkout, workflow role, restore membership)` binding; failure returns one repairable typed state.

### Phase 5 — deliver AI-authored Feature planning

Status: complete.

- Implement `workboard feature create` and `epic continue` preflight, selectors, Feature worktree preparation,
  planner launch, generic bootstrap prompt, proposal workflow, approval, validation, document publication,
  planning-store commit, and Work-item materialisation.
- Implement `workboard mcp` and matching CLI operations for hierarchy reads, proposals, publication,
  checkpoints, and managed session requests.
- Generate/install provider-specific thin planning skills from one versioned provider-neutral contract.
- Preserve an unmanaged `/continue-roadmap` compatibility shim that hands off immediately to the managed
  workflow instead of planning in the wrong checkout.
- Offer to start the first Work item in a fresh native agent after plan publication.

Verification gate: disposable repositories complete Epic import → Feature planning → approval → Feature and
Work-item Markdown → first execution launch with Claude and Codex; rejected plans, changed Epic/code heads,
invalid documents, failed Git commits, missing integration, and retries create no duplicates or false success.

Consumption contract: the approved result is a Git commit containing one complete Feature document and its
Work-item documents plus confirmed application identities and a selectable first Work item.

### Phase 6 — recover managed working sets

Status: complete.

- Persist logical Windows Terminal grouping for every managed session and active work set.
- Implement `workboard recover`, `--since yesterday`, interactive selection, dry-run, one-Feature-per-window,
  one-session-per-tab launch plans, duplicate skipping, and remove-from-restore behavior.
- Restore present checkouts, safely recreate missing checkouts, and expose conflicts for dirty, unreachable,
  colliding, unresumable, or provider-incompatible entries.
- Preserve older sessions on historical worktrees and allow a confirmed new session to replace an
  unresumable native session without losing Work-item history.

Verification gate: restart fixtures cover terminal closure, daemon crash, computer restart, already-live
sessions, deleted/moved/replaced worktrees, missing branches, stale layouts, partial launches, and retry after
every external boundary.

Consumption contract: recovery emits a complete preview and either restores each selected native session in
the intended checkout exactly once or returns a per-entry typed conflict with no silent omission.

### Phase 7 — import Concertable and retire progress-ledger orchestration

Status: implementation and review complete; external parity acceptance pending.

- Import Concertable roadmaps as Epics, existing plans as candidate Features, plan phases as proposed Work
  items, and durable progress facts into the appropriate Work-item documents through an editable preview.
- Import exact legacy native sessions, repository/worktree history, and confirmed associations from the old
  catalogue; leave ambiguous records visible for adoption rather than guessing.
- Run the complete Feature-create, Work-item start/resume, multi-session detail, worktree replacement, and
  recovery workflows against Concertable and one unrelated repository with Claude and Codex.
- After parity acceptance, remove every migrated roadmap, `_PLAN.md`, `_PROGRESS.md`, planning folder,
  planning graph, and generic `/continue-roadmap`, resume/handoff/worktree implementation from Concertable.
  Preserve only repository-specific instructions and non-planning codebase knowledge.
- Keep rollback artifacts until the external planning store and imported database have verified backups.

Verification gate: every active Concertable plan and relevant session has exactly one reviewed destination;
no native transcript is modified; Workboard can recover the accepted daily working set after a real Windows
restart; removing old ledgers does not remove any fact required to choose the correct next action.

Consumption contract: Concertable consumes only the installed Workboard integration, its code, and its own
repository-specific instructions; every canonical Epic/Feature/Work-item and executable planning document
lives in the external store, and native CLIs remain ordinary PowerShell/Windows Terminal processes.

### Phase 8 — harden the open-source Windows release

- Add concise installation, upgrade, uninstall, integration-permission, storage, backup, recovery, and
  troubleshooting documentation.
- Restore only the release packaging, compatibility gates, fuzzing, SBOM, provenance, and diagnostics needed
  by the terminal product; do not restore the old graphical updater merely for parity.
- Build an unsigned Windows dogfood candidate, use it for several days, fix workflow friction, and freeze the
  v0 command/schema/document contracts only after owner acceptance.
- Define later Linux packaging and graphical-client entries without delaying the accepted Windows CLI/TUI.

Verification gate: clean-machine installation and removal preserve unrelated provider configuration;
upstream Claude/Codex compatibility checks pass; the release can create, plan, execute, resume, adopt, and
recover work without the source checkout or Node.js.

Consumption contract: users receive the `workboard` executable and optional background daemon/integrations;
repositories require no copied generic workflow implementation.

## Explicit v0 exclusions

- No embedded Claude/Codex chat or terminal emulator.
- No Tauri, React, browser, mobile, or cloud UI.
- No SaaS account, hosted database, team collaboration, remote scheduler, or autonomous worker farm.
- No automatic push, pull request, merge, or remote-provider mutation.
- No attempt to replace GitHub Issues, Jira, Linear, or full project-management software.
- No hard dependency on GitHub PR discovery, full transcript indexing, SQLCipher, signing identity, or
  cross-platform installers for the first dogfood slice.
- No deletion of the old repository, active legacy worktree, or Concertable planning corpus before import
  and parity evidence.

## End-to-end acceptance scenarios

### Create a Feature from an existing roadmap

1. Run `workboard feature create --epic launch 'Venue availability'` from any directory.
2. Workboard resolves Concertable and creates the Feature worktree before launching the planner.
3. A native Claude or Codex window opens in that worktree and is immediately visible under the Feature.
4. The agent reads the full Launch Epic and Concertable code, collaborates with the user, and publishes the
   approved Feature and Work-item Markdown to the external store.
5. Workboard offers the first Work item and launches its native agent in the inherited Feature worktree.

### Find and resume work without remembering an ID

1. Run `workboard work start` with no argument.
2. Fuzzy-search the terminal board by Feature, Work item, repository, provider, or recent phrase.
3. Select a Work item and see all associated Claude and Codex sessions.
4. Resume the chosen exact native session in its intended checkout or start a new managed session.

### Recover yesterday after a restart

1. Run `workboard recover --since yesterday`.
2. Review the selected Features, Work items, sessions, and any required checkout recreation.
3. Confirm once.
4. Workboard opens one Windows Terminal window per Feature and one correctly titled tab per safe session,
   skips already-live processes, and reports conflicts individually.

### Use the old skill entry safely

1. Invoke the generic `/continue-roadmap` compatibility skill from an unmanaged base session.
2. The skill identifies the configured Workspace and hands the request to Workboard.
3. Workboard creates the Feature/worktree and opens a managed planner there.
4. The original session remains an explicit origin or Epic-level session and never becomes the hidden owner
   of delivery work.

### Remove Concertable planning and recovery duplication

1. Import and verify every roadmap, plan, progress ledger, Epic, Feature, Work item, session, and historical
   worktree.
2. Resume several old and new sessions entirely through Workboard.
3. Delete the migrated roadmaps, `_PLAN.md` files, `_PROGRESS.md` files, planning folders, and generic
   repository-specific orchestration only after acceptance.
4. Confirm that every canonical Epic, Feature, Work item, and executable plan now exists in the external
   Workboard store rather than Concertable.
5. A fresh agent finds its Work item, current document, correct checkout, native sessions, and next action
   without a hand-written `cd` or continuation prompt.

## Exit condition

The v0 plan is terminal when a clean Windows installation of `workboard` can configure an external
Git-backed planning store, import or create an Epic, ask a native Claude or Codex agent to create and publish
a Feature plan, materialise Work items, launch and exactly bind execution sessions in the correct inherited
or overridden worktree, search and resume those sessions through a Rust terminal board, recover the previous
working set into safe Windows Terminal windows/tabs, adopt legacy sessions, preserve complete checkout
history, move every canonical Concertable roadmap/plan/progress artifact into the Workboard planning store,
and remove Concertable's generic planning/session-recovery machinery without losing durable product
knowledge.

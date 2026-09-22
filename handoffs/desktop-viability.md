# Desktop: make Agent Workboard genuinely usable beside the backend cutover

Work in this exact checkout and continue autonomously through a complete, verified, committed implementation:

`C:\Users\TommySeery\source\repos\agent-workboard.worktrees\desktop-board-session-control-ui`

The product goal is a daily-usable Windows desktop Workboard for Concertable, developed alongside a separate backend Codex tab that is implementing the structured human/managed Work-item state mutation needed to retire `_PROGRESS.md`.

Current evidence to verify rather than assume:

- This branch already contains the Tauri/React desktop, persistent repository → Epic → Feature navigation, scoped Kanban boards, Work-item detail, proposal approval, and zero/one/many session start/resume controls.
- It already contains the Frictionless backend branch through commit `956ffe7` and is ahead of it.
- The installed application loop exists in `apps/workboard-desktop/scripts/install-desktop.ps1`.
- Work-item detail remains intentionally read-only for structured state: the daemon advertises `structured_checkpoint_unavailable`, and browser tests assert that no checkpoint/save/complete button exists.
- Recovery, focus/follow-up/profile/fan-out integration, final installed acceptance, and general usability still need honest completion or explicit gating.

Outcome:

Make the desktop application usable as the ordinary human control surface while retaining the daemon/application as the only workflow and persistence authority. A user must be able to understand what is happening, find work, operate sessions, and—once the backend contract arrives—manually update durable Work-item state without touching SQLite or planning Markdown directly.

Work in two stages:

1. Immediately finish and verify UI work that does not require the new backend contract: recovery preview/execute, zero/one/many session controls, disabled reasons, error/reconciliation UX, navigation, board density/scrolling, keyboard behavior, narrow-window behavior, and the installed edit-to-see-it loop. Exercise the real installed application, not only component tests. Fix concrete usability defects you find.
2. The backend tab is working in `C:\Users\TommySeery\source\repos\agent-workboard.worktrees\Feature-Frictionless-Managed-Work-Item-Launch`. Do not edit that checkout or invent its contract in TypeScript. Once it has clean committed structured-state work beyond `956ffe7`, merge those backend commits/branch into this branch, regenerate the daemon/client/TypeScript contracts through the repository's normal generator, and implement the human editors against the exact accepted operation.

Required desktop behavior after backend integration:

- Obvious manual controls for editing current state, concrete next action, blockers, decisions, verification, status, and terminal intent.
- Revision-checked forms with clear validation, stale-state handling, pending/success/failure feedback, and authoritative refresh after mutation.
- Status changes available from the Work-item page and an efficient board interaction; no fake drag/drop or optimistic move before daemon success.
- Work-item creation/edit entry points should be understandable. If manual creation lacks a backend contract, surface the exact remaining gate and hand it back rather than implementing a client-owned workaround.
- Start, Resume, Start another, follow-up, focus/status, recovery, close, and any unavailable action must show the authoritative reason and never be decorative text.
- The app must remain useful when a capability is unavailable, but an implemented capability must not stay hidden behind a stale read-only gate.

Scope boundary with the backend tab:

- You own daemon/client protocol exposure after consuming the backend commit, Tauri, generated TypeScript, React UI, desktop tests, packaging/install loop, and installed UX acceptance.
- Do not independently design or implement a competing storage/application checkpoint operation.
- Avoid broad release hardening that does not affect the daily Concertable path until the core edit/control loop is accepted.

Read this checkout's `AGENTS.md`, `plans/PLANS.md`, the desktop plan, and applicable React skills before editing. Reconcile the plan honestly: item 8 cannot remain described as fully delivered if its required editors are absent. Preserve user work. Follow the repository rule of zero code comments unless a line-level invariant genuinely cannot be expressed in code. Run the relevant Rust, generated-contract, TypeScript, browser, Tauri, and installed-app gates, and commit at natural boundaries without asking.

Completion means Tommy can install the current desktop build, navigate Concertable, select or create work, manually update its durable state/status, start or resume the correct Claude/Codex session, recover interrupted work, and see the authoritative result without using a repository `_PROGRESS.md` ledger or hand-editing Workboard storage.

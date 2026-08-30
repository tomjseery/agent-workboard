# Agent Workboard instructions

## Planning

`plans/` is the repository's planning source of truth.

- Use one `<EPIC>_ROADMAP.md` for an Epic and one `<NAME>_PLAN.md` for each buildable delivery item.
- Do not create `_PROGRESS.md` files. Keep current status, decisions, verification, blockers, and the next
  action in the matching `_PLAN.md`.
- Workboard Features and Work items remain execution records: assignment, dependency state, sessions,
  checkpoints, and completion. They are not a second prose planning system. Translate their design and phase
  content into the relevant repository plan.
- Before implementation, read the relevant roadmap and plan. Keep the plan current at material boundaries,
  including changed acceptance criteria, completed phases, verification, blockers, and the next delivery gate.
- Use stable Workboard keys in plan phases when work is represented in Workboard, but do not duplicate a
  separate plan in Workboard Markdown.

## Verification

For Rust changes, run:

- `cargo fmt --all -- --check`
- `cargo clippy --workspace --all-targets -- -D warnings`
- `cargo test --workspace`

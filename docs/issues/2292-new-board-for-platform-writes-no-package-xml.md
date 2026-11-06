---
id: 2292
title: "`nros new board <name> --for-platform <p>` writes no `package.xml`, so the board catalog refuses the board it just scaffolded"
status: open
type: bug
area: cli, boards, scaffold
related: [rfc-0064, rfc-0049, phase-290, phase-375, phase-537, 2291, 2293]
found: 2026-11-06
---

**Home: none yet.** Reported by the phase-537 W12 book session with an
in-tree CLI.

## Symptom

`nros new board acme --for-platform acmeos` reports `board package `acme`
scaffolded`. Every catalog reader then refuses the board, because its
`nros-board.toml` has no `package.xml` beside it that announces a board.

## Code site

There are two board scaffolders, and only one of them writes the file the
catalog requires:

- `packages/cli/nros-cli-core/src/cmd/new_platform.rs`, `scaffold_board()`.
  `nros new` reaches it at `cmd/new.rs` (the `keyword == Some("board")`
  arm). It writes `Cargo.toml`, `src/lib.rs` and `nros-board.toml`, and no
  `package.xml`.
- `packages/cli/nros-cli-core/src/cmd/board_new.rs` (`nros board new`)
  writes `nros-board.toml` and a `package.xml` from
  `render_package_xml()`, with the `kind="board"` announcement.

The refusal is RFC-0064 R5 D5, enforced in
`orchestration/board_descriptor.rs`: a descriptor must sit beside a
`package.xml` that announces a board. That rule is correct. The
`--for-platform` scaffold predates it (phase-290 W4.b) and was never
updated. The two scaffolders also disagree on the descriptor template, so
the same request gives different packages depending on which verb the user
found.

## Fix direction

- One board scaffolder. `nros new board --for-platform` should call the
  `board_new` code (or both should call a shared renderer). It should never
  keep a second template.
- The acceptance is that a board from either spelling resolves through the
  catalog (`nros board info <name>` with the package on the board search
  path). A test that runs that is what would have caught this.
- `nros new platform` (`scaffold_platform`, same file) has a matching gap.
  Issue 2293 covers it.

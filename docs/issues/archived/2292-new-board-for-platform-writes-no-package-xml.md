---
id: 2292
title: "`nros new board <name> --for-platform <p>` writes no `package.xml`, so the board catalog refuses the board it just scaffolded"
status: resolved
type: bug
area: cli, boards, scaffold
related: [rfc-0064, rfc-0049, phase-290, phase-375, phase-537, 2291, 2293]
found: 2026-11-06
resolved: 2026-11-07
resolved_in: "fix-issue-2291-2292 branch: one board scaffolder"
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

## Resolution

There is one board scaffolder. `new_platform::scaffold_board` no longer has a
template: it calls `board_new::run` with `out_of_tree = <into>`, so
`nros new board <name> --for-platform <p>` writes exactly the package
`nros board new <name> --platform <p> --out-of-tree <into>` writes
(`nros-board.toml` and the `package.xml` announcement) and prints the same next
steps. The old crate skeleton (`Cargo.toml`, `src/lib.rs`) is gone with it. A
board package is not a crate (`nros-board-freertos-posix` has neither file), and
the old descriptor's `board_crate` would have pointed at
`packages/boards/nros-board-<name>`, a path that does not exist for an
out-of-tree board.

`nros new` has no Zephyr board id flag, so `--for-platform zephyr` is now
refused and the refusal names `nros board new <name> --platform zephyr
--west-board <board>/<soc>/<variant> --out-of-tree <dir>`. The Zephyr guide's
line is updated to that spelling, and the Extending overview now lists
`package.xml` among what the board scaffold writes.

Tests in `packages/cli/nros-cli/tests/board_new_scaffold.rs`:
`new_board_for_platform_resolves_through_the_catalog` (the acceptance: the
board resolves with `nros board info` on `$NROS_EXTRA_BOARD_PATH`),
`both_board_scaffold_spellings_write_the_same_package` (byte-equal
`nros-board.toml` and `package.xml`), and
`new_board_for_zephyr_names_the_verb_that_takes_a_board_id`.

Routing `nros new board` through `board_new` put its last next-step line,
`just check fast`, on a user road that never printed it. That line is now
printed on the in-tree arm only, where the recipe exists and is the
contributor's acceptance; the out-of-tree road (both spellings) prints no
contributor step. `the_out_of_tree_next_steps_name_no_contributor_step`
asserts it for both spellings. A user-tier check verb to print there instead
is issue 2294's open question.

### Not fixed here

The acceptance holds for a board on a platform the catalog knows. A board for
a NEW platform, the `nros new platform acmeos` then
`nros new board acme --for-platform acmeos` road the Extending overview
describes, is still refused by every catalog reader, on both spellings:

```
unknown variant `acmeos`, expected one of `posix`, `freertos`, `bare-metal`,
`nuttx`, `zephyr`, `threadx-linux`, `threadx-riscv64`, `stm32`
```

The descriptor's `platform` deserializes into the closed `PlatformKind` enum
(`orchestration/board_descriptor.rs`), so a porter's platform needs a core edit
before any board can name it. That contradicts RFC-0049's "zero central-file
edits" porter contract, and it needs its own issue: opening the set touches
every `PlatformKind` match (feature, family, driver road), which is a design
decision, not a scaffold fix.

---
id: 1815
title: "`log-arena-probe` names no board crate, so #1305's sync refusal stops every native fixture build"
status: resolved
type: bug
area: [testing, cli, build]
severity: high
found: 2026-10-11
resolved: 2026-10-12
related: [1305, 1037, phase-479]
---

## Measured

`just native build-fixture-rust` on `origin/main` (`f061565539`) in the ros2
distrobox stops at the row-codegen pass, before any native fixture is built:

```
→ (row codegen) packages/testing/nros-tests/bins/log-arena-probe
Error: …/packages/testing/nros-tests/bins/log-arena-probe: image `native` builds for
board `native`, whose entry needs the crate `nros-board-linux`, but
…/log-arena-probe/Cargo.toml depends on no board crate.
  Replace the board dependency in …/Cargo.toml with `nros-board-linux` (issue 1305 …)
Location:
    nros-cli-core/src/cmd/leaf_settings.rs:406:5
```

The same pass runs inside `build-test-fixtures`, so every lane that builds
native fixtures stops there too. Found while running phase 481's Zephyr tier 2:
the native peers of the seven native↔Zephyr interop tests could not be built.

## Cause (read)

`9317222d57` (fix #1305, 2026-10-11) refuses a single-package leaf whose board
crate disagrees with its image's board. `log-arena-probe` (phase-479 W5,
reshaped by #1037) states `[image.native] board = "native"` but deliberately
links no board crate: it takes the POSIX `nros_platform_*` symbols from
`nros-platform-cffi` (`posix-c-port`) and uses only `nros-log` and the boot
record from `nros-node`. "No board crate" is read as "the wrong board crate",
and the leaf is refused. #1305's sweep did not cover this leaf.

## What would close it

Either give the probe the board crate its image names (`nros-board-linux`),
and confirm `tests/log_arena_knob.rs` still reads the rung it was built to read,
since the board crate carries the `[board.knobs.log]` rung the probe measures; or
have #1305's check tell a leaf with NO board crate apart from one with the
WRONG one, if a board-less native leaf is a supported shape. Then
`just native build-fixture-rust` runs past this row.

**Workaround (local only):** adding
`nros-board-linux = { path = "../../../../boards/nros-board-linux" }` to the
probe's `Cargo.toml` let the native build finish. Not measured: whether the
probe still reports the right rung with that dependency.

## Resolution (2026-10-12)

The second option: a board-less leaf IS a supported shape. `log-arena-probe`
has its own `main` and takes the POSIX platform through `nros-platform-cffi`,
so it was never written for some OTHER board, which is the stale state the
1305 check exists to catch. `check_board_crate_dependency` now returns early
when the leaf names no `nros-board-*` crate, and still refuses one that names
a different board crate than its image needs. The probe's manifest is
unchanged, so the rung `tests/log_arena_knob.rs` measures is untouched.

Why 1305's own sweep missed it: `every_in_tree_rust_leaf_names_the_board_crate_its_image_needs`
walked `examples/*/system.toml`, while the check also runs in the fixture
build's row-codegen pass over `packages/testing/`. The sweep now walks every
tracked `system.toml` (the 0196 shape: a gate narrower than its rule).

Measured:

- `nros sync` in `packages/testing/nros-tests/bins/log-arena-probe` with the
  rebuilt CLI: rc=0, `sync: done.` (before: the refusal quoted above).
- Mutation: deleting the early return fails both the widened sweep (naming
  `log-arena-probe`) and the new `a_leaf_naming_no_board_crate_is_accepted`;
  restored, all three `board_crate_tests` pass.

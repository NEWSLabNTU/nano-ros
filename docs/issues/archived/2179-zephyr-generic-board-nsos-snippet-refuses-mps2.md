---
id: 2179
title: "The generic `zephyr` board's `nros-nsos` snippet reaches every mps2/an385 image of a leaf whose image says `board = \"zephyr\"`, and Kconfig refuses the build"
status: resolved
type: bug
area: [zephyr, boards, fixtures]
severity: medium
found: 2026-10-31
resolved: 2026-10-31
related: [2157, 2164]
---

## What

Issue 2157 put native_sim's NSOS in one snippet, `nros-nsos`, named by the
generic `zephyr` board (`packages/boards/zephyr/nros-board.toml`,
`[board.zephyr] snippets = ["nros-nsos"]`), and the module hook adds it to
`SNIPPET` for every rendered `system.toml` image
(`_nros_image_kconfig_merge_snippets`, `zephyr/cmake/nros_image_kconfig.cmake`).
The snippet is deliberately board-blind: on a non-native board Kconfig refuses
it, which 2157 measured on `mps2/an385` with a bare `west build -S`.

But the in-tree mps2/an385 fixture rows build leaves whose images say
`board = "zephyr"` -- e.g. `examples/zephyr/rust/talker` (`[image.zephyr_zenoh]
board = "zephyr"`) for the `talker-serial` row
(`west_build_name = "build-cortex-m-rust-talker-serial-zenoh"`, board
`mps2_an385`). So the hook adds `nros-nsos` to an mps2 build too.

Measured 2026-10-31, Zephyr 3.7, `west build --cmake-only -b mps2/an385` of
`examples/zephyr/rust/talker` with the row's `conf_files` on `origin/main`
(`1e4c8c7673` + later):

```
Merged configuration '.../cmake/zephyr/mps2-an385-serial.conf'
Merged configuration '.../zephyr/snippets/nros-nsos/nsos.conf'
...
nsos.conf:15: warning: attempt to assign the value 'y' to the undefined symbol NATIVE_SIM_SLOWDOWN_TO_REAL_TIME
error: Aborting due to Kconfig warnings
```

With the board's `snippets = []` (a local edit, not committed) the same
configure completes. Every mps2/an385 row over a leaf whose image names the
generic `zephyr` board is expected to fail the same way; only the serial row
was built.

## Direction

The board the image names is `zephyr` while the west BOARD is `mps2/an385`,
so either the snippet must follow the west board (a native-sim board package
names it, the generic one does not), or the mps2 rows must build an image
whose board is the mps2 package (`nros-board-mps2-an385-zephyr`). Whichever,
a fixture row that builds a non-native board must be in a lane that would
have caught this; check which lane builds `talker-serial` and why it stayed
green (or did not run) after 2157.

## Sighting on main: live-peer regression (2026-10-31)

live-peer regression run 41167433083 (schedule, `380cde09c6`), job
124438293461 `rows whose board is NOT this runner`, step `Build the fixtures
those rows resolve`, failed on the first Zephyr fixture,
`zephyr/mps2_an385/c/talker/zenoh` (`build-cortex-m-c-talker-zenoh`):

```
Merged configuration '/__w/nano-ros/nano-ros/zephyr/snippets/nros-nsos/nsos.conf'
nsos.conf:15: warning: attempt to assign the value 'y' to the undefined symbol NATIVE_SIM_SLOWDOWN_TO_REAL_TIME
error: Aborting due to Kconfig warnings
✗ Zephyr fixture build FAILED (exit 2)
```

So the defect also reaches the C leaves (`examples/zephyr/c/talker`, image
`zephyr_zenoh`, `board = "zephyr"`), not only the Rust serial row this issue
measured, and the board-elsewhere live-peer lane cannot build any fixture
until it is fixed.

## Resolution

Fixed 2026-10-31 by scoping a board package's Zephyr facts to the west board
they describe, not by editing the snippet or the rows.

- `BoardDescriptor::builds_west_board(west)`
  (`packages/cli/nros-cli-core/src/orchestration/board_descriptor.rs`): does
  this package describe a build given `-b <west>`? Compared by Zephyr board
  NAME in either spelling (`mps2/an385` = `mps2_an385`; `native_sim`,
  `native_sim/native/64`, `native_sim_64` all build the generic `zephyr`
  package).
- `nros ws leaf-system --west-board <BOARD>`
  (`packages/cli/nros-cli-core/src/cmd/leaf_system.rs`): when the image's
  board package does not describe that board, it contributes NEITHER its
  snippets (`NROS_LEAF_SNIPPET`) NOR its conf files (the issue-1782 head of
  `NROS_LEAF_CONF`) -- the same class, one predicate -- and prints
  `NROS_LEAF_BOARD_NOT_APPLIED=<its west board>`.
- The module hook (`zephyr/cmake/nros_image_kconfig.cmake`) passes the build's
  `BOARD` (read as `boards.cmake`'s `zephyr_check_cache` will: variable, env,
  `CACHED_BOARD`) and says when the package was not applied.
- The same hook now also moves Zephyr's `CACHED_SNIPPET` for the names it owns.
  `zephyr_check_cache(SNIPPET)` prefers that over `SNIPPET`, so a build dir
  that had configured once with `nros-nsos` kept re-reading it after the board
  stopped naming it, and stayed red until a pristine build.

Sweep: the `[board.zephyr]` fields the hook road reads are `snippets` and the
package conf files; both now go through the predicate. `rust_target`,
`rust_prereqs`, `rust_components` are read only by `nros build`'s preflight,
which passes `-b` from the same descriptor, as does its `-DNROS_BOARD_SNIPPETS`
(a hand-written entry's carrier), so neither can disagree with the build.
`zephyr` is the only in-tree package naming a snippet.

Evidence (box tree, Zephyr 3.7, `NROS_ZEPHYR_FIXTURE_FILTER=... just zephyr
build-fixtures`):

- before: `talker-serial` (mps2_an385) fails at Kconfig on
  `nsos.conf:15 ... NATIVE_SIM_SLOWDOWN_TO_REAL_TIME`;
- after: `talker-serial`, `talker` (mps2_an385, TCP) and the native_sim
  `talker` all build, and so does live-peer's `build-cortex-m-c-talker-zenoh`; native_sim still has `SNIPPET=nros-nsos` and
  `CONFIG_NET_NATIVE_OFFLOADED_SOCKETS=y`, mps2 has `SNIPPET` empty;
- negative control: the hook without `--west-board` fails the mps2 row again,
  same Kconfig error; restoring it builds that SAME dir incrementally, no
  pristine (the `CACHED_SNIPPET` half).

Unit: `a_board_package_builds_only_its_own_west_board`,
`the_generic_board_package_answers_only_for_native_sim`.

Which lanes saw it: live-peer's board-elsewhere job (the sighting above) did,
on its first Zephyr fixture -- that is the lane that caught it on main. Tier 2's
`just build tier2` also builds the mps2 west leaves, but every scheduled
`run-matrix` since 2157 landed stopped earlier, in `check fast` (run of
2026-10-31: "1 of 492 gate(s) FAILED"), so it never reached a Zephyr build.
No merge-gating lane builds a Zephyr image, which is by design (no SDK there);
the class is now one predicate with unit coverage on the merge lane, so no new
gate.

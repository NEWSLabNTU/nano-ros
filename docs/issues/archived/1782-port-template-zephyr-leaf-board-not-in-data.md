---
id: 1782
title: "`cpp-port-minimal-publisher/zephyr` names the generic `zephyr` board, which lowers to `native_sim`, so its build cannot be derived from its own data"
status: resolved
type: bug
area: [examples, tooling]
severity: low
found: 2026-10-10
related: [issue-1764, phase-482, issue-1296, issue-1308, phase-452, phase-518]
resolved: 2026-10-29
resolved_in: "#2821"
---

**Home: [phase-452](../roadmap/phase-452-copy-out-front-door.md)** — homed 2026-10-17; listed in that phase's "Issues homed here (2026-10-17)".

## What was measured

Issue 1764's fix makes `check-template-copy-out` build each template
SUB-PROJECT from a copy, by the road the sub-project's board declares. That
works for `cpp-port-minimal-publisher/mps2-an385-freertos`: its board
descriptor states `[board.cmake] toolchain_file`, and the copied leaf builds
with it (one ARM ELF, `build/minimal_publisher`).

It does not work for `cpp-port-minimal-publisher/zephyr`. Its `system.toml`
says `[image.zephyr] board = "zephyr"`, and the generic `zephyr` descriptor
lowers that to `west_board = "native_sim/native/64"`. `nros build --dry-run`
in the leaf prints `west build -b native_sim/native/64 …`.

Three builds of a copy of the template, against a Zephyr 3.7 workspace, with
`-DZEPHYR_EXTRA_MODULES=<this checkout>`:

| board | conf | result |
| --- | --- | --- |
| `native_sim/native/64` (what the data says) | Zephyr's default (`prj.conf`) | compile error: `conflicting types for 'pthread_t'`, `clockid_t`, `timer_t` (host libc against Zephyr's POSIX types) |
| `mps2_an385` | Zephyr's default (`prj.conf`) | compile error: `unknown type name 'pthread_t'` in `zenoh-pico/system/platform/zephyr.h` |
| `mps2/an385` | `prj.conf;prj-zenoh.conf;<nano-ros>/cmake/zephyr/mps2-an385.conf` (the README) | builds, `zephyr.elf` |

So the board the port targets and the Kconfig fragments it needs are stated
only in the template's README and in its fixture row (`examples/fixtures.toml`,
`west_build_name = "build-cortex-m-cpp-port-minimal-publisher-zenoh"`, whose
board tail `mps2-an385.conf` is added by `scripts/build/zephyr-fixture-leaves.sh`).
Neither is data a copy of the leaf carries. The README itself says
`native_sim` is not a target for this port, which contradicts the board its
`system.toml` names.

## Consequence

`check-template-copy-out` reports this sub-project NOT VERIFIED (through
`nros_check_unverified`) even on a host with Zephyr installed, naming this
issue. A user who copies the template and runs `nros build` in `zephyr/`
gets the `native_sim` build that fails.

## Direction

Make the leaf state its target. Two candidates, to be decided with a Zephyr
build of each:

1. `board = "mps2-an385-zephyr"` (descriptor exists, `west_board =
   "mps2_an385"`), plus the image's `conf = [...]` for `prj-zenoh.conf`. The
   QEMU network tail `cmake/zephyr/mps2-an385.conf` still has to come from
   somewhere; the board descriptor is the natural owner.
2. Keep `board = "zephyr"` and let an image state a concrete west board.

Either way, acceptance is: a copy of the template builds in `zephyr/` by a
road derived from its data, and `check-template-copy-out` builds it instead of
skipping it where Zephyr is installed.

## Also hit by a ported workspace (2026-10-26, phase-518 W4)

`examples/templates/ament-workspace-port/zephyr` and the ported ros2/demos
`composition` both need the same tail. The board package's own `prj.conf`
(`packages/boards/nros-board-mps2-an385-zephyr/prj.conf`) is comments only, so
`nros build` with `board = "mps2-an385-zephyr"` produced an image that failed to
link: `undefined reference to z_impl_sys_rand_get`, because nothing set
`CONFIG_TEST_RANDOM_GENERATOR`. The port built and ran once a copy of
`cmake/zephyr/mps2-an385.conf` was named in the image's `os_config`. This
confirms the direction above: the fragment belongs to the board package, where
`board_projection::board_files` already looks (`prj.conf`,
`boards/<flat>.conf`).

## Resolution (2026-10-29)

Direction 1, as the 2026-10-26 section confirmed.

- **The board package owns its Kconfig.** `cmake/zephyr/mps2-an385.conf` moved
  (one file, not a copy) to `packages/boards/nros-board-mps2-an385-zephyr/prj.conf`,
  replacing the comment-only `prj.conf` there. Every consumer of the old path
  (the fixture leaves script's board tail, the serial row in
  `examples/fixtures.toml` and `binaries/mod.rs`, the override ratchet's
  baseline, comments) names the new one.
- **A single-package leaf now gets its board package's files.**
  `board_projection::board_files` reached a generated workspace application
  (`nros build`, issue 1288) and a `nano_ros_use_board()` caller, but never a
  leaf. `nros ws leaf-system` now puts them at the head of `NROS_LEAF_CONF`
  (`board_conf_paths`), so the nano-ros module hook merges them ahead of the
  image's `os_config` — for `nros build` and for a plain
  `west build -- -DNROS_IMAGE=<id>` alike. A bringup, a board without
  `[board.zephyr]`, and an app that calls `nano_ros_use_board()` get nothing
  added. A build that already names the file (the fixture lanes, in
  `CONF_FILE`) keeps its own order: the hook skips a fragment the build names.
- **The four port templates** (`cpp-port-minimal-publisher`,
  `rclcpp-compat-smoke`, `topic-state-monitor-port`, `ament-workspace-port`,
  all `zephyr/`) say `board = "mps2-an385-zephyr"` and
  `os_config = ["prj-zenoh.conf"]`; their READMEs and `prj.conf` headers no
  longer spell a tail.
- **`check-template-copy-out`** builds a Zephyr sub-project with `nros sync`
  + `nros build` in the copied leaf instead of reporting it NOT VERIFIED.
  That surfaced a defect the blanket skip had hidden: a copy of
  `zephyr-byo/app` did not compile (issue 2140, since resolved), so that one
  project was NOT VERIFIED by name (`KNOWN_UNBUILDABLE`) until 2140 closed.

Measured: `cp -r examples/templates/cpp-port-minimal-publisher` to a scratch
directory outside the checkout, then `nros build --zephyr-workspace
~/.nros/workspaces/zephyr/3.7` in its `zephyr/`: exit 0, `west build -b
mps2_an385`, merged `prj.conf` → the board package's `prj.conf` →
`prj-zenoh.conf` → the image rendering, and
`build/zephyr-zenoh/minimal_publisher__zephyr_west/zephyr/zephyr.elf` is a
32-bit ARM ELF with `CONFIG_TEST_RANDOM_GENERATOR=y`, `CONFIG_ETH_SMSC911X=y`
and `CONFIG_MAIN_STACK_SIZE=131072`.


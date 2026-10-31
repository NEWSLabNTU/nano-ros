---
id: 2187
title: "Zephyr 4.4 refused every mps2 row and three conf files that 3.7 accepted through deprecated aliases"
status: resolved
resolved_in: "2026-11-01"
type: bug
area: [zephyr, boards, build]
severity: high
found: 2026-11-01
related: [2157, 2182, 0432, 2188, 2189]
---

## What was measured

A sweep of every Zephyr fixture row on v4.4.0
(`NROS_ZEPHYR_VERSION=4.4 just build zephyr`, then the zephyr test binaries).
Before this, only two rows had ever been built on 4.4 (issue 2157). Of 65 rows,
36 built. The rest failed in four ways, none of them reachable on 3.7:

| Rows | Failure on 4.4 | Why 3.7 accepted it |
|---|---|---|
| 10 mps2 rows | `No board named 'mps2_an385' found` | `boards/deprecated.cmake` maps `mps2_an385` to `mps2/an385`; 4.4 dropped the alias |
| `build-logging-smoke` | `prj.conf:26`: undefined symbol `PTHREAD_IPC` | `[DEPRECATED]` alias that selects `POSIX_THREADS` |
| `build-ws-rs-realtime{,-derived}-entry-zenoh` | `prj.conf:62`: undefined symbol `SCHED_DUMB` | 4.4 renamed it `SCHED_SIMPLE` |
| `build-cortex-m-rust-talker{,-serial}-zenoh` | `zephyr` crate: 4 x E0061, `GpioPin::new` takes 6 arguments, 5 supplied | the issue-0432 gpio patch was applied on 3.7 only |

## Cause

Each of these was a 3.7 deprecation that 4.4 completed. 3.7 tolerated every
one of them, so nothing failed until a 4.4 build ran.

- The board name lived in the fixture rows (`board = "mps2_an385"`), the mps2
  board descriptor's `west_board`, and four scripts keyed on the string.
- `scripts/zephyr/patches/4.4.sh` skipped the gpio pair as part of the
  "legacy 3.7 Rust patches". But 4.4's `zephyr-lang-rust` pin (a763400) still
  emits a one-cell `GpioPin::new` call for mps2's `#gpio-cells = <1>`
  controller, and the same hunks apply to it unchanged.

## Resolution

- `mps2_an385` → `mps2/an385` in the 10 rows, the descriptor and the four
  scripts. `BoardDescriptor::builds_west_board` had matched any `<name>_…`
  spelling. It now matches the same board in either spelling, the board with
  trailing qualifiers omitted, or the legacy `<name>_<last qualifier>` alias,
  so `mps2/an521` no longer matches a package whose board is `mps2/an385`.
- `PTHREAD_IPC` → `POSIX_THREADS`, which both lines define. The
  `SCHED_DUMB` line is removed: it is each line's default scheduler, and
  `SCHED_CPU_MASK` depends on that default.
- The fvp board's `CONFIG_MAX_PTHREAD_COUNT=16` is removed and not replaced.
  On 3.7 it is a `[DEPRECATED]` alias that `pthread.c` never reads (its pool is
  `posix_thread_pool[CONFIG_POSIX_THREAD_THREADS_MAX]`), so the line was dead
  there too. `zephyr/Kconfig` already defaults the real knob to 16 under either
  RMW. This line had been flipped between the two names twice before, by
  b447c4aee6 and 7b762e62c1 (the second for a downstream build with fatal
  warnings), so naming neither name ends the flip-flopping.
- `scripts/zephyr/patches/4.4.sh` applies the gpio patch (it is idempotent).

### Sweep

- Board names: every `west_board` in `packages/boards`, and every `board` in
  `examples/fixtures.toml`, checked against 3.7's `boards/deprecated.cmake` and
  4.4's `board.yml` names. `mps2_an385` was the only stale one.
- Kconfig: every `CONFIG_*=` in the 141 tracked `*.conf` files, checked against
  every `config`/`menuconfig` symbol in each line's workspace plus our own
  Kconfig. Only `MAX_PTHREAD_COUNT`, `PTHREAD_IPC` and `SCHED_DUMB` are defined
  on 3.7 and not on 4.4. No symbol is undefined on both lines.

No gate was added. Both checks need the two Zephyr workspaces, and no CI lane
checks out either of them.

## Measured

- Zephyr 4.4: all 64 non-SMP rows build. The zephyr test binaries
  (`zephyr`, `zephyr_cortex_m_qemu`, `zephyr_link_layout`,
  `zephyr_leaf_staleness`, `zephyr_prjconf_requirements`, `cli_bringup_zephyr`,
  `zephyr_self_pkg`, both ROS 2 interop binaries, and `entry_e2e`'s zephyr
  cells), with the Linux zenoh peers built: 76 run, 64 passed, 2 lane-skipped.
  The 12 failures are issue 2188 (10 Cyclone cases) and issue 2189 (2
  configure-only fixtures), neither of them touched here.
- Zephyr 3.7: the 10 renamed mps2 rows and the three rows whose conf changed
  all build.
- `a_board_package_builds_only_its_own_west_board` passes, including the new
  `mps2_an521` case.

## Not measured

- `build-ws-c-realtime-entry-smp` (qemu_cortex_a53): the 4.4 SDK (1.0.1)
  provisioning fetches only `arm-zephyr-eabi` and `x86_64-zephyr-elf`, so there
  is no `aarch64-zephyr-elf` to build it with. That is a provisioning choice in
  `[tool.zephyr-sdk-1-0-1]`, not a defect in the row.
- The fvp board was not built on either line, because its lane needs the FVP
  model and a host idlc. The pool size above comes from reading Kconfig.

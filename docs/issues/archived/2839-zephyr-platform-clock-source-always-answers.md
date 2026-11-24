---
id: 2839
title: "Zephyr's `platform` clock source always answered, with no wall clock behind it, so `epoch_failure = halt` could never fire on a list ending in it"
status: resolved
type: bug
severity: medium
area: [zephyr, clock, knobs]
related: [phase-582, issue-0758, issue-2780]
found: 2026-11-22
resolved: 2026-11-24
resolved_in: "Zephyr's `platform` source no longer answers without a wall clock (RFC-0141 D1 amended); the three Zephyr clock cells pass on 3.7 and 4.4"
---

Found by nros 175 validating phase-582 W1 on the ASI board (FVP, Zephyr).

## What happens

1. **`platform` answers on a port with no clock.** `nros_zephyr_clock_try`
   (`zephyr/nros_platform_zephyr_shims.c`) returned true for
   `NROS_WALL_CLOCK_SOURCE_PLATFORM`, though Zephyr has no wall clock. So
   `source = "sntp,platform"` with `epoch_failure = "halt"` could never
   halt. With no SNTP server it latched "nros: wall clock from platform"
   while `wall_clock_state().synced` was false and every stamp read 0, and an
   image whose peers refuse unstamped data ran anyway.
2. **The `epoch_failure` help contradicted the code.** It said `unstamped`
   gives "boot-relative stamps"; `clock_resolve.h`, `platform.h` and the
   resolver's own message say stamps read 0 (ROS "unset"), which is what the
   code does.
3. **A message advised a retired spelling.** The Zephyr heap-exhaustion
   report said "raise with `[image.<id>.knobs.zephyr] heap_size`", retired
   for `memory.heap_bytes`. The class: `check-retired-knob-spellings` read
   ENV and Kconfig spellings only, never the `toml:` road. Widened, it found a
   second one: a platform-config warning naming `knobs.zenoh.tx.split_lock`
   (retired for `zenoh.tx_split_lock`).

## Fix (manager ruling A, 2026-11-22)

- `platform` answers only on a port that HAS a wall clock. Zephyr's try now
  returns false. A list ending in `platform` on Zephyr therefore reaches
  `epoch_failure`: `unstamped` (the default) runs on exactly as before, with
  stamps 0, but now says "no wall-clock source answered (...)", and `halt`
  halts. A POSIX host's `platform` keeps answering from `CLOCK_REALTIME`.
- The `clock.source` and `clock.epoch_failure` help say so, and say stamps
  read 0.
- Both messages name the current spellings. `check-retired-knob-spellings`
  gains the TOML road in messages: a retired `toml:<group>.<name>` written in
  a C, C++ or Rust prose literal as `knobs.<group>] <name>`,
  `knobs.<group>.<name>` or `<group>.<name>`. It has 7 self-test cases, and
  the registry parse is a case too.

Cells:
- The two W1c-2 fallback cases in `tests/zephyr_clock_resolve.rs` now expect
  no latch and the no-source line.
- New `+ [ClockHalt]`: `zephyr_talker_clock_halt` (`sntp,platform`, `halt`,
  a loopback port with no server). It must print the halt line, latch
  nothing, exit, and publish nothing.

## Closes when

The three Zephyr cells pass on 3.7 and 4.4.

## Resolution (2026-11-24)

Closed on its condition. nros 241 ran the Zephyr clock cells
(`packages/testing/nros-tests/tests/zephyr_clock_resolve.rs`) at main
b493149c41. The native talker and the Zephyr fixture builds returned 0 on
both lines.

| Case | 3.7 | 4.4 |
| --- | --- | --- |
| `falls_back_after_the_settle_time` | PASS (8.7 s) | PASS (7.1 s) |
| `clock_halt_stops` | PASS (3.7 s) | PASS (2.9 s) |
| `latches_the_hlc` | PASS (6.7 s) | PASS (6.0 s) |
| `wait_is_not_replayed` | PASS | PASS (10.0 s) |
| `cases_bound_to_matrix_cells` | PASS | PASS |

The same file's two SNTP re-sync cases were an unmet precondition in that
run, not failures: the MessageInfo observer fixture was not built. They are
not this issue's cases. They belong to issue 2884's change and run with it.

The `clock_halt_stops` cell is also the acceptance that issue 2883 (resolved,
superseded by this fix) points to.


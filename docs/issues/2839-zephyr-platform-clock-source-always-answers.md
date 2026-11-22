---
id: 2839
title: "Zephyr's `platform` clock source always answered, with no wall clock behind it, so `epoch_failure = halt` could never fire on a list ending in it"
status: open
type: bug
severity: medium
area: [zephyr, clock, knobs]
related: [phase-582, issue-0758, issue-2780]
found: 2026-11-22
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

---
id: 1581
title: "realtime-rust's FreeRTOS and ThreadX images state no locator, so they dial the board default and every tiered test on them fails before a tier runs"
status: resolved
resolved: 2026-09-29
type: bug
area: [examples, testing, freertos, threadx]
severity: medium
found: 2026-09-29
related: [issue-1571, phase-383]
---

## What happens

In `examples/workspaces/realtime-rust/src/demo_bringup/system.toml`, the
`[image.freertos_realtime]` and `[image.threadx]` tables give no `locator`.
On `main` only `[image.zephyr]` states one (`tcp/10.0.2.2:7471`). Both images
therefore bake the board default, `tcp/192.0.3.1:7447`.

The tests start their router elsewhere: port 7891 on FreeRTOS, and
`tcp/127.0.0.1:9091` on threadx-linux. Every Rust tiered cell on those images
therefore fails with `Executor::open failed: ConnectionFailed` before any
tier code runs:

- `realtime_tiers` freertos/rust and threadx-linux/rust;
- `sched_dims_applied_e2e` threadx-linux/rust CorePin, PreemptThreshold and
  TimeSlice.

These were observed by the issue-1571 agent while verifying the Rust
tier-backing change. The FreeRTOS image booted fine by hand against a router
on 7447. The missing tables were confirmed by reading `main`.

## Cause

The locator was lost when phase-383 W10.a (`f1fc1202c`) migrated the
realtime workspaces. The comment at line ~182 of the same file notes that
"the locator was the deleted package's".

## Acceptance

- Each image table states the locator its test's router uses, or the tests
  read the image's baked locator rather than assuming one.
- The five cells above reach tier code.
- Check the sibling realtime-c and realtime-cpp bringups for the same gap.

## Resolution

**Source of truth.** The port is `nros_tests::alloc::port_of`; a `system.toml`
literal cannot call it, so the image STATES it and the test side VERIFIES the
statement — the pattern `orchestration_tiers_freertos.rs` already used. Which
carrier states it is decided by the board, not by taste: every board applies
the bringup's `[image.<id>] locator` (the `DeployOverlay` / `NROS_BOOT_CONFIG`
bake), and only boards whose crates `option_env!("NROS_LOCATOR")` also read a
row `env.NROS_LOCATOR`. FreeRTOS and threadx-linux read no env, so the image
table is the only carrier there.

**Fix.** The class was four images, not two. phase-383 W9 had already dropped
`examples/workspaces/rust`'s `freertos` and `threadx` locators the same way:

| bringup | image | locator | cell |
| --- | --- | --- | --- |
| realtime-rust | `freertos_realtime` | `tcp/192.0.3.1:7891` | FreertosMps2 / Rust / RealtimeTiers |
| realtime-rust | `threadx` | `tcp/127.0.0.1:9091` | ThreadxLinux / Rust / RealtimeTiers |
| rust | `freertos` | `tcp/192.0.3.1:7830` | FreertosMps2 / Rust / EntryPubsub |
| rust | `threadx` | `tcp/127.0.0.1:9030` | ThreadxLinux / Rust / EntryPubsub |

The threadx realtime row's `env.NROS_LOCATOR = tcp/127.0.0.1:9091` named the
right port and reached nothing on that board; it is removed. realtime-c and
realtime-cpp have no gap: their embedded rows are cmake (`NROS_ENTRY_LOCATOR`)
or west (runtime override). No runtime test consumes the two
`workspaces/rust` rows today, so their values are restored rather than
measured.

**Guards.**
- `just check image-locator-bake` (fast lane): every embedded cargo image row
  bakes a locator its board reads; a row env on a board that ignores it is
  refused; two carriers must agree. The reads-env table is verified against
  the board crates' sources. Over the pre-fix tree it reports exactly the four
  rows above.
- The four Rust RealtimeTiers resolvers assert their row's baked port equals
  `port_of(..., RealtimeTiers)` before the test starts its router
  (`nros_tests::fixtures::baked_locator`), and a static unit test checks all
  ten embedded RealtimeTiers rows with no fixture.

**Measured** (fixtures rebuilt from the fixed tree; both binaries contain
their stated locator):
`realtime_tiers` — freertos/rust and threadx-linux/rust RAN and passed (17
rows, 15 skipped for fixtures not built in this worktree);
`sched_dims_applied` — threadx-linux/rust CorePin (FALLBACK, the declared
arm), PreemptThreshold (ACCEPT) and TimeSlice (ACCEPT) RAN and passed.


---
id: 2825
title: "A test that waits longer than its nextest kill is killed mid-wait, and reports a bare TIMEOUT with no output"
status: open
type: test-gap
severity: medium
area: [testing, ci]
related: [issue-2105, issue-1013, phase-590]
found: 2026-11-22
---

## What

Tier-2 run 44916540238 (main cbf8c22e6f):
`zephyr_cortex_m_qemu::zephyr_cortex_m_cpp_zenoh_pubsub_e2e` reported
TIMEOUT at 60 s with no output captured.

## Why the output was empty

The cell waits for the boot's net line with
`qemu.collect_until(NET_STACK_READY_MARKER, BOOT_BUDGET)`, where
`BOOT_BUDGET` is 90 s, and prints what it collected only after that returns.
Its nextest kill was 60 s:

- `binary(zephyr)` in `.config/nextest.toml` is an EXACT binary name, so none
  of the 120 s `zephyr` rows reach `zephyr_cortex_m_qemu`.
- The binary's own group row (`zephyr-cortex-m-baked-port`) set no
  `slow-timeout`, on the stated grounds that its other cells "are cheap".
- So the default profile's 30 s x 2 = 60 s applied, against waits of 90 s
  (both pubsub cells and the idle-wake boot) and 90 + 60 s (idle-wake arrival).

Any boot slower than 60 s was killed mid-wait, before the test could print.
The verdict says neither what the guest printed nor why it was slow. Whatever
slowed or hung this boot is still UNKNOWN: the run kept no output to read.

## Fixed here

`zephyr_cortex_m_qemu` (and `pubsub_zephyr_cortex_m_ros2_interop_e2e`, which
shares its group row) gets `slow-timeout = { period = "60s", terminate-after = 3 }`,
a 180 s kill above its longest wait. A slow or hung boot now fails as the
test's own assertion, with the console attached. The cell itself is in the
batch's runtime run, solo, to read what the boot actually does.

## Still open: the class

Nothing checks that a test's own wait is shorter than the kill nextest gives
it. The effective kill is the first matching override per setting, so the
answer needs nextest's own resolution (`cargo nextest show-config`), which
needs the test list (a build). A static scan cannot answer it, and an approximate
one found these binaries with a literal wait of at least 60 s and no `slow-timeout` row
naming them. The list is UNVERIFIED: the scan ignores `test()` filters and
match order.

    action_multigoal (180 s), isr_wake_rtos (120 s), lending_freertos_e2e (90 s),
    violation_channel_e2e (90 s), action_raw_goal_e2e, condvar_contract_rtos,
    freertos_stack_guard, killed_run_verb, log_sink_small_stack,
    pubsub_zephyr_cortex_m_ros2_interop_e2e (fixed here), ros_editions_e2e,
    zenoh_send_stall (60 s each)

Acceptance for the class: a check over `cargo nextest show-config`'s
resolved per-test timeout against each test's declared budget, run where the
test list already exists (the lane that builds the tests). Its negative
control is this binary before the fix.

## What the console then showed (2026-11-22, host 241, #4597 at 84e3223b47)

With the 180 s kill the cell failed as its own assertion, console attached:
`Zephyr's in-kernel net stack never assigned the static address`. But the
same console shows the image publishing (`Hello World: 1..13`), and just
before that a `--- 14 messages dropped ---` notice during a burst of warnings
from the parameter service. The marker the cell waits for,
`IPv4 address: 10.0.2.15`, is `NET_INFO` in Zephyr's `net_config`, a deferred
log line, so it was most likely dropped. The net stack ran: on mps2_an385 the
smsc911x driver is the only route to the host router.

So this red belongs to the log-loss class, issues 2762 and 2815 (deferred
logging drops lines under a burst; nros 118's logging batch), not to the net
driver, and not to this issue's timeout. Until those land, the cell can read
"driver did not come up" for a healthy image. The fix that belongs to this
cell is a readiness marker no log drop can remove. That is left for the
logging batch, which owns the drop and can say which line survives it.

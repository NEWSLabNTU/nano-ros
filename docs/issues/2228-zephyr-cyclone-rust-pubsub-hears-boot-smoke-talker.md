---
id: 2228
title: "Zephyr Rust pubsub e2e counts a sample its own talker never sent: the boot smoke runs the same image on the same baked domain concurrently"
status: open
type: bug
severity: medium
area: [testing, zephyr]
related: [2221, 1424]
found: 2026-11-03
---

## What

`example_e2e::case_10_cyclonedds_rust_pubsub_e2e` starts the Rust Cyclone
listener, waits for readiness, starts the Rust talker, and passes the delivery
check once the listener reports `min_received` samples. Then the heap gate reads
both images' boot records.

`boot_smoke::cyclonedds_rust_talker_boots` starts the SAME talker image
(`build-rust-talker-cyclonedds`), whose `CONFIG_NROS_DOMAIN_ID` is baked. Both
tests are in test-group `qemu-zephyr-dds` (`max-threads = 4`), so they run
concurrently. The e2e listener then receives the smoke's talker, and the e2e's
own talker may not have booted yet.

## Measured

In the issue 2221 run (no retries, 25 lanes together), `case_10` failed at the
heap gate:

```
talker: ... REFUSED -- magic is 0x00000000, expected 0x4e525352.
  Not a boot report, or an image that never reached boot_report::init().
```

`boot_report::init()` runs in `Executor` assembly, before anything can publish.
So the talker the e2e started had not opened its executor, yet the listener had
counted a sample. `cyclonedds_rust_talker_boots` and
`cyclonedds_rust_listener_boots` finished 0.2 s before `case_10` (16.2 s,
16.2 s, 16.4 s). Solo, `case_10` passed 5 of 5.

The delivery check is the real defect: it accepts any sample on the topic as
proof the pair works. The heap gate only exposed it.

## Same symptom, mechanism not found

`case_19_xrce_rust_pubsub_e2e` failed the same way in a second run (talker
record `magic 0`, listener had counted `Hello World: 1`) and passed 5 of 5 solo.
Its agent port 2400 is per-cell, and there is no Rust XRCE boot smoke, so the
foreign publisher is not identified yet.

## Fix direction

- Give the e2e its own readiness edge on the talker: wait for the talker's
  publish marker (or its boot record) before counting the listener's samples.
- Keep two tests that start the same baked-domain image from overlapping:
  route the Rust boot smokes and the Rust e2e cells of one rmw into one group
  with `max-threads = 1`, or bake the smokes a domain no e2e uses.

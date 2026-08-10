---
id: 503
title: "nros-log Record.timestamp_ns exists but is hardcoded 0 at every emission site — on-target timestamps are one extern away"
status: open
type: enhancement
area: api
related: [issue-0502]
---

## Problem

`packages/core/nros-log/src/lib.rs` defines
`Record.timestamp_ns: u64` ("Monotonic timestamp in nanoseconds. `0` if
unavailable."), but nothing ever populates it: the `nros_info!`-family
macros (`macros.rs:37`) and the `log`-crate bridge (`log_compat.rs:141`)
both write a literal `0`, and no sink prints it. The field is dead
weight today.

Meanwhile `nros_platform_clock_us` is a universal export — every
platform port provides it and every image links it (the executor already
depends on that). Wiring the timestamp is one extern call at the
emission site plus formatting in the sinks.

## Why it matters

On embedded targets the only timestamps available for log analysis are
whatever the host attaches when it reads the serial/console stream, and
those measure the transport, not the target. Concretely, on an emulated
lane (QEMU `-icount shift=auto`, mps2-an385) host-stamped log cadence
showed 5.7% of a healthy 10 ms loop's periods as >1.5x nominal while
the target-side truth was 0.3% — a ~35x distortion from emulator clock
wobble plus console batching, measured by adding a guest-clock field to
the application's own log lines (NEWSLabNTU/nano-ros-rt-eval
`results/guest_cadence.md`, and its `src/island_clock` crate is exactly
the workaround this issue would delete). On real hardware the same
class of distortion appears as UART/DMA batching. Anyone debugging
timing from logs hits this; today they must hand-roll a clock into
their message text.

## Fix direction

- Macros default `timestamp_ns` from `nros_platform_clock_us()` (behind
  a feature gate if the extern must stay optional for host-side unit
  tests; the `log_compat` bridge takes the same default).
- Sinks that format for a console print it as a fixed-width prefix
  (`[123.456789]`), matching what RTOS shells and dmesg users expect;
  binary/deferred sinks already carry the field.
- Resolution note: the value is only as good as the platform clock —
  issue 0502 (ms-quantized `clock_us` on FreeRTOS/ThreadX) is the
  companion fix that makes the stamp worth printing there.

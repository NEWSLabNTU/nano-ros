---
id: 2847
title: "A test's readiness or observed marker rides a droppable deferred log, so a log burst can lose it"
status: open
type: bug
severity: low
area: [zephyr, testing, logging]
found: 2026-11-23
related: [2825, 2815, phase-590]
---

## What happens

Some Zephyr cells wait for a console line as their evidence: a readiness
marker (the image is up) or an observed marker (the image did what the cell
asserts). On an image built with `CONFIG_LOG_MODE_DEFERRED=y` and
`CONFIG_LOG_PRINTK=y`, which is Zephyr's default once `CONFIG_LOG=y`, every
such line is a deferred log message, and `printk` is one too. With
`CONFIG_LOG_MODE_OVERFLOW=y`, a full log buffer drops the OLDEST pending
messages to make room. A marker logged during a boot-time warning burst can
then be dropped. The cell sees no marker and fails on a healthy image.

The class is that channel: a marker a test reads, carried on a log that may
drop it. Fixing one site leaves the others.

## The sites

### mps2/an385 network readiness

The cortex-m cells (`zephyr_cortex_m_qemu`,
`pubsub_zephyr_cortex_m_ros2_interop_e2e`) wait for `NET_STACK_READY_MARKER`,
Zephyr's own `net_config` line `IPv4 address: 10.0.2.15`. That line is the
evidence that Zephyr's in-kernel network stack and the smsc911x driver came up,
so it has to stay Zephyr's. Issue 2825's console showed `--- 14 messages
dropped ---` during a boot-time warning burst, no marker, and the image
publishing anyway.

### qemu_cortex_a53 SMP core-pin placement

The `CorePinPlacement` cell in `sched_dims_applied_e2e` waits for
`nros: core pin observed sched_task=`high` running_on=1`. Two producers print
it: `printk` in `nros-board-zephyr/c/zephyr_run_sched_tasks.c` (the C entry
this cell runs) and `log::info!` in `nros-board-zephyr/src/entry_sched_tasks.rs`
(the Rust entry). Both are deferred log messages on this image.

Measured on 2026-11-23, current main, host load average about 75: three solo
runs, two passed with `running_on=1`. The third booted, brought up CPU 1 and
ran the app for 8,678 control ticks, but its console showed four
`--- N messages dropped ---` events in the boot burst, and the marker never
appeared. No run printed a wrong CPU. The image has `CONFIG_LOG_BUFFER_SIZE=1024`.

`CONFIG_NROS_CONSOLE_WHOLE_LINES` (issue 2384) makes this wider on a C image:
it sends C `stdout` through `printk`, so a `printf` marker is a deferred log
message too.

## Survey

Of the 36 Zephyr images built on host 241 on 2026-11-23, two have
deferred-mode `printk` with overflow: `build-cortex-m-c-talker-zenoh` (mps2)
and `build-ws-c-realtime-entry-smp` (qemu_cortex_a53 SMP), the two sites
above. The others set `CONFIG_LOG_MODE_IMMEDIATE=y` (the cyclonedds snippet
and the per-board cyclonedds confs) or build without `CONFIG_LOG_PRINTK`. That
is a sample of the images one host had built, not the full fixture set. A
zenoh-lane image that sets neither inherits the deferred default.

## What has been done, and why it is not the end

- Issue 2815's fix makes the burst quieter: one history-depth warning
  instead of twelve.
- The mps2 board fragment sets `CONFIG_LOG_BUFFER_SIZE=4096`, from 1024.

Both make the drop rarer. Neither makes it impossible: the next burst that
outgrows the buffer drops the marker again, and the test reads as a broken
image.

## Directions (not decided)

1. `CONFIG_LOG_MODE_OVERFLOW=n` on the images whose cells read markers. A
   full buffer then drops NEW messages, so a marker logged early survives,
   and a late one does not. Whether that covers a site depends on when its
   marker prints relative to the burst, which has not been measured.
2. Print each test-observed marker on a channel the log buffer cannot drop.
   For the network marker, read the assigned address from the network
   interface (`net_if`) after `net_config` runs, so the evidence stays
   Zephyr's own (it reads the stack's state). For a `printk` marker, the
   choices are `CONFIG_LOG_MODE_IMMEDIATE` on that image, or a console write
   that does not go through the log.
3. Have the harness treat `--- N messages dropped ---` before the marker as
   "marker unknown", not "marker absent", and fall back to a second witness.
   This makes the verdict honest without making the marker reliable.

## Closes when

No test-observed marker on a Zephyr image can be lost to log-buffer
overflow, at both sites above and at any site found later. A cell that floods
the log at boot still sees its marker, and a gate or self-test refuses a new
marker added on the droppable channel.

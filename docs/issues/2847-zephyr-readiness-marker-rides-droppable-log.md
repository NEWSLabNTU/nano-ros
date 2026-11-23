---
id: 2847
title: "A test's readiness or observed marker rides a droppable deferred log, so a log burst can lose it"
status: open
type: bug
severity: low
area: [zephyr, testing, logging]
found: 2026-11-23
related: [2825, 2815]
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

Zephyr's `choice LOG_MODE` defaults to `LOG_MODE_IMMEDIATE` on `ARCH_POSIX`
and to `LOG_MODE_DEFERRED` everywhere else. So every native_sim image is
immediate and never had this channel, and every other board with
`CONFIG_LOG=y` is deferred unless its conf says otherwise. In
`examples/fixtures.toml` that is 18 Zephyr rows: 13 on mps2/an385, 4 on
qemu_cortex_a53 SMP (the realtime entry, two log-sink cells and the
dead-holder cell) and 1 on nucleo_f767zi. A first survey counted only the
images one host had built, which were mostly native_sim, and understated
this.

## What had been done before the fix

- Issue 2815's fix makes the burst quieter: one history-depth warning
  instead of twelve.
- The mps2 board fragment sets `CONFIG_LOG_BUFFER_SIZE=4096`, from 1024.

Both made the drop rarer, not impossible.

## Directions that were considered

1. `CONFIG_LOG_MODE_OVERFLOW=n`: a full buffer drops NEW messages, so an
   early marker survives and a late one does not. Not chosen: it moves the
   loss rather than removing it.
2. A separate console channel for each marker (`k_str_out`, or reading the
   address from `net_if`). Not chosen: it needs a change at every site,
   including Zephyr's own `net_config` line, and on SMP a raw console write
   can interleave with the logging thread's output character by character
   (the issue 2384 class).
3. Have the harness treat `--- N messages dropped ---` before the marker as
   "marker unknown". Not chosen: it makes the verdict honest without making
   the marker reliable.

## The fix

`zephyr/Kconfig` gives every nros image with deferred logging
`CONFIG_LOG_BLOCK_IN_THREAD=y` and `CONFIG_LOG_BLOCK_IN_THREAD_TIMEOUT_MS=10000`
(second definitions of Zephyr's symbols, which win because modules are sourced
first). In `mpsc_pbuf_alloc`, a thread-context allocation that finds the
buffer full now waits on the buffer's semaphore before the drop branch is
reached, so the logging thread runs and makes room. This covers every marker
at once, whoever prints it and however (`printk`, `LOG_*`, Rust `log::`, C
`printf` through `CONFIG_NROS_CONSOLE_WHOLE_LINES`).

What it does not cover:

- **ISR context.** An interrupt cannot wait, so a message logged from an ISR
  can still be dropped. No marker a test reads is printed from an ISR: the
  core-pin line comes from the sched task's own thread, and `net_config`'s
  static-address line from the `SYS_INIT(APPLICATION)` hook, which runs in
  the init thread.
- **A logging thread starved for 10 s.** The wait is bounded, because a
  forever wait deadlocks if the logging thread logs into a full buffer. A
  cell whose logging thread cannot run for 10 s fails anyway.

The cost is that a thread logging into a full buffer stalls instead of losing
its message. The board's sched-task runner logs only at setup and on
failure, never per tick; the realtime demo's apps print one line per tick.
Measured on qemu_cortex_a53 SMP at host load about 85, 70 s: the 100 Hz
control thread printed 6,757 consecutive ticks in about 66 s of guest time
(about 102 Hz), every console line arrived, and no drop was reported. The
`CorePinPlacement` cell then passed 5 of 5 solo runs at host load about 77,
where before the fix it had passed 2 of 3.

`just check zephyr-log-markers-lossless` holds both defaults and refuses a
tracked `.conf` that turns them off.

## Closes when

No test-observed marker on a Zephyr image can be lost to log-buffer
overflow, at both sites above and at any site found later. A cell that floods
the log at boot still sees its marker, and a gate or self-test refuses a new
marker added on the droppable channel.

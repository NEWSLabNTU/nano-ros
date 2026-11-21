---
id: 2815
title: "The Nucleo-F767ZI C talker faults in Zephyr's logging at boot: the main thread branches to `log_buffer` during the parameter-service warning burst"
status: open
type: bug
area: [zephyr, logging, boards]
severity: high
found: 2026-11-22
related: [552, 2387, 1726, phase-590]
---

## What fails

`zephyr_renode_twin::zephyr_nucleo_renode_c_zenoh_serial_pubsub_e2e` fails in
tier 2. The run is 44916540238, at main `cbf8c22e6f`, on 118's runner:

> the host listener heard nothing from the Nucleo-F767ZI twin over the serial
> link in 120s

The test does not fail on Renode. Its Renode log ends with `Got exception when
reading from socket: ... Connection reset by peer`, and that line reads like
the cause. It is not. `packages/runners/nros_runner_renode/src/main.rs` records
that Renode prints exactly that trace whenever its monitor socket closes ("which
reads like a failure and is none").

The failure is in the guest console the test kept:

    nros: wall clock from platform
    <wrn> nros: ... service request subscription `/talker/describe_parameters`: asked for history depth 1000 and the backend granted 4.
    --- 10 messages dropped ---
    <wrn> nros: ... service response publisher `/talker/describe_parameters`: asked for history depth 1000 and the backend granted 4.
    <wrn> nros: ... `/talker/get_parameter_types` ... (two more)
    <err> os: ***** USAGE FAULT *****
    <err> os:   Illegal use of the EPSR
    <err> os: r0/a1:  0x000000ff  r1/a2:  0x2002239c  r2/a3:  0x20058838
    <err> os: r3/a4:  0x08017dd7 r12/ip:  0x20058838 r14/lr:  0x00000006
    <err> os: Faulting instruction address (r15/pc): 0x2002239c
    <err> os: >>> ZEPHYR FATAL ERROR 35: Unknown error on CPU 0
    <err> os: Current thread: 0x2005dd28 (unknown)

The image opens its session over the serial link, starts creating the six
parameter services, warns once per endpoint (the QoS depth downgrade), drops
log messages, and faults 78 ms after boot.

## What the addresses are

Symbolized against the image the run used. It was copied off the runner:
`build/zephyr-workspace-builds/3.7/build-nucleo-f767zi-c-talker-serial/zephyr/zephyr.elf`,
built 2026-11-21 15:27Z.

| register | value | symbol |
| --- | --- | --- |
| pc, r1 | `0x2002239c` | `log_buffer` (`d`, 0x44 bytes), the start of `_log_mpsc_pbuf_list`: Zephyr logging's mpsc packet buffer, a DATA object |
| r2, r12 | `0x20058838` | inside `buf32` (0x400 bytes at `0x20058628`), the `CONFIG_LOG_BUFFER_SIZE=1024` storage behind it |
| r3 | `0x08017dd7` | `__set_BASEPRI` (CMSIS), a Thumb address |
| lr | `0x00000006` | not a return address |
| current thread | `0x2005dd28` | `z_main_thread` |

So the MAIN thread, inside a logging call, branched to the address of
`log_buffer` while passing that same address as the first argument. That is the
shape of a call through a function pointer read out of the buffer. The mpsc
pbuf holds two of them, `get_wlen` and `notify_drop`, and the console shows the
drop path ran. A call to an even address is an ARM-state branch, which a Cortex-M
cannot take, hence `Illegal use of the EPSR`.

Relevant configuration, from that build's `.config`:

- `CONFIG_LOG_MODE_DEFERRED=y` and `CONFIG_LOG_MODE_OVERFLOW=y`, with
  `CONFIG_LOG_BUFFER_SIZE=1024`;
- `CONFIG_LOG_PROCESS_THREAD_STACK_SIZE=768`;
- `CONFIG_MAIN_STACK_SIZE=16384`, which the build's stack-floor check passed;
- `CONFIG_HW_STACK_PROTECTION=y` and `CONFIG_MPU_STACK_GUARD=y`.

The warning text is not new: it has been in `packages/rmw/cffi/src/lib.rs`
since `16b79ea8fa` (2026-08-27).

## Hypotheses

1. **`log_buffer` is overwritten.** Something writes over the mpsc pbuf's state,
   so its callback reads as its own address. This now looks the more likely of
   the two. A plain main-stack overflow on this build would normally trap as an
   MPU stack-guard fault, not as this.
2. **Main-stack overflow during parameter-service registration.** This is
   issues 0552 and 2387, whose signature this matches: a large frame that steps
   past the guard region and lands in a logging frame. Not ruled out.

## Discriminators

Each of these is one image rebuild and one run of the cell:

- `CONFIG_LOG_MODE_IMMEDIATE=y`. This takes the mpsc pbuf out of the path. If
  the fault disappears and nothing else changes, the corruption is the deferred
  buffer's.
- `CONFIG_MAIN_STACK_SIZE=32768`. If the fault disappears, it is the stack after
  all.
- A Renode write watchpoint over `log_buffer`'s 0x44 bytes
  (`sysbus AddWatchpointHook 0x2002239c Word Write "..."`). This names the
  writer directly.

## Reproduction

Only with the serial link and router, that is, the test itself. Booting the
copied image alone in Renode 1.17.0, with no peer, ran clean three times out of
three. That proves nothing: the session never opens, no entity is created, and
the image ends at 12 s in `NodeError::Transport`. The real cell's counts are in the verdict above.

## Verdict, 2026-11-22: the main stack

Host 241 ran the discriminator cell at main `9c8ecf5dcb`, three runs a
variant, with the host peer built and each variant's `.config` checked:

| variant | runs | result |
| --- | --- | --- |
| baseline: `MAIN_STACK_SIZE=16384`, deferred logging | 3 | USAGE FAULT, Illegal use of the EPSR (PC `0x08044308` twice, `0x2002239c`) |
| `CONFIG_LOG_MODE_IMMEDIATE=y` | 3 | USAGE FAULT (PC `0x08043510`, `0x00000000`, `0x0805b043`) |
| `MAIN_STACK_SIZE=32768` | 3 | pass, no fault |

So hypothesis 2 holds and hypothesis 1 is ruled out. Main's stack overflows
while the parameter services register. `log_buffer` was only what the
overflow trampled; with immediate logging the corruption lands somewhere else.

**Why the MPU stack guard did not trap it.** The frames on that path are far
larger than the guard region. Prologue `sub sp` sizes, from the run's own
image:

| function | frame |
| --- | --- |
| `Executor::pass` | 9,472 B |
| `Executor::reconcile_parameter_services` | 9,344 B |
| `Executor::ensure_parameter_store` | 8,704 B |
| `parameter_services::read_parameter_value_into` | 8,320 B |

`pass` calls `reconcile_parameter_services`, so those two frames alone take
18.8 KB of a 16 KiB stack. One `sub sp` of 9 KB moves the stack pointer
straight past the guard and into `.data`, and the next store lands there:
here in `log_buffer`. Nothing touched the guard, so nothing faulted.

`reconcile_parameter_services`'s frame is its `todo` list, a
`heapless::Vec<(u8, String<64>, String<64>), MAX_SERVICE_SETS>` built on the
stack, about 130 bytes for each node the table can hold. This is issue
2387's class one function over: 2387 moved the service set out of this frame
and left the list that feeds it.

**Fix direction.** Shrink the frames rather than raise every image's stack:
the list holds node indices only and the names are cloned per iteration, and
the other three frames are checked for the same shape. Then measure main's
peak on this board. A frame-size check over the Cortex-M fixture images keeps
a frame larger than the guard from coming back unnoticed.

These are the frames PR #4589's sweep (issue 2820) listed as "fit, not
fixed": they fit a 64 KiB FreeRTOS app stack, but not a 16 KiB Zephyr main.
This issue fixes them, starting with the four above and taking
`ExecutorSink::create_node` (17.8 KiB) and the `create_entity` closures
(5.6-16.8 KiB) from that list too, since they run on the same registration
path. The frame-size check is not a second scanner: it belongs in the
stack-floor tooling (`scripts/nros-stack-floor.py`), which already reads every frame.

## A gate for the frame-size class

One stack frame larger than an MPU stack guard (32-64 B) jumps the guard, so an
overflow through it writes past the stack with no fault, which is one way to
reach the corruption above. `just check max-frame` (`scripts/check-max-frame.py`)
fails on any frame in nano-ros's own code over `max_frame` (4096 B, in
`config/stack-floors.toml`) unless the shrink-only
`config/max-frame-baseline.json` already holds it. It reads the `big_frames`
each fixture build's stack-floor report records, over every 32-bit ARM image,
and runs after the fixture build of `just ci` matrix, matrix-nightly and full.
The baseline holds one row per function at the largest size any build has (57
functions, from 501 frames across 74 builds; 51 once this issue's fix dropped
the six rows it brought under the cap), so a build it never saw passes while
it adds and grows nothing; it is the list to shrink. The gate
stops the class growing; it does not by itself name this fault's cause.

## Fixed, 2026-11-23: the frames alone cure it

`ParameterValue` is about 8.3 KiB whichever variant it holds, because the
`StringArray` variant sizes it, so every by-value one is a frame of that size.
The registration path now fills a parameter slot in place
(`ParameterServer::declare_with`), and the reconcile list holds node indices
only. Measured prologues (thumbv7em-none-eabihf, release, LTO off):

| Function | Before | After |
| --- | --- | --- |
| `Executor::pass` | 9,472 B | 2,732 B |
| `Executor::reconcile_parameter_services` | 10,304 B | under 1,024 B |
| `Executor::declare_parameter_resolved` | 8,320 B | under 1,024 B |
| `ExecutorSink::create_node` | 17,536 B | 9,472 B |

The Nucleo Renode cell, at the image's stock `CONFIG_MAIN_STACK_SIZE=16384`
with deferred logging and `CONFIG_LOG_MODE_OVERFLOW=y` (the configuration
that faulted 3/3 on main), passes 3/3 with this fix alone (host 241). The
max-frame baseline lost the rows for the first three and for
`seed_use_sim_time_default`, so any of them growing back past the cap fails
`just check max-frame`.

Still over the cap and still in the baseline: `create_node`,
`declare_parameter_overridden`, `declare_described_overridden` and the
action/lifecycle registration frames (6-9 KiB, the same by-value
`ParameterValue` shape). They no longer overflow this image, and the
shrink-only baseline is the list to work through.

## The warning burst, also reduced

`report_qos_downgrade` (`packages/rmw/cffi/src/lib.rs`) warned once per
ENTITY for a history-depth clamp. A clamp is the backend's fixed ceiling, so
every endpoint asking the same depth gets the same answer. An image serving the
parameter services asks 1000 for each of its twelve service endpoints and
zenoh-pico grants 4: twelve identical warnings in the first 80 ms of boot, the
burst this fault sits inside.

It now warns once per distinct (requested, granted) depth pair and logs the
repeats at debug (`first_depth_downgrade`). Reliability and durability
downgrades stay one warning per entity: they are what silences a pair, and they
are rare. The frames fix above is what cures the fault; this one only stops
the boot from printing the same line twelve times.

## Closes when

The cell passes and the cause above is named and fixed, with a cell that would
fail again if it returned.

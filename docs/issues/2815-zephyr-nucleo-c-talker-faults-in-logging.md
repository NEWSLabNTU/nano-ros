---
id: 2815
title: "The Nucleo-F767ZI C talker faults in Zephyr's logging at boot: the main thread branches to `log_buffer` during the parameter-service warning burst"
status: open
type: bug
area: [zephyr, logging, boards]
severity: high
found: 2026-11-22
related: [552, 2387, 1726]
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
the image ends at 12 s in `NodeError::Transport`. A 3x solo run of the real
cell is queued on host 241, and its counts go here.

## Closes when

The cell passes and the cause above is named and fixed, with a cell that would
fail again if it returned.

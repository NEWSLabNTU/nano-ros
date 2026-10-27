---
id: 2105
title: "On a Zephyr mps2_an385 zenoh image, an executor future that returns wake_up_after(1000) is never polled again"
status: open
type: bug
area: [zephyr, zenoh]
severity: medium
found: 2026-10-27
related: [rfc-0110, phase-508, 2094, 1636]
---

## Measured

The image was `build-cortex-m-c-talker-zenoh` (C talker, `mps2_an385`,
QEMU, SLIRP, publishing at 1 Hz), built with a temporary
`CONFIG_INIT_STACKS=y` overlay so the RFC-0110 stack sampler is compiled in.
The run used temporary `printk` instrumentation, since reverted:

```
TMPDBG spawn 1
TMPDBG sample 2 used 992 size 131072
```

The sampler future (`zpico.c` `_zpico_exec_stack_fn`) did the following:
- it was spawned, and its first poll returned `wake_up_after(4000)`;
- it was polled once more at about +4 s and took a sample;
- it returned `wake_up_after(1000)`;
- it was never polled again in the following 86 s.

The talker kept publishing throughout, so the executor thread was alive.

## Why it matters

The keep-alive and lease are timed futures on the same executor. A talker
hides a keep-alive that never runs, because its own traffic sets
`_transmitted`. So this image cannot tell us whether its keep-alive runs.
Issue 2094 was exactly that shape on NuttX: a timed future scheduled at
garbage, invisible to every cell with traffic.

## What it is not

- **Not issue 2094's clock split.** Zephyr builds its own zenoh-pico system
  layer in C (`zephyr/nros_zenoh_zephyr_system.c`), and the alias TU skips
  its system section there (issue 1636). That clock counts nanoseconds in
  64 bits.
- **Not general to Zephyr.** On Zephyr native_sim, a quiet listener held
  one session for 150 s, with client keep-alives every ~21 s. Timed futures
  wake there.

## Next

- Run `test_rtos_quiet_link_keeps_its_session`'s shape on a Zephyr Cortex-M
  LISTENER, which has no traffic of its own. No such fixture exists yet: the
  Cortex-M rows are talkers.
- Dump the executor with gdb on the QEMU gdbstub, the way issue 2094 did.
  Break on `_z_executor_get_next_fut`'s entry ADDRESS. The executor is in
  `r1`, because `r0` is the struct-return slot.

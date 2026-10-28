---
id: 2094
title: "A quiet NuttX zenoh link loses its session after 60 s: the executor's clock had the wrong ABI, so no keep-alive went out"
status: resolved
type: bug
area: [nuttx, zenoh]
severity: high
found: 2026-10-26
resolved: 2026-10-27
resolved_in: "fix/2094-nuttx-keepalive (phase-508)"
related: [rfc-0110, phase-508, 0906, 1636]
---

## Symptom

A NuttX Rust listener (`nuttx::build_nuttx_listener`, QEMU `virt`, SLIRP),
alone against `rmw_zenohd` with no talker, lost its session at the 60 s
lease in 3 of 3 runs, then reconnected repeatedly. The router saw at most two
client keep-alives in the first minute, where it was owed one every 20 s
(`Z_TRANSPORT_LEASE` 60000, issue 0906; `Z_TRANSPORT_LEASE_EXPIRE_FACTOR`
3). FreeRTOS (mps2-an385) and Zephyr native_sim held one session over the
same soak. The pub/sub, service and action cells all passed on NuttX: any
traffic sets `_transmitted`, so a keep-alive that never runs is never missed.

## Root cause: two defects, one hiding the other

Found with gdb on QEMU's gdbstub: logging breakpoints on the executor's
futures, and dumps of the live executor taken at `_z_executor_get_next_fut`.
Two pitfalls cost runs here. Both functions return a struct, so `r0` is
the hidden return slot and the executor is in `r1`. And a breakpoint set by
NAME lands after the prologue, where an argument register may already be
reused, so break on the entry ADDRESS.

1. **The linked clock had the wrong ABI.** `ZENOH_NUTTX` makes every
   zenoh-pico TU include `unix.h`, where `z_clock_t` is a 16-byte
   `struct timespec`. The alias TU (`platform_aliases.c`) is compiled
   against the GENERIC header and defines the same clock symbols for
   `uint64_t` ms. That is a different calling convention: a struct is
   returned through a hidden pointer, a `uint64_t` in r0:r1. So every
   `z_clock_now()` in the executor read a result slot nobody wrote.
   - The executor dump showed the keep-alive future's schedule as
     `0x000000bf_da9e2002`: SLEEPING until `0xbfda9e20` ms, about 37 days
     out.
   - The value was stack garbage, so the behaviour changed from run to
     run. In one run the keep-alive did fire on time.
   - `nuttx_clock.c` had the right implementation all along, and it was
     never linked. Nothing referenced a symbol only it defined, so the
     alias TU's copies won. The phase-508 port noted it as "dead code, a
     separate finding" and routed around it.
2. **`nuttx_clock.c` counted nanoseconds in `unsigned long`**, which is 32
   bits on armv7a and wraps every 4.29 s. It had never run, so nothing had
   exercised it. Once linked, every "elapsed since the executor epoch"
   landed modulo 4.29 s: the lease future fired at 20.8 s of a 60 s lease,
   and after that a reconnect followed every ~5.5 s.

## Fix

- **The alias TU gains `NROS_PLATFORM_ALIASES_SKIP_CLOCK`.** It covers the
  clock functions and `_z_condvar_wait_until`, whose `abstime` is a
  `z_clock_t*` too. `nros-zpico-build` defines it for NuttX. This is issue
  1636's Zephyr fix, narrowed to the clock, because NuttX still takes its
  mutexes and tasks from the alias TU.
- **`nuttx_clock.c` is now live and gains `_z_condvar_wait_until`.** It
  carries the REMAINING time onto the `nros_platform_clock_ns()` timeline,
  so the two monotonic clocks need no shared epoch. Codes are translated as
  issue 1906 did.
  - zenoh-pico always references that symbol, so the object is always
    linked. If the alias clock ever comes back on NuttX, the result is a
    duplicate-symbol link error, never a silent ABI split.
- **`nuttx_clock.c` counts nanoseconds in `uint64_t`.**
- **Sweep.** Every other port's alias TU sees the generic header (POSIX,
  ThreadX, bare metal), or skips the system section (Zephyr, issue 1636),
  or builds scalar-only (FreeRTOS). Zephyr's own clock already counts in
  64 bits. NuttX was the only port with the split.
- **Regression cell:** `test_rtos_quiet_link_keeps_its_session`
  (`rtos_e2e`, FreeRTOS / NuttX / ThreadX-Linux). A listener alone for 75 s
  must hold ONE router session. No existing cell was quiet for longer than
  the lease, which is how this shipped.

## Verified

The test catches the defect: with only the first fix applied, NuttX gave 11
sessions in 75 s, because defect 2 was still in place. After both fixes:

- The quiet-link cell passes on all three platforms.
- A 150 s NuttX soak held one session, with client keep-alives every 25–30
  s.
- The stack sampler prints, which it never did before, because timed
  futures now wake.

## What is left, measured

- **Timers on NuttX run late by up to two socket timeouts.** The executor
  checks its timers only when the blocking read returns, which is every
  5 s. And `_z_executor_get_next_fut` runs a ready read ahead of a timer
  that is due. So the keep-alive goes out at 25–30 s rather than 20 s, and
  the lease check runs ~9 s late. Against the router's 60 s expiry that is
  still a 2x margin.
- **The 100 ms experiment is NOT evidence that NuttX needs 5000 ms.** That
  run (99 sessions in 150 s) was on the broken clock, so it says nothing
  about the timeout. Measured afterwards on the fixed clock (2026-10-29):
  NuttX takes 100 ms, the quiet link holds one session, every NuttX
  `rtos_e2e` cell passes 2–16x faster, and the 5000 ms special case is
  gone from `nros-zpico-build` and the NuttX manifest.
- **Attribution.** zenoh-pico 1.8's lease task slept on the same clock, so
  the ABI split probably predates RFC-0110. A dedicated lease thread
  tolerated it; the executor's sleeping-futures queue did not.
- **A Zephyr mps2_an385 image** in which the stack sampler took one sample
  and never woke again is NOT explained by this. Zephyr's clock is its own
  64-bit TU (issue 1636). It needs its own look.

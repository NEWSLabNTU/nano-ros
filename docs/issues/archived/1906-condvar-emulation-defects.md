---
id: 1906
title: "Four condvar defects in the platform ports and the zenoh-pico shim: the
  wrong timeout code, a broadcast that wakes one waiter, a wall-clock timed wait
  and timeouts rounded down"
status: resolved
type: bug
area: [platform, rmw]
severity: high
found: 2026-10-15
resolved: 2026-10-15
related: [issue-1196, issue-1907, issue-1750]
---

**Home: issue 1196** — found by its design study; the API redesign there
depends on these being right first.

## What

Four defects in how the platform ports implement condition variables, and in
how the zenoh-pico alias shim reaches them. Each is fixed in the change that
filed this issue. What is still open is a runtime test on ThreadX and
FreeRTOS (see "Not yet verified").

### 1. The shim returned the platform's timeout code to zenoh-pico

`zpico-sys/c/zpico/platform_aliases.c`'s `_z_condvar_wait_until` returned
`nros_platform_condvar_wait_until`'s value unchanged. The platform returns `1`
on timeout. zenoh-pico's contract is `Z_ETIMEDOUT` (-71,
`zenoh-pico/utils/result.h:94`), and every upstream system layer maps it, as
does our own Zephyr system file.

The consequence is in `src/collections/sync_group.c`:

```c
ret = _z_condvar_wait_until(&state->counter_condvar, &state->counter_mutex, deadline);
if (ret == Z_ETIMEDOUT) {
    break;
} else if (ret != _Z_RES_OK) {
    return ret;          // `1` lands here, with counter_mutex still HELD
}
```

`_z_sync_group_wait_deadline` returned while holding `counter_mutex`, so every
later notifier drop or close blocked on it forever.
`_z_cancellation_token_cancel_with_timeout` also never reported a timeout. The
path is believed unreached in nano-ros today, because `zpico.c` uses no
cancellation tokens (inferred, not proven). It is the path a bounded design
leans on.

The shim also fell back to the unbounded `condvar_wait` on a NULL deadline. No
zenoh-pico caller passes NULL, and upstream's own implementations dereference
the pointer unconditionally, so that contract was ours alone.

**Fix.** `0` maps to `_Z_RES_OK`, a positive code to `Z_ETIMEDOUT` and a
negative one to `_Z_ERR_GENERIC`. A NULL deadline is `_Z_ERR_GENERIC`. The
alias TU now reads the codes from `result.h`, which needed zenoh-pico's include
dir on its build (`nros-zpico-build/src/runner.rs`).

### 2. ThreadX's broadcast woke one waiter

`nros-platform-threadx`'s condvar was a bare `TX_SEMAPHORE`, so
`condvar_signal_all` put a single token ("the semantic is 'wake at least
one'"). It is not: a broadcast wakes all. zenoh-pico broadcasts in
`_z_sync_group_close`, on notifier drop, and in `_z_fifo_mt_close` /
`_z_ring_mt_close`. With two waiters the second slept forever, and on the
unbounded `condvar_wait` (issue 1196) nothing would ever wake it. A signal
with nobody waiting also banked a token, so later waits returned at once.

**Fix.** The storage is a semaphore plus a waiter count, updated with
interrupts disabled. A signal releases one registered waiter, and a broadcast
releases all of them. A wait that times out deregisters itself. A token a
signaller put after that timeout becomes a spurious wake for the next waiter,
which condvar semantics allow.

### 3. POSIX and NuttX timed waits ran on the wall clock

`nros-platform-posix` built the condvar with the default clock,
`CLOCK_REALTIME`, and converted the monotonic deadline into a relative delay
re-anchored on the wall clock. A clock step during the wait (NTP, `date`)
moved the deadline: longer when the clock stepped back, early when it stepped
forward. Upstream zenoh-pico's unix layer sets `CLOCK_MONOTONIC` on its
condvar, so ours had regressed from it. NuttX compiles the same file and
honours the clock attribute (`pthread_condattr_setclock`).

The "pthread timed wait hangs on NuttX" note (`zpico.c`,
`platform-implementation-notes.md`) may be this defect. That is inferred, not
measured.

**Fix.** The condvar is created with `CLOCK_MONOTONIC`, and `wait_until` passes
the deadline through unchanged.

### 4. FreeRTOS and ThreadX rounded timeouts down

FreeRTOS converted with `pdMS_TO_TICKS`, and ThreadX with
`ms * TX_TIMER_TICKS_PER_SECOND / 1000`. Both round down, so the wait timed
out before its deadline, and a remainder under one tick became a zero-tick
poll that a predicate loop spins on. At 100 Hz that is any remainder under
10 ms. A product landing on `portMAX_DELAY` or `TX_WAIT_FOREVER` would have
turned the bounded wait into an unbounded one. The ThreadX file's own
`wake_wait_ms` already rounded up, so two conversions in one file disagreed.

**Fix.** Both ports round up and clamp below the "forever" value. ThreadX now
also maps `TX_WAIT_ABORTED` to a timeout, as `wake_wait_ms` does. FreeRTOS
returned `-1` on timeout, so a caller could not tell a timeout from an error.
It now returns `1`, and `nros/platform.h` states the codes: 0 signalled,
1 timeout, -1 error.

## Verified

- POSIX: `nros-platform-cffi/tests/c_port_posix.rs` gains
  `condvar_wait_until_times_out_with_code_1_and_not_before_its_deadline` and
  `condvar_signal_all_wakes_every_waiter`. Both pass (13/13 in the file).
  With the clock attribute removed again, the first fails with
  `timed out after 7.752µs, before its 50 ms deadline`.
- Shim: `nros-rmw-zenoh/tests/condvar_shim_codes.rs` calls
  `_z_condvar_wait_until` as zenoh-pico does and asserts `Z_ETIMEDOUT`, and that
  a NULL deadline is an error. Both pass. With the mapping reverted, the first
  fails with `got 1`. It runs in `check required-features-tests`.
- ThreadX: `nros-board-threadx-linux` builds, and the archive carries the new
  code.
- FreeRTOS: `nros-board-mps2-an385-freertos` builds for `thumbv7m-none-eabi`.

## Not yet verified

ThreadX and FreeRTOS are COMPILE-checked only. No test runs either port's
condvar: there is no host harness for them like `c_port_posix.rs`, and the
fixture images exercise a broadcast only on a close with several waiters. What
would close this issue: a ThreadX-linux test that parks three waiters and
broadcasts, plus a timed wait asserting `1` and a lower bound, as the POSIX
tests do.

## Resolution (2026-10-15)

The open item, a runtime test on ThreadX and FreeRTOS, is done. Both ports now
run the contract on a real kernel.

`packages/testing/nros-condvar-contract` is a `no_std` crate that drives the
`nros_platform_*` C ABI directly. Two test images run it,
`bins/condvar-contract-threadx-linux` and `bins/condvar-contract-freertos-mps2`.
Each prints one line per check, and `tests/condvar_contract_rtos.rs` reads
them. The marker strings come from the crate, re-exported through
`nros_tests::output`, so the image and the test share one spelling. The
checks:

- a timed wait nobody signals returns `1`, and not before its deadline;
- a broadcast releases all three waiters within 1 s;
- ThreadX only: a wait under a mutex held twice is refused (issue 1907).

Measured:

| Port | Result | Negative control |
| --- | --- | --- |
| ThreadX Linux | all three pass | single-waiter `signal_all` put back: `FAIL broadcast: a waiter missed the broadcast` |
| FreeRTOS (QEMU MPS2-AN385) | both pass | `-1` timeout code put back: `FAIL timed-wait`, QEMU exit 1 |

**The first version of the broadcast check could not fail, and its negative
control is what showed it.** With the one-waiter broadcast put back, two
waiters ran out their 5 s deadline, then saw the release flag, and were
counted as woken. The check passed at the 5.06 s mark. A waiter now counts
only if a wake released it, and the broadcast must finish within 1 s. The
POSIX twin of this test in `c_port_posix.rs` had the same hole and has the
same fix.

Not covered: FreeRTOS's tick rounding. Every shipped FreeRTOS config ticks at
1000 Hz, so a 50 ms wait is a whole number of ticks either way, and rounding
down could not be told apart from rounding up there.

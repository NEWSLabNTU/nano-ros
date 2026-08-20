---
id: 715
title: "Every threadx-linux CycloneDDS image SEGVs in `_tx_thread_timeout` from the ThreadX timer thread, seconds after reaching its ready banner"
status: resolved
type: bug
area: boards/threadx-linux
related: [issue-0713, rfc-0072]
---

## Symptom

Three tier-2 failures, one cause:

```
test_threadx_linux_cyclonedds_service     — "produced no calls/requests", server output EMPTY
test_threadx_linux_cyclonedds_action      — "produced no result"
test_threadx_linux_cyclonedds_talker_to_native_listener — listener never received 2 messages
```

Run any of the three binaries by hand and it dies:

```
$ NROS_DOMAIN_ID=107 timeout 12 examples/threadx-linux/c/service-server/build-cyclonedds/c_service_server
...
Waiting for service requests (Ctrl+C to exit)...
timeout: the monitored command dumped core        # rc=139 (SIGSEGV)
```

3 of 3 reproduce, every run:

| binary | rc |
| --- | --- |
| `c/service-server/build-cyclonedds/c_service_server` | 139 |
| `c/action-server/build-cyclonedds/c_action_server` | 139 |
| `c/talker/build-cyclonedds/c_talker` | 139 |

The image gets all the way through init — byte pool, board network init, app
thread, `app_main`, node created, entity created — prints its READY banner, and
then faults.

## Where

```
Thread 3 "c_talker" received signal SIGSEGV, Segmentation fault.
#0  _tx_thread_timeout ()
#1  _tx_timer_thread_entry ()
#2  _tx_thread_shell_entry ()
#3  _tx_linux_thread_entry ()
#4  start_thread ()
```

It is the ThreadX TIMER thread, in `_tx_thread_timeout` — the kernel's expiry
walk — not application code and not Cyclone's own threads. That points at a
corrupted or freed `TX_THREAD` / timer entry still linked into the expiry list,
rather than at a null argument on some API call.

Note when it happens: AFTER the ready banner, i.e. after the RMW is up and the
first timer-driven work would begin.

To reproduce under gdb, pass the port's own signals through or you will stop on
them instead of the fault:

```
gdb -q -batch \
  -ex "handle SIGUSR1 SIGUSR2 SIGALRM nostop noprint pass" \
  -ex run -ex bt <binary>
```

`SIGUSR1` is `_tx_linux_timer_interrupt`, `SIGUSR2` is
`_tx_linux_thread_suspend_handler` — both normal for the ThreadX Linux port.

## Scope

CycloneDDS only, as far as measured. The zenoh builds of the same three examples
are not covered by this issue and were not tested here; `threadx_linux` passes
its OTHER tier-2 coordinates, so this is not the whole platform.

## Why it was not caught earlier

`threadx_linux` + cyclonedds is a tier-2 coordinate. Tier 1 is native-only, so
no lane a developer runs before pushing builds or executes these images. Tier 2
had not completed on this host until today — it was blocked in sequence by
issue 0698 (CMake 4), a stale issue-index row, a NuttX header gate, and two
boards that did not compile (issue 0708).

## Root cause + fix (2026-08-20)

The vendored Linux port narrows `LONG`/`ULONG` to 32-bit `int` on LP64, so
`TX_TIMER_INTERNAL.tx_timer_internal_timeout_param` — a `ULONG` — can no longer
hold a thread pointer. The port compensates by routing the pointer through
`tx_timer_internal_extension_ptr` (`TX_TIMER_INTERNAL_EXTENSION`,
`TX_THREAD_CREATE_TIMEOUT_SETUP`, `TX_THREAD_TIMEOUT_POINTER_SETUP`).

**The narrowing and its compensation were keyed on different conditions.** The
narrowing had already been re-keyed to the data model —

```c
/* Test the DATA MODEL rather than the architecture: LP64 needs the narrowing… */
#if (defined(__LP64__) && __LP64__) || (defined(__x86_64__) && __x86_64__)
```

— while the compensation block 340 lines below still read
`#if defined(__x86_64__) && __x86_64__`. On aarch64 the first fires and the
second does not, so the common fallback
`TX_ULONG_TO_THREAD_POINTER_CONVERT(timeout_input)` runs and truncates:

```
#0  _tx_thread_timeout ()      x19 = 0xaac85c70   <- upper 32 bits gone
=>  ldr w1, [x19, #124]                            (tx_thread_state)
```

That also explains the selectivity: the param only carries a pointer once a
thread takes a TIMED suspend, so CycloneDDS images (ddsrt's timed waits) died
and zenoh ones did not.

A half-fix, and the file says so — its own comment records "This port narrowed
them under `__x86_64__` only" for LONG/ULONG. Two sites were re-keyed; this
third was missed.

Fixed in the submodule (`9a29f1b`): same data-model test on the compensation
block.

## Verified

aarch64, all three binaries rebuilt:

| binary | before | after |
| --- | --- | --- |
| `c_talker` | rc=139 (SEGV) | rc=137 — alive at kill, `Publishing: 'Hello World: 11'` |
| `c_service_server` | rc=139 | rc=137 — alive at kill |
| `c_action_server` | rc=139 | rc=137 — alive at kill |

(137 is SIGKILL from the test's own `timeout`, i.e. still running.)

## Not pushed

The fix lives in the vendored `third-party/threadx` FORK. Per CLAUDE.md the
agent does not push fork remotes: the commit is made and the branch left ready,
and the maintainer pushes it, then bumps the superproject pointer to the pushed
commit.

## Not diagnosed

Which timer/thread the expiry walk trips over, and whether it is a lifetime bug
(an entity torn down while its timer is still linked) or a memory-layout one
(the byte-pool arena overlapping a `TX_THREAD`). The trio failing identically
suggests something in the shared board/RMW startup path rather than in any one
example.

## Addendum (2026-08-20) — the repro line does not stop the image, on any arch

Measured on x86_64 while re-checking this issue from a second session, before
the LP64 cause above was known. It is independent of the fix and of the
architecture, and it makes the `timeout 12` in the Symptom section misleading
for whoever reads this next.

**The Linux port blocks SIGTERM.** `_tx_linux_thread_init` builds its wait mask
as `sigfillset()` minus `RESUME_SIG`, and every suspended ThreadX thread sits in
`sigsuspend(&_tx_linux_thread_wait_mask)`
(`third-party/threadx/kernel/ports/linux/gnu/src/tx_initialize_low_level.c:402,438`),
so SIGTERM has no thread it can be delivered to. Measured: a manual `kill -TERM`
after 6 s of healthy publishing does not terminate the image within 90 s.

Plain `timeout 12` sends SIGTERM and never escalates, so it does not stop these
images at 12 s. **Use `timeout -s KILL`** — which is what the verification table
above already reports (`rc=137`). It is also why
`graceful_kill_process_group` spends its full 2 s SIGTERM grace on every
threadx-linux teardown before the SIGKILL fallback.

Two things ruled out in that same pass, recorded so they are not re-run:

* **A `TX_THREAD` ABI split across TUs.** The right neighbourhood — this board's
  `tx_user.h` adds `TX_THREAD_USER_EXTENSION` and defines `TX_64_BIT` — but not
  the defect. Crossing `ninja -t deps` with each object's `DEFINES` on the
  linked image: 199 TUs include `tx_api.h` and 199 carry
  `TX_INCLUDE_USER_DEFINE_FILE`. The disagreement was never between TUs; it was
  between two `#if` keys inside the port, 340 lines apart.
* **x86_64 as a reproduction host.** The narrowing fires on
  `__LP64__ || __x86_64__` and the compensation on `__x86_64__` alone, so on
  x86_64 both fire and the truncation cannot happen. Three binaries, many runs,
  under load average 45+ and under gdb: no fault, and all four
  `threadx_linux_cyclonedds` tests pass. **This bug is aarch64-only** — an
  x86_64 host cannot confirm or deny it, which is worth knowing before spending
  a session on it as I did.

---
id: 715
title: "Every threadx-linux CycloneDDS image SEGVs in `_tx_thread_timeout` from the ThreadX timer thread, seconds after reaching its ready banner"
status: open
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

## Not diagnosed

Which timer/thread the expiry walk trips over, and whether it is a lifetime bug
(an entity torn down while its timer is still linked) or a memory-layout one
(the byte-pool arena overlapping a `TX_THREAD`). The trio failing identically
suggests something in the shared board/RMW startup path rather than in any one
example.

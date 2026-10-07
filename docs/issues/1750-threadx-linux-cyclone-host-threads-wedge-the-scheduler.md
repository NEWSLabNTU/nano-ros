---
id: 1750
title: "A threadx-linux Cyclone image wedges its ThreadX scheduler within 2 s:
  Cyclone's host pthreads call the byte pool and leak `_tx_linux_mutex`"
status: open
type: bug
area: [platform, rmw]
severity: medium
found: 2026-10-08
related: [1741, 0968, phase-480]
---

## What was measured

Found while diagnosing issue 1741, on the prebuilt
`examples/threadx-linux/c/service-server/build-cyclonedds/c_service_server`
(and `c/talker/build-cyclonedds/c_talker`, same result), run under `gdb`
with NO signal sent, stopped 5 s after start:

    $1 = {__lock = 2, __count = 1700, __owner = <tid of "tev">, __kind = 1 /* recursive */}
         -- `p *(pthread_mutex_t*)&_tx_linux_mutex`
    _tx_linux_global_int_disabled_flag = 1
    _tx_thread_system_state            = 0

Every other thread is parked:

| thread | where |
| --- | --- |
| main (ThreadX scheduler) | `_tx_thread_schedule` → `pthread_mutex_lock(&_tx_linux_mutex)` |
| port timer ISR thread | `_tx_thread_context_save` → `pthread_mutex_lock(&_tx_linux_mutex)` |
| `dq.builtins` (Cyclone) | `ddsrt_malloc` → `nros_platform_alloc` → `_tx_byte_allocate` → `_tx_thread_interrupt_disable` → `pthread_mutex_lock(&_tx_linux_mutex)` |
| app thread (ThreadX) | `_tx_thread_sleep` → `_tx_thread_system_return` → `sem_wait` (never resumed) |
| `tev` (Cyclone) | `ddsrt_cond_waitfor` — idle, HOLDING `_tx_linux_mutex` 1700 deep |

So the whole kernel stops, permanently, before any request can be served.
The banner reaches `Waiting for service requests` because it prints before
the first spin.

## Why

Cyclone on threadx-linux runs its own threads (`gc`, `dq.*`, `tev`, `recv`)
as plain host pthreads — ddsrt's POSIX thread backend — and they allocate
through `ddsrt_malloc` → `nros_platform_alloc` → `tx_byte_allocate`. Every
ThreadX service brackets itself with `TX_DISABLE`/`TX_RESTORE`, which the
Linux port implements as `_tx_thread_interrupt_control()` in
`ports/linux/gnu/src/tx_thread_interrupt_control.c`: it LOCKS the recursive
`_tx_linux_mutex`, and releases it again only in one of two branches —
`_tx_thread_system_state != 0` (an ISR) or `_tx_thread_current_ptr != NULL`
(a ThreadX thread is the current one). A foreign pthread calling while no
ThreadX thread is current (`_tx_thread_current_ptr == NULL`, e.g. the app
thread asleep) takes NEITHER branch, so every call leaks one recursion level.
`tev` accumulated 1700 of them, and nothing else can ever take the mutex.

The port is not wrong to assume this: ThreadX services are callable from
ThreadX threads and ISRs, and a host pthread is neither.

## Probable relationship to 0968

Issue 0968's diagnosed threadx_linux Cyclone cluster
(`test_threadx_linux_cyclonedds_{service,action,talker_to_native_listener}`)
found that "discovery does not complete: the participant is never admitted",
with SPDP seen in both directions. A participant whose kernel is wedged
answers nothing after its first few announcements, which fits. NOT
established: that the wedge is the WHOLE of 0968's cluster — re-run those
three once this is fixed before closing either.

## Not established

- Whether every threadx-linux Cyclone image wedges, or how soon. Two images
  measured (C service-server, C talker), both wedged by 2 s; the C++ images
  and the Rust Cyclone path were not run.
- Whether zenoh-pico threadx-linux images can hit the same path. zenoh-pico's
  threads on this board are created through the ThreadX platform layer, so
  probably not, but no zenoh image was inspected under gdb.

## Shape of a fix

The rule to restore is "only ThreadX contexts call ThreadX services". Either
Cyclone's ddsrt threads become ThreadX threads on this board (a ddsrt thread
backend over `nros_platform_*` threads, as the Zephyr native sync backend did
for mutexes — RFC-level, touches the cyclonedds fork), or the allocator path a
foreign thread reaches stops entering ThreadX (e.g. a host-thread-safe pool in
front of the byte pool for non-ThreadX callers). The second is smaller but
leaves every other ThreadX service a foreign thread might reach.

Issue 1741's fix bounds the SYMPTOM — a wedged image now ends on SIGTERM
within 2 s — and does not touch this.

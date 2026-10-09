---
id: 1750
title: "A threadx-linux Cyclone image wedges its ThreadX scheduler within 2 s:
  Cyclone's host pthreads call the byte pool and leak `_tx_linux_mutex`"
status: resolved
type: bug
area: [platform, rmw]
severity: medium
found: 2026-10-08
related: [1741, 0968, 1237, 0832, 1769, phase-480]
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

## Resolution

Fixed at the platform seam (the issue's option b, widened to every service),
plus the board declining the one foreign path that has no correct answer. No
fork change was needed: a foreign thread cannot be MADE a legal ThreadX caller
(the byte pool records `_tx_thread_current_ptr` as its search owner, a mutex
needs an owner ThreadX can suspend), so the port is right and the platform has
to keep foreign callers out.

### Reproduced first, on `origin/main` (`685e55764f`)

Built `examples/threadx-linux/c/service-server` (cyclonedds) fresh in this
tree; under gdb, no signal, stopped at 5 s:

    _tx_linux_mutex: __lock 2, __count 1508, __owner <LWP of "tev">, __kind 1
    _tx_linux_global_int_disabled_flag 1, _tx_thread_system_state 0, current NULL

`dq.builtins` in `ddsrt_malloc → nros_platform_alloc → _tx_byte_allocate →
_tx_thread_interrupt_disable → pthread_mutex_lock`; scheduler, timer ISR and
app thread parked exactly as the table above says.

### The sweep — every ThreadX service a foreign thread reaches on this board

Two REACHABLE paths, both from Cyclone's host threads:

| path | how | fix |
| --- | --- | --- |
| heap | ddsrt funnel (issue 0832) → `nros_platform_{alloc,realloc,dealloc}` | two owners decided by ADDRESS (the pool is one range): a foreign caller gets `malloc`; a POOL block it frees goes on a lock-free list the next ThreadX context drains; `realloc` keeps a host block on the host heap |
| wake | Cyclone's participant `on_data_available` → runtime wake cb → `nros_platform_wake_signal` → `tx_semaphore_ceiling_put` | board declines the slot (`NROS_RMW_CYCLONEDDS_FOREIGN_WAKE OFF` in `cmake/board/nano-ros-board-threadx-linux.cmake`, issue 1237's rule) — the runtime then takes its documented poll-only path |

Every other ThreadX-calling entry of `nros-platform-threadx` now opens with one
guard from `src/threadx_context.h` (`nros_threadx_in_kernel_context()`: the
port's own thread-local `_tx_linux_threadx_thread`, the timer-ISR thread by
identity, or `tx_application_define`), compiled out on bare-metal ports:

- clock → foreign callers read `_tx_timer_system_clock` directly;
- heap stats → a direct field read;
- wake / condvar SIGNAL → refused (-1) with one stderr line naming the function
  and this issue — a lost wake costs the waiter its bound, never liveness;
- mutex, condvar/wake wait and park, sleep, yield, task, critical section,
  timer → FATAL with that line. There is no correct answer to them from a
  thread ThreadX cannot suspend, and carrying on would be a silent race.

Sweep command: `grep -n 'tx_[a-z_]*(' packages/platform/nros-platform-threadx/src/*.c`
(every hit is inside a guarded function, the heap's kernel-context branch, or
`net.c`). NOT guarded: `net.c` (NetX BSD over the nsos shim) — no foreign
thread reaches it (Cyclone uses ddsrt's own host sockets); the board's
`nros_threadx_alloc`/`_free`/tier spawning in `threadx_hooks.c` — called only
from ThreadX contexts.

The 1741 termination guard thread is itself a foreign host thread and calls the
app's signal handler; the in-tree handlers only reach `nros_executor_cancel`
(atomics), and a handler that entered ThreadX would now abort with a named line
instead of wedging.

### After, measured

- gdb, same image, 8 s: `_tx_linux_mutex` `__lock 0, __count 0, __owner 0`;
  scheduler in `nanosleep`, app thread in its normal `wake_wait_ms`. The C++
  service-server: same, `__count 0`.
- live request: native C client → threadx-linux C server,
  `Result of add_two_ints: 5`, `Total requests handled: 1`, empty stderr.
- issue 0968's cluster, solo, fixtures built from this tree (C and C++
  threadx-linux cyclonedds rows + native linux C/C++ cyclonedds peers):

  | cell | pre-fix | post-fix |
  | --- | --- | --- |
  | `test_threadx_linux_cyclonedds_service` | FAIL (no roundtrip, 32.8 s) | PASS 1.9 s |
  | `test_threadx_linux_cyclonedds_action` | FAIL (no result, 13.5 s) | PASS 10.2 s |
  | `test_threadx_linux_cyclonedds_talker_to_native_listener` | FAIL (0 samples) | PASS 3.1 s |
  | `test_threadx_linux_cyclonedds_cpp_talker_to_native_listener` | not run | PASS 2.7 s |

### Regression test

The four threadx-linux Cyclone cells now drain the threadx image's whole output
and fail if `nros_tests::output::THREADX_FOREIGN_THREAD_REFUSAL` printed
(`assert_threadx_image_kept_foreign_threads_out`, `tests/native_api.rs`). Two
negative controls, measured:

- pre-fix tree → service, action and C talker FAIL (the wedge);
- heap fixed but the Cyclone wake left ON → service and action still PASS the
  roundtrip on latency alone, print
  `nros: nros_platform_wake_signal() called from a host thread ThreadX does not own - refused`,
  and FAIL the new assertion. The talker passes either way (no subscription,
  so no data-available).

### Not measured / not established

- zenoh-pico threadx-linux images: established FROM CODE, not run — every
  zenoh task is `tx_thread_create`d (`zpico-sys/c/platform/threadx/task.c`),
  and the only other host thread in a threadx-linux image is 1741's termination
  guard. If that ever changes, the guard now names the call instead of wedging.
- Rust threadx-linux Cyclone: there is no image — `rmw-cyclonedds = []` is an
  empty feature in `examples/threadx-linux/rust/*` and no fixture row exists.
- The foreign threads' host-heap allocations are NOT counted by
  `nros_platform_heap_used_bytes` (which reports the byte pool).
- A ThreadX thread now `free()`s a host block Cyclone allocated. That is the
  same libc-heap use the board already had (C stdio, Rust std images); it is
  not new exposure, and it was not separately measured.
- The sibling on `freertos-posix` (heap_3's scheduler-suspension bracket called
  from Cyclone's threads) is filed as issue 1769, unmeasured.

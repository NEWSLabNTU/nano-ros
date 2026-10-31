---
id: 2188
title: "Every Cyclone image on Zephyr 4.4 aborts ~2 ms after boot: a timed k_condvar_wait returns with the mutex unlocked"
status: resolved
resolved_in: "2026-11-01"
type: bug
area: [zephyr, cyclonedds, rmw]
severity: high
found: 2026-11-01
related: [2187, 0496]
---

## What was measured

There are two defects. The first one hid the second.

**1. The 4.4 lane never built a Cyclone row.** `scripts/build/zephyr-fixture-leaves.sh`
kept `fixture_rmws=(zenoh)` on 4.4, "pending cyclonedds-on-4.4" (180.A,
2026-05-26). Phase 180 then proved Cyclone on 4.4, and the gate stayed. Every
4.4 run dropped the 12 Cyclone rows without a word, while the test resolver still
counted them in-lane. In the issue-2187 sweep, 23 tests failed `MISSING` (the
rows plus three west fixtures that a failed leaf stage never reached).

**2. With the rows built, every Cyclone image aborts.** Before it aborts, the
build needs the `cyclone-idl` Python group in the 4.4 venv. `msg_to_cyclone_idl.py`
runs under that venv's Python 3.12, and ROS Humble's `rosidl_adapter` is built
for 3.10. Once the images build, each one aborts at its first spin:

```
ddsrt: k_mutex_unlock failed (-22)
abort()
>>> ZEPHYR FATAL ERROR 4: Kernel panic on CPU 0
```

That is 9 example_e2e cases plus `boot_smoke::case_17_cyclonedds_rust_talker_boots`.
3.7 passes the same cases with the same runtime checks enabled
(`CONFIG_RUNTIME_ERROR_CHECKS=y`, no `NO_RUNTIME_CHECKS`).

## Cause

gdb breakpoint on `sync_fatal`, `build-c-listener-cyclonedds` on 4.4:

```
#1 ddsrt_mutex_unlock (mutex=…)            ddsrt/src/sync/zephyr/sync.c:84
#2 gcreq_queue_thread (q=…)                core/ddsi/src/q_gc.c:146
```

`q_gc.c:146` unlocks `q->lock` right after
`ddsrt_cond_waitfor(&q->cond, &q->lock, to)`. Zephyr's `kernel/condvar.c`
differs between the lines:

- **3.7**: `k_mutex_unlock; z_pend_curr(…); k_mutex_lock(mutex, K_FOREVER)`.
  It always takes the mutex back, as POSIX does.
- **4.4**: it returns early for `K_NO_WAIT` with the mutex released, and takes
  the mutex back only `if (ret == 0)`. A timeout (`-EAGAIN`) returns UNLOCKED.

The issue-0496 native ddsrt backend (`k_mutex`/`k_condvar`, which replaces the
pooled pthreads) assumes the POSIX contract. The GC thread's periodic timed wait
times out, and the unlock that follows lands on a mutex nobody owns:
`-EINVAL` (`owner == NULL`). The issue-180.A
`pthread-mutex-unlock-4.4.patch` cannot help any more, because Cyclone no longer
uses pthread mutexes on Zephyr.

## Resolution

- **cyclonedds fork, branch `nano-ros`, commit `404fc5d9`** (a fast-forward
  from `8665a51f`; the superproject pin moves to it in the same change): `ddsrt_cond_waituntil` takes the mutex back after `k_condvar_wait`
  when `mutex->mutex.owner != k_current_get()`. It is conditional because
  `k_mutex` is recursive: an unconditional lock on 3.7, which has already taken
  the mutex back, would leave it held after the caller's unlock.
- `zephyr-fixture-leaves.sh`: 4.4 gets the same Cyclone rule as 3.7 (build the
  rows when a host idlc resolves, and report them when it does not).
- `provision-py312-venv.sh`: `--create` installs the `cyclone-idl` group, and the
  check WARNS when it is missing.

These land together. Lifting the gate without
the fix would only swap 23 MISSING reds for 10 aborts.

## Measured

- `case_11_cyclonedds_c_pubsub_e2e`: aborted on 4.4 before the fix; passes on
  4.4 and on 3.7 after it (A/B on the same build dirs).
- All 12 Cyclone rows build on both lines. `nros-tests::zephyr` filtered to
  `/cyclonedds/`: **4.4: 19/19**, **3.7: 19/19**.
- On 4.4, `case_10_cyclonedds_rust_pubsub_e2e` failed once and passed on retry.
  Delivery had worked. The heap-headroom gate read the talker's boot report
  as magic `0x00000000` ("never reached `boot_report::init()`"). That was seen
  once and is not chased here.

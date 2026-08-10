---
id: 371
title: native_sim cyclone app abort()s ~19-21 s into an Autoware-graph session during MRM/service churn
status: resolved
type: bug
severity: high
area: rmw
related: [issue-0267, issue-0496]
---

# 0371 — native_sim cyclone app abort()s ~19–21 s into an Autoware-graph session

**Status:** Resolved 2026-08-10 — root cause below; the ddsrt half landed as
cyclonedds@5b87ee52, pinned by nano-ros fa4c8bd15.
**Filed:** 2026-08-01
**Affects:** `nros-rmw-cyclonedds` on Zephyr native_sim joining a large
(~40-participant / 68-composable) stock ROS 2 Humble graph
(simple-autoware-safety-island direct-connection demo)

## ROOT CAUSE (2026-08-10): the Zephyr POSIX pthread **mutex pool** is exhausted

`CONFIG_MAX_PTHREAD_MUTEX_COUNT` runs out mid-SEDP-ingestion, and every layer
above it swallows the failure, so the process dies ~20 s later in an unrelated
function with no message.

The chain, each link verified under gdb:

1. Zephyr's `pthread_mutex_init` (`lib/posix/options/mutex.c:214`) allocates a
   slot from a fixed `SYS_BITARRAY` pool. On exhaustion it leaves
   `*mu = PTHREAD_MUTEX_INITIALIZER` (`-1`) and returns `ENOMEM`.
2. CycloneDDS's POSIX `ddsrt_mutex_init` (`src/ddsrt/src/sync/posix/sync.c:23`)
   **discards that return value**. The caller believes the mutex exists.
3. The first `ddsrt_mutex_lock` on it reaches `acquire_mutex` → `to_posix_mutex`,
   which retries the pool alloc for a `-1` handle, fails again, and returns
   `EINVAL`.
4. `ddsrt_mutex_lock` treats any non-zero return as `abort()` — one of **13 bare
   `abort()` calls** in that file, none of which log anything.

So the observable is a bare `abort()` → `ZEPHYR FATAL ERROR 4` in an unnamed
cyclone pthread, arbitrarily far from the actual resource failure.

### The measurement

gdb on the abort site, decoding the handle and counting the pool bitarrays:

```
***** ddsrt_mutex_lock: pthread_mutex_lock FAILED *****
raw=0xffffffff marked_initialized=True index=2147483647
  -> PTHREAD_MUTEX_INITIALIZER (-1): the pool alloc failed
posix_mutex_bitarray: 16384 / 16384 allocated      <-- FULL
posix_cond_bitarray:    250 / 16384 allocated
#1  ddsrt_mutex_lock (mutex=0x99a150)  sync.c:42
#2  ddsi_new_proxy_reader (...)        ddsi_proxy_endpoint.c:584
#3  handle_sedp_alive_endpoint (...)   q_ddsi_discovery.c:1727
#4  handle_sedp (...)                  q_ddsi_discovery.c:1863
#5  builtins_dqueue_handler (...)      q_ddsi_discovery.c:2109
#6  dqueue_thread (...)                q_radmin.c:2552
```

A second capture aborted one frame over, on the addrset created inside
`addrset_from_locatorlists` (`q_addrset.c:484` ← `q_ddsi_discovery.c:246`) —
same mechanism, different first-lock.

The **mutex pool is at 100 %, the cond pool at 1.5 %**. Only mutexes are
scarce; the two are not consumed in pairs.

### Why removing one Autoware node "fixed" it

It is a threshold on the **remote** endpoint count, not a property of that node.
Cyclone takes one pool mutex per proxy entity *and* one per addrset, so demand
scales with the graph the island joins, not with the island's own 4 nodes.
Shadowing `autoware_manual_lane_change_handler` removes enough endpoints to keep
peak demand just under 16384 — which is also why the 07-31/08-01 flip tracked
whether that node had crashed at startup (sim 32/33 vs 33/33). Nothing about
that node's service or transient-local endpoints is special.

### Corrections to the original write-up

- **"256 MiB arena + 16384 mutex/cond pools did NOT absorb"** — the arena was
  never the problem, and the *cond* pool was never close. The mutex pool was
  exactly the limit, hit exactly.
- **"under `gdb -batch -ex run` the scenario runs to completion — the debugger
  masks it"** — a plain `break abort` does *not* mask it; the abort reproduced
  under gdb on the first try. What masks it is a breakpoint on a hot path
  (a breakpoint on every `pthread_mutex_init` — ~7000 hits — slows SEDP
  ingestion enough that the pool never fills).
- The earlier suspect list (service req/rep churn, SEDP proxy churn from the
  scenario's subscription-per-poll) is not implicated. The scenario only matters
  because it keeps the island alive long enough to finish ingesting the graph.

## Reproduction (2026-08-10, nano-ros `42c720958`)

Still reproduces on current main — cyclone's RMW sources have not changed since
the `eee004fce` pin (only build-side commits), and the vendored cyclonedds
pointer is unmoved.

```sh
# simple-autoware-safety-island, autoware_manual_lane_change_handler un-shadowed
just zephyr-build && just demo-all
```

→ `abort()` at 18.758 s, first attempt. `tmp_island.log`:

```
abort()
@ WEST_TOPDIR/zephyr/lib/libc/common/source/stdlib/abort.c:13
[00:00:18.758,006] <err> os: >>> ZEPHYR FATAL ERROR 4: Kernel panic on CPU 0
[00:00:18.758,006] <err> os: Current thread: 0x6f3b78 (unknown)
```

The gdb probe used is in the demo repo at `tmp/abort-probe.gdb` (decodes the
`pthread_mutex_t` handle and popcounts `posix_mutex_bitarray` /
`posix_cond_bitarray`).

## Fix

**Demo side (done):** raise `CONFIG_MAX_PTHREAD_MUTEX_COUNT` to 131072 in
`src/zephyr_entry/prj-cyclonedds.conf`. The cond pool stays at 16384.

Verified in simple-autoware-safety-island with the workaround removed (the full
33/33 graph, `autoware_manual_lane_change_handler` present):

```
VERDICT: PASS — island stopped the vehicle (4.25 -> 0.00 m/s),
                MRM recovered, vehicle resumed (1.39 m/s)
island aborts: 0
```

A preceding run survived 140 s with zero aborts but scored FAIL because the
initial `ChangeOperationMode` call returned `success=False` for all 8 of the
scenario's attempts; the next run engaged on attempt 1. That is scenario/ADAPI
timing, unrelated to this issue — in both runs the island stayed up, operated
MRM and recovered.

**nano-ros side (landed) — the diagnosability defect was the real bug here.**
A resource ceiling is a legitimate thing to hit; dying anonymously 20 seconds
later in an unrelated function is not. `cyclonedds@5b87ee52` routes all three
init entry points in `src/ddsrt/src/sync/posix/sync.c` through one
`sync_init_failed()` helper that names the object and, under Zephyr, the
Kconfig knob to raise. `ddsrt_cond_init` had the same discarded return as the
mutex one; `ddsrt_rwlock_init` checked but aborted silently. Swept the sibling
ports: freertos and threadx already check every init return, so the
discarded-return class is closed.

Note the pin gap this exposed: `4c8ff8c2` ("fail loudly when pthread_mutex_init
fails") had been on the fork branch since 2026-07-17, but the superproject pin
predated it, so the image that hit this issue still had the fully-silent path.
Three fork commits were unpinned; `fa4c8bd15` advances the pin over all four.

## Residual → filed as issue 0496

131072 slots is ~4.1 MiB of static RAM, which native_sim can afford and a real
board cannot. Cyclone putting a pool mutex on every addrset is the underlying
scalability limit for cyclone-on-Zephyr-POSIX against large graphs: it makes the
joinable graph size a compile-time constant scaling with the REMOTE endpoint
count. Filed as **issue 0496** with the measurements. (The ~5 MiB figure quoted
in the commits that closed this issue was an estimate; measured is 4 MiB of pool
+ 128 KiB type array + 16 KiB bitarray.)

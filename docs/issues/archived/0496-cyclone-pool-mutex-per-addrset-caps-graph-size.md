---
id: 496
title: Cyclone takes a pthread pool mutex per addrset, so joinable graph size is bounded by static RAM
status: resolved
type: tech-debt
severity: medium
area: rmw
related: [issue-0371]
---

# 0496 — a pool mutex per addrset bounds the graph a Zephyr image can join

**Status:** Resolved 2026-08-10 — striped locks landed as cyclonedds@942dda3c
**Filed:** 2026-08-10
**Affects:** `nros-rmw-cyclonedds` on any Zephyr target (POSIX pthread pools)
**Split from:** issue 0371, which was the crash this causes. 0371 is resolved —
the ceiling was raised and the failure now reports itself. This issue is the
sizing rule that remains.

## The problem

CycloneDDS puts a mutex in every `addrset`:

```c
struct addrset {
  ddsrt_mutex_t lock;              /* q_addrset.h:31 */
  ddsrt_atomic_uint32_t refc;
  ddsrt_avl_ctree_t ucaddrs, mcaddrs;
};

struct addrset *new_addrset (void)
{
  struct addrset *as = ddsrt_malloc (sizeof (*as));
  ddsrt_atomic_st32 (&as->refc, 1);
  ddsrt_mutex_init (&as->lock);    /* q_addrset.c:174 */
  ...
```

On Zephyr `ddsrt_mutex_t` is a `pthread_mutex_t`, and Zephyr's POSIX layer
allocates those from a **fixed static pool** sized by
`CONFIG_MAX_PTHREAD_MUTEX_COUNT` — `static struct k_mutex
posix_mutex_pool[CONFIG_MAX_PTHREAD_MUTEX_COUNT]` plus a type byte and a
bitarray bit each (`zephyr/lib/posix/options/mutex.c`).

Cyclone creates an addrset per proxy entity (and transiently per SEDP
announcement), so **pool demand scales with the size of the remote graph, not
with what the local image declares.** A 4-node safety island needs a pool sized
for Autoware, not for its own four nodes. There is no backpressure: the pool is
a compile-time constant, so "how big a graph can this image join" becomes a
Kconfig value someone has to guess.

## Measurements (2026-08-10, native_sim, Zephyr 3.7)

Joining the simple-autoware-safety-island demo graph — stock ROS 2 Humble
Autoware, 33 nodes / 13 containers / 68 composable, ~40 participants:

| Quantity | Value |
| --- | --- |
| `CONFIG_MAX_PTHREAD_MUTEX_COUNT` that **fails** | 16384 (exhausted ~19 s in → issue 0371) |
| `CONFIG_MAX_PTHREAD_COND_COUNT` in use at that point | ~250 |
| `CONFIG_MAX_PTHREAD_MUTEX_COUNT` that **works** | 131072 (full demo reaches `VERDICT: PASS`) |
| `sizeof(struct k_mutex)` | 32 B |
| `sizeof(posix_mutex_pool)` at 131072 | 4 MiB |

So the working configuration spends **~4.1 MiB of static RAM** (4 MiB pool +
128 KiB type array + 16 KiB bitarray) on mutexes, essentially all of it for
addrsets and proxy entities belonging to *other* processes. native_sim can
afford that; no real board in this project's matrix can.

The mutex:cond ratio is the tell — **~65:1**. Cyclone pairs a mutex with a
condvar for things like waitsets and queues, so a ratio like that says the bulk
of the mutexes belong to objects that have no condvar, i.e. addrsets and entity
locks.

**Steady-state demand is NOT measured** — only bracketed as
`16384 < peak ≤ 131072`. Getting the exact figure would size the knob properly
and show whether it plateaus (steady-state demand) or creeps (a leak), which is
the one thing these numbers cannot distinguish. Two dead ends, recorded so the
next attempt skips them:

- **Sampling requires being stopped in a frame that belongs to the Zephyr
  image.** These globals exist in two objfiles — the native_simulator runner
  links the kernel image in, and `break abort` resolves to "2 locations" — so a
  context-free `print posix_mutex_bitarray` silently resolves to the copy that
  is never written and reads a plausible-looking `0 / 131072`. A sample taken at
  a breakpoint in cyclone code reads the real one.
- **`gdb -p` attach then `continue` hangs** on a running native_sim process; its
  scheduler does not tolerate gdb stopping arbitrary threads. Launching the
  image under gdb works, but then there is no obvious way to sample on a timer
  in batch mode — `run &` + `interrupt` does not stop synchronously
  ("Cannot execute this command while the selected thread is running").

The workable shape is probably a breakpoint whose ignore-count is set high
enough to fire late, or a build with a small periodic hook that prints the
occupancy itself.

## Why it is worth fixing rather than just sizing

The addrset lock guards two AVL trees that are, in the overwhelming majority of
cases, **written once at construction and read-only afterwards** — an addrset is
built by `addrset_from_locatorlists` / `copy_addrset_into_addrset_*` and then
refcounted and shared. A per-object OS mutex for that is heavy on a pooled
POSIX implementation.

Options, roughly in order of how much they buy:

1. **Do not give each addrset its own OS mutex.** A shared lock striped over a
   small fixed array (hash the addrset pointer), or a single global addrset
   lock, would drop this from O(remote endpoints) to O(1). Contention is
   plausibly fine — these are short read critical sections.
2. **Make the immutability explicit** and drop the lock where an addrset is
   already published as read-only, keeping it only for the mutable construction
   window.
3. **Zephyr-side:** nothing to do — the POSIX mutex implementation is pool-only,
   there is no heap-backed mode to opt into.
4. **nano-ros-side, cheap mitigation:** state the sizing rule where someone will
   find it (pool ≥ f(remote endpoint count), not f(local nodes)) and consider a
   startup log line reporting pool headroom, so an image that is close to the
   edge says so before it hits the wall.

**Not fixed upstream.** CycloneDDS master as of `5e82de60` (2026-05-19) still
does `ddsrt_mutex_init (&as->lock)` in `ddsi_new_addrset` — the type became
opaque but the per-object mutex is unchanged. So (1) or (2) is a change to carry
in the fork and offer upstream, not a version bump to wait for.

## Fix (landed): option 1, striped locks — `cyclonedds@942dda3c`

64 stripes keyed on the addrset address, so the whole domain uses 64 mutexes
regardless of graph size. Pinned by nano-ros; the safety island's
`CONFIG_MAX_PTHREAD_MUTEX_COUNT` went from 131072 back to **16384 — the value
that used to exhaust** — and the full demo passes.

Sharing a mutex between addrsets is only safe if nothing holds two addrset
locks, or one across a callback, since a same-thread re-acquire on a
non-recursive mutex is a deadlock rather than a wait. Two places did, and both
had to be restructured; this, not the striping, was the actual work:

- `copy_addrset_into_addrset_{uc,mc,no_ssm_mc}` locked the source and let the
  per-locator add lock the destination. Now takes both stripes for the whole
  walk, ordered by stripe address and collapsed to one acquire when they
  collide, calling a new `add_xlocator_to_addrset_locked`. Side benefit: the
  copy is now atomic, which it was not.
- `addrset_forall*` ran the caller's callback under the lock. At least one
  callback re-enters this layer on the same thread — `purge_helper` deletes a
  proxy participant, and writing that participant's builtin-topic sample calls
  `addrset_forall` again. They now snapshot the locators under the lock and run
  the callback after releasing it (stack buffer for the usual handful, heap only
  for an outsized addrset).

`addrset_eq_onesidederr` already used `TRYLOCK` and documents a failed acquire
as "not equal"; same-stripe now takes that path, which is a tolerated false
negative rather than a hang (trylock of a self-held mutex returns EBUSY on both
glibc and Zephyr). `addrset_forone` never took a lock and is untouched.

### What remains, and the honest limit of the result

The pool still scales with the graph, just with a much smaller constant: the
**per-proxy-entity** locks are untouched, and 2048 slots still exhausts on this
graph. So this removes one term, not the dependence. It bought roughly an 8×
reduction in the required pool (131072 → 16384) on a ~40-participant graph.

### Regression cover (added after the fact)

The first version of this fix shipped with only a clean native suite and one
end-to-end run behind it — no targeted test of either restructured path, which
for a deadlock hazard is thin. `nros_rmw_cyclonedds_addrset_striped_lock_concurrency`
(`packages/rmw/cyclonedds/nros-rmw-cyclonedds/tests/`) now covers both.

It drives 128 addrsets — comfortably more than the 64 stripes, so collisions are
certain by pigeonhole without the test needing to know the hash — across 8
threads: bidirectional copies at every offset, self-copy (the degenerate
same-stripe case), a `purge_helper`-shaped callback that locks a second addrset,
and a callback that runs a nested `forall`. Plus semantic checks that the
snapshot rewrite did not change what `forall_count` reports or break the copy
union.

Two things about it worth keeping:

- **The watchdog distinguishes stuck from slow.** A hang is the failure mode, so
  a phase that misses its deadline is not enough information; the workers bump a
  progress counter and the watchdog reports whether it advanced. That mattered
  immediately: the first run "hung", and it was the test's own fault — the copy
  phases union every addrset into every other, so leaving that state in place
  made the nested-forall phase quadratic in the union (~10^8 inner callbacks).
  Each phase now builds a fresh pool.
- **It was validated by mutation, not by passing.** Reverting
  `copy_addrset_into_addrset_uc` to lock only the source hangs the self-copy
  phase; making `addrset_forall_count` run the callback under the lock hangs the
  re-entrant-callback phase. Both reported STUCK and exited 1, on the phase
  aimed at them. A concurrency test that has never been seen to fail is not
  evidence of anything.

**Still not fixed upstream.** CycloneDDS master as of `5e82de60` (2026-05-19)
does `ddsrt_mutex_init (&as->lock)` in `ddsi_new_addrset`. Worth offering.

---
id: 507
title: The cyclonedds fork carries two nano-ros-only lock changes that upstream lacks
status: open
type: tech-debt
severity: low
area: rmw
related: [issue-0371, issue-0496]
---

# 0507 — two nano-ros-only changes in the cyclonedds fork, not offered upstream

**Status:** Open
**Filed:** 2026-08-10
**Affects:** `third-party/dds/cyclonedds` (branch `nano-ros`)

## What diverges

Closing issue 0496 added two changes to the vendored fork that upstream
CycloneDDS does not have:

1. **Striped addrset locks** — `cyclonedds@942dda3c`. `struct addrset` no longer
   carries its own `ddsrt_mutex_t`; the lock comes from a 64-entry stripe array
   keyed on the addrset address. Required restructuring
   `copy_addrset_into_addrset_*` (holds both stripes, ordered) and
   `addrset_forall_*` (snapshots, so the callback runs unlocked).
2. **A Zephyr-native ddsrt sync backend** — `cyclonedds@a09babf3`.
   `ddsrt_mutex_t` / `ddsrt_cond_t` become embedded `struct k_mutex` /
   `struct k_condvar` instead of handles into Zephyr's fixed
   `CONFIG_MAX_PTHREAD_{MUTEX,COND}_COUNT` pools. New files
   `ddsrt/sync/zephyr.h` + `src/sync/zephyr/sync.c`, selected by
   `DDSRT_WITH_ZEPHYR` from the consumer's `config.h`.

Upstream master as of `5e82de60` (2026-05-19) still does
`ddsrt_mutex_init (&as->lock)` in `ddsi_new_addrset` and has no Zephyr sync
backend, so neither is a "wait for the next release" situation.

## Why it is worth an issue rather than a shrug

Both touch code paths upstream changes freely — the addrset locking discipline
and the ddsrt platform layer. Every future rebase onto upstream has to
re-establish that:

- nothing new holds two addrset locks at once, or one across a callback (the
  striping makes either a deadlock; `nros_rmw_cyclonedds_addrset_striped_lock_concurrency`
  is the regression cover, and it is mutation-validated, so it will actually
  catch a regression rather than pass quietly);
- no new ddsrt sync entry point appears that the Zephyr backend does not
  implement (a missing one is a link error, so this half is self-enforcing).

Offering both upstream converts that recurring cost into a one-time review. The
Zephyr backend is the easier sell — it is additive and behind a config switch.
The addrset striping is the more valuable one for upstream: it is not
Zephyr-specific, it removes an allocation per addrset on every platform, and the
two nesting fixes it forced are arguably correctness improvements in their own
right (the copy became atomic, and callbacks no longer run under the lock).

For an upstream PR the Zephyr backend also needs a `WITH_ZEPHYR` option in
cyclone's own `src/ddsrt/CMakeLists.txt`. nano-ros never needed one: the Zephyr
build lists ddsrt sources manually in `zephyr/cmake/nros_rmw_cyclonedds.cmake`.

## Known-and-intentional, not part of this

`ddsrt_rwlock_t` and `ddsrt_once_t` stay on pthreads under the Zephyr backend.
Cyclone creates exactly one rwlock in production code (the log sink in
`ddsrt/src/log.c`) so it is not a scaling term, and `pthread_once_t` is
caller-owned rather than pooled. The consequence is only that
`CONFIG_MAX_PTHREAD_MUTEX_COUNT` cannot go to zero — 256 is ample.

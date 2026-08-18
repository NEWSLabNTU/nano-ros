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


## Census 2026-08-15 (phase-355 W2) — it is FIFTEEN commits, not two

This issue was written about the two changes issue 0496 added. Measured against
the release line the fork is based on, `origin/releases/0.10.x..fork/nano-ros`
is **15 commits**. The two named here are the newest, not the whole debt:

| # | commit | change | diffstat |
| --- | --- | --- | --- |
| 1 | `942dda3c` | ddsi/addrset: stripe the lock instead of one mutex per addrset | 167+/51- |
| 2 | `a09babf3` | ddsrt: Zephyr-native sync backend (`k_mutex`/`k_condvar`) | 301+ |
| 3 | `8601ca66` | freertos/threadx sync ports say what failed before aborting | 63+/13- |
| 4 | `5b87ee52` | ddsrt/posix: say which pool ran out; fix cond/rwlock too | 45+/7- |
| 5 | `4c8ff8c2` | ddsrt/posix: fail loudly when `pthread_mutex_init` fails | 6+/1- |
| 6 | `1d794c0a` | ddsi_udp: Zephyr multicast join via `struct ip_mreqn` | 23+ |
| 7 | `4aa337b0` | q_sockwaitset: AF_UNIX socketpair self-pipe on Zephyr | 13+ |
| 8 | `290152c0` | zephyr: tolerate NSOS socket gaps | 182+/13- |
| 9 | `22150fbf` | freertos: avoid TLS-only DDS state | 78+/13- |
| 10 | `902f7707` | ddsrt: add ThreadX NetX port | 1231+/14- |
| 11-15 | `56e6170a` `e8ce7315` `5558c6ae` `12b4af2c` `6eb92277` | ThreadX port follow-ups | 220+/83- |

A correction to my own reading on the way: `git branch -a --contains` reported
none of these on a fork branch, which looks like "the superproject records a
commit no one can fetch". It was **stale remote-tracking refs** — after
`git fetch fork --prune` all three are on `fork/nano-ros`, and
`git fetch fork <sha>` served it directly. Nothing is unpushed. Recorded because
the false alarm is one `git fetch` away from being believed.

## The decision W2 asks for

Grouped by what upstream would actually be asked to take. PR submission is NOT
done here: CLAUDE.md's vendored-fork rule keeps fork-remote pushes with the
maintainer, and opening a PR against `eclipse-cyclonedds` is an outward-facing
act that is theirs to make. What this records is the decision and the reason, so
"still carrying it" stops being the default.

### OFFER UPSTREAM — not platform-specific, useful to everyone

* **`942dda3c` striped addrset locks.** The strongest candidate, as this issue
  already argued: not Zephyr-specific, removes an allocation per addrset on
  every platform, and the two nesting changes it forced (the copy became atomic;
  callbacks no longer run under the lock) are correctness improvements in their
  own right. Regression cover exists and is mutation-validated
  (`nros_rmw_cyclonedds_addrset_striped_lock_concurrency`).
* **`5b87ee52` + `4c8ff8c2` + `8601ca66` diagnostics.** ~110 lines total whose
  entire content is "fail loudly, and say which pool ran out". They fix the
  failure mode this project hit repeatedly — an anonymous `abort()` 20 s into a
  40-participant graph (issues 0371/0496). Nothing about them is nano-ros
  specific, and a maintainer reviewing them needs no embedded context.

### OFFER UPSTREAM — additive, behind a switch

* **`a09babf3` Zephyr sync backend.** Additive and selected by
  `DDSRT_WITH_ZEPHYR`, so it cannot regress an existing platform. Needs one
  thing nano-ros never did: a `WITH_ZEPHYR` option in cyclone's own
  `src/ddsrt/CMakeLists.txt`, because the Zephyr build lists ddsrt sources
  manually in `zephyr/cmake/nros_rmw_cyclonedds.cmake`.
* **`1d794c0a`, `4aa337b0`, `290152c0` Zephyr socket-layer fixes.** Portability
  fixes against Zephyr's NSOS quirks. Individually small and independently
  defensible.

### PERMANENTLY OURS — offered only if upstream asks

* **`902f7707` + its five follow-ups — the ThreadX NetX port** (~1450 lines
  across 21 files). This is a new platform port, not a fix. Upstream carries
  ports for the platforms it supports and takes on their maintenance; proposing
  one it has no CI for, no users asking for, and no way to test is asking a
  maintainer to adopt a burden. Keeping it ours is the honest arrangement — and
  it is *why* the rebase cost this issue worries about exists, so it should be
  stated rather than hoped away.
* **`22150fbf` FreeRTOS TLS-only state.** Same reasoning, smaller.

### What this changes about the rebase cost

Ten of the fifteen are offerable; five are ours by choice. If the offerable ten
land upstream, every future rebase carries a ThreadX port and one FreeRTOS fix
rather than fifteen patches across the addrset locking discipline and the whole
ddsrt platform layer — which is the recurring cost this issue was opened to
bound.

## Known-and-intentional, not part of this

`ddsrt_rwlock_t` and `ddsrt_once_t` stay on pthreads under the Zephyr backend.
Cyclone creates exactly one rwlock in production code (the log sink in
`ddsrt/src/log.c`) so it is not a scaling term, and `pthread_once_t` is
caller-owned rather than pooled. The consequence is only that
`CONFIG_MAX_PTHREAD_MUTEX_COUNT` cannot go to zero — 256 is ample.


## Verified 2026-08-19 — the census holds; PRs deferred, fork maintained

Re-measured against the fork. Every claim above is still true and the debt has
not grown:

| claim | result |
| --- | --- |
| 15 nano-ros-only commits | **exact** — matches the table 1:1 |
| fork tip vs superproject gitlink | **identical** (`8601ca66`) — nothing unpushed, nothing drifted |
| upstream still `ddsrt_mutex_init (&as->lock)` | **true** — `5e82de60:src/core/ddsi/src/ddsi_addrset.c:86` |
| upstream has no Zephyr sync backend | **true** — its `src/ddsrt/src/sync/` is freertos, posix, windows |

Upstream DOES carry `ports/zephyr/*` (build + board integration, not a ddsrt
sync backend), which strengthens rather than weakens the "additive, behind
`DDSRT_WITH_ZEPHYR`" case: there is already a place for it to sit.

### DECISION — upstream PRs are future work

Submission is deferred; the fork is maintained as-is. The grouping above stays
as the record of what WOULD be offered, so the decision is retrievable when
someone picks it up, rather than re-derived from scratch. This is a deliberate
"carry it" — not the default drift the issue was opened to stop.

### Two corrections, both of which cost time to rediscover

**The census recipe names a ref that no longer exists.** It cites
`origin/releases/0.10.x..fork/nano-ros`, but the fork now carries only `master`
(`5e82de60`) and `nano-ros` (`8601ca66`). Counting `master..nano-ros` gives
**58**, not 15, because it sweeps in upstream 0.10.x release commits (version
bumps, iceoryx fixes, deserializer patches) that are not ours.

Two recipes that do not depend on a vanished ref, cross-checking each other:

```sh
# by boundary: 5041f356 is the newest upstream 0.10.x commit our stack sits on
git log --oneline 5041f356..nano-ros          # -> 15

# by authorship, as an independent check
git log --oneline --author=jerry73204 5e82de60..nano-ros   # -> 15
```

**The submodule checkout is SHALLOW**, and that reproduces this issue's own
false alarm. With the default clone, `git rev-list --count HEAD` is 8 and
`942dda3c`, `902f7707`, `12b4af2c` all report as absent objects — which reads
exactly like "the superproject records commits nobody can fetch". It is not:
`git fetch --unshallow origin` restores 2375 commits and every one resolves. The
correction paragraph above warned about stale remote-tracking refs; this is the
same trap by a different mechanism, so verification of ANY claim here must start
by unshallowing.

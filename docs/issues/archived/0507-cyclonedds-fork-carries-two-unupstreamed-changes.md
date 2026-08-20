---
id: 507
title: The cyclonedds fork carries two nano-ros-only lock changes that upstream lacks
status: resolved
type: tech-debt
severity: low
area: rmw
related: [issue-0371, issue-0496, issue-0609]
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

## Upstreaming prep: `upstream-zephyr-sync` (2026-08-20)

The Zephyr sync backend is now a **single squashed commit on upstream `master`**
(`e53552fb`), sitting on the local branch `upstream-zephyr-sync` in the submodule
checkout. Local only — fork remotes stay maintainer-gated, so nothing is pushed
and the submodule is back at the recorded pin `8601ca66` (superproject clean).

Cherry-picking `a09babf3` gave one conflict, in `sync.h`: our commit adds **two**
arms (`DDSRT_WITH_THREADX` and `DDSRT_WITH_ZEPHYR`) and upstream has neither. Only
the Zephyr arm belongs in this PR — the ThreadX arm names `sync/threadx.h`, which
is not part of this patch and is a separate nano-ros port.

Three things the census did not predict, each of which would have broken the PR:

1. **Upstream already has a Zephyr port** — `ports/zephyr/`, plus
   `option(WITH_ZEPHYR "Build for Zephyr RTOS" OFF)` at top-level
   `CMakeLists.txt:65`. So this is not "add Zephyr support"; it is "swap one TU
   in a port that exists". Re-declaring the option in `src/ddsrt/CMakeLists.txt`
   (where `WITH_FREERTOS` lives) would shadow the documented one — dropped.
2. **The `elseif(WITH_ZEPHYR)` branch already compiles `src/sync/posix/sync.c`**
   and lists `sync/posix.h`. Adding the Zephyr backend as a parallel block
   duplicates every `ddsrt_mutex_*`/`ddsrt_cond_*` symbol, and POSIX `sync.c`
   stops compiling anyway once `sync.h` selects the Zephyr types. The patch
   therefore **swaps** the two entries inside that branch.
3. **`DDSRT_WITH_ZEPHYR` did not exist as a define.** `sync.h` tests it, but only
   `DDSRT_WITH_FREERTOS` was wired (`set(...)` in `src/ddsrt/CMakeLists.txt` +
   `#cmakedefine` in `dds/config.h.in`). Without both halves the new arm is dead
   code and Zephyr silently keeps the pool-bound POSIX locks. Both added.

Nano-ros-internal prose (issue numbers, `zephyr/cyclonedds-config`, the striped
addrset reference) was rewritten upstream-neutral in all three files; `grep -i
'nano-ros\|nros\|issue 0'` over the touched files is clean.

**Not done:** the branch is unbuilt. Nano-ros selects this backend through its
own `config.h`, so the `WITH_ZEPHYR` → `DDSRT_WITH_ZEPHYR` → source-swap path
added here has never been exercised by any build in this repo. A Zephyr
configure of upstream `ports/zephyr` is the gate before a PR.

The addrset striping patch remains un-prepped and is the harder half: upstream
renamed those files (`ddsi_addrset`), so it needs **re-authoring**, not a
cherry-pick.

## Upstreaming prep: `upstream-addrset-stripe` (2026-08-20)

The addrset striping change is now a single commit on upstream `master`
(`e53552fb`), on the local branch `upstream-addrset-stripe`. Local only, nothing
pushed; submodule back at `8601ca66`.

This one was re-authored rather than cherry-picked, as predicted — but for a
smaller reason than expected. Upstream renamed `q_addrset.{c,h}` to
`ddsi_addrset.c` + a public `ddsi_addrset.h` / internal `ddsi__addrset.h` split
and prefixed every function `ddsi_`, but the **lock discipline is unchanged**:
`struct ddsi_addrset` still holds a `ddsrt_mutex_t lock`, and the same functions
take it in the same places. `git cherry-pick -Xfind-renames` matched the .c and
auto-merged most of it, leaving 7 conflicts that were purely the rename. The
header could not be matched at all, since one file became two.

Two upstream differences that changed the patch's shape:

- `addrset_purge` is **gone** upstream, so that hunk drops entirely.
- The forall family is `ddsi_addrset_forall{,_uc}_count` — two functions, where
  the fork patched three (`forall`, `forall_uc_else_mc`, `forall_mc`). Ported to
  what exists rather than reintroducing the fork's spelling.
- `addrset_forall_helper` and its arg struct become dead once the snapshot
  replaces them; upstream builds with strict warnings, so they are removed.

**The stripe hash was changed, and this is the one place the upstream version is
better than the fork's.** The fork divides the address by `2 * sizeof (void *)`
before masking, to shift out constant low bits. That works only when the
allocator's chunk spacing *for this exact struct size* is coprime with 64.
Measured with glibc over 4096 allocations: divide-and-mask uses all 64 stripes
at sizeof 40, 64, 96 and 128 — and **16 of 64 at sizeof 48**, quadrupling the
collision rate silently. Removing the mutex field takes `sizeof (struct
ddsi_addrset)` from 80 to **40** (confirmed via DWARF on the built library), so
the fork's hash is fine today and one added pointer away from not being. The
upstream commit uses a multiplicative hash (multiply by a 64-bit odd constant,
take the high bits), which measured 64/64 at every size. Worth backporting to
the fork.

Verified, which the Zephyr branch still is not: builds warning-free, and ctest
is **1498/1498** on Linux/x86_64. The 16 `psmx_*_iox` tests are excluded — they
abort with `POSH__RUNTIME_NO_WRITABLE_SHM_SEGMENT`, an Iceoryx shared-memory
environment problem on this host. That was confirmed against a baseline build of
unpatched `master`, not assumed: the same test aborts identically without the
change.

Remaining before either branch is a PR: the Zephyr branch is still unbuilt (a
Zephyr configure of upstream `ports/zephyr` is its gate), and pushing is
maintainer-gated for both.

## Zephyr branch: gate cleared, and a third stale-ref false alarm (2026-08-20)

**The "unbuilt" caveat above is discharged**, by splitting what was unproven from
what was not:

- The *code* was never in doubt. `src/ddsrt/src/sync/zephyr/sync.c` and
  `sync/zephyr.h` on `upstream-zephyr-sync` are **identical to the pin's, modulo
  comments** (diff ignoring comment lines is empty), and nano-ros builds and runs
  exactly that code on Zephyr today. Rewriting the prose could not break it.
- The *wiring* was new, upstream-only, and is what a PR would break. Verified by
  configuring upstream with `-DWITH_ZEPHYR=ON` on a host: the generated
  `dds/config.h` gets `#define DDSRT_WITH_ZEPHYR 1`, the build graph compiles
  `src/sync/zephyr/sync.c`, and `sync/posix/sync.c` drops out of it entirely
  (0 targets). Compiling that TU on the host then fails on exactly one thing —
  `fatal error: zephyr/kernel.h` — which is the proof the new arm is reached.

So the chain up to the Zephyr toolchain is verified. A real cross-compile is
still the last mile, but nothing between the option and the source list is
guessed any more.

**Third false alarm from stale remote-tracking refs.** Before pruning,
`git branch -r` listed `origin/nano-ros/zephyr-nsos-patches` at `e8ce7315`, and
`git branch -r --contains 8601ca66` came back EMPTY — reading exactly like "the
pin and the two patches are on no remote branch". It is wrong. `git fetch` had
said so (`error: some local refs could not be updated; try 'git remote prune
origin'`) and the message is easy to skim past. After `git remote prune origin`
the branch is `origin/nano-ros`, it is at `8601ca66` **exactly**, and it contains
everything. Nothing is unpushed.

That is now three distinct mechanisms producing the same false conclusion on this
issue: stale remote-tracking refs (here), a shallow checkout (above), and a
vanished ref in the census recipe (above). Any claim about what this fork does or
does not carry must start with `git remote prune origin && git fetch --unshallow`.

## Backport to the fork: `nano-ros-addrset-hash` (2026-08-20)

Local branch `nano-ros-addrset-hash`, one commit (`38da62e5`) on the pin,
carrying the multiplicative stripe hash back from the upstream branch. Not
pushed. Builds warning-free; ctest **1282/1282** with the iceoryx/shm tests
excluded — those abort on this host with or without the change, confirmed
against a baseline build of the unpatched pin. Note the fork spells them
`iceoryx_*`/`shm_*`, not `psmx_*_iox`, so a `-E iox` filter silently misses them.

Not urgent for the fork: `sizeof (struct addrset)` is 40 there too, which is a
good case. It is pre-emptive, and the argument for doing it anyway is that the
allocator the fork actually runs against is picolibc's on Zephyr, which nobody
measured.

## State of all three branches

All local, none pushed (fork remotes are maintainer-gated), submodule restored to
`8601ca66` with the superproject clean:

| branch | base | verified |
| --- | --- | --- |
| `upstream-zephyr-sync` | upstream `master` `e53552fb` | wiring at configure time; code identical to shipped |
| `upstream-addrset-stripe` | upstream `master` `e53552fb` | builds clean, ctest 1498/1498 |
| `nano-ros-addrset-hash` | fork pin `8601ca66` | builds clean, ctest 1282/1282 |

Local `master` is **70 behind `origin/master`**; both upstream branches sit on the
stale local ref and want a rebase before any PR.

## Correction: the fork tracks ROS's cyclonedds, so upstreaming does not un-fork it

The prep above treated "get these into upstream" as the path to closing this
issue. That is wrong about the horizon, and the version numbers say so plainly:

| | version |
| --- | --- |
| fork pin `8601ca66` | **0.10.5** |
| `ros-humble-cyclonedds` on this host | **0.10.5-2jammy** |
| `origin/master`, which both upstream branches target | **11.0.1** |

The submodule is pinned to the cyclonedds **that ROS ships**, not to an arbitrary
upstream point. It has to be: the host's `rmw_cyclonedds_cpp` (1.3.4 here) links
the ROS build, and a nano-ros image that speaks a different cyclonedds is the
same drift class as issue 0609, where a vendored zenoh pin fell behind the one
`rmw_zenoh_cpp` actually used.

Three consequences:

1. **Upstreaming does not reduce the carried delta on any horizon this issue
   cares about.** Even a merged PR against `master` (11.0.1) leaves the fork
   carrying both patches until ROS ships a cyclonedds that contains them —
   Humble is frozen at 0.10.5, so that is a distro migration away, not a release.
   The value of the two upstream branches is future-proofing and community
   benefit, not closing this issue.
2. **The 11.0.1 verification does not transfer.** `upstream-addrset-stripe` is
   ctest 1498/1498 and `upstream-zephyr-sync` is configure-verified, but both are
   against a codebase nano-ros never runs. The measurement that speaks to what
   nano-ros ships is the fork-side one: `nano-ros-addrset-hash`, 1282/1282 on
   0.10.5.
3. **`origin` is the NEWSLabNTU fork, not eclipse-cyclonedds.** It carries only
   `master` and `nano-ros`; the `releases/*` refs listed before the prune were
   stale local remote-tracking entries and are gone. A real upstream PR needs an
   eclipse-cyclonedds remote added first, and the branches rebased from the
   fork's `master` mirror (70 behind) onto real upstream.

So this issue closes when the delta is *carried deliberately and documented*, or
when a ROS distro bump lands the patches — not when a PR merges. The branches
remain worth having; they are just not the exit.

## Resolution (2026-08-20): the delta is documented and carried deliberately

Closed as **resolved**, on the finding that this issue's framing was wrong rather
than on the work it asked for.

It was filed as tech debt — two nano-ros-only changes "not offered upstream",
with the implied fix being to offer them. The versions say that cannot be the
fix. The submodule is pinned to the cyclonedds **ROS ships** (0.10.5, matching
`ros-humble-cyclonedds`) because a nano-ros image must speak the same Cyclone as
the host's `rmw_cyclonedds_cpp`. Upstream takes patches against `master` (11.x).
So a merged PR would leave this delta untouched until a ROS distro migration —
and the verification would not even transfer, a green ctest on 11.x saying
nothing about the 0.10.5 code that ships.

The real defect was that the delta was **unenumerated**: 15 commits over 39 files
that no single document described, which is what made it read as debt. That is
fixed by [docs/reference/cyclonedds-fork-delta.md](../reference/cyclonedds-fork-delta.md),
which records all 15 grouped by purpose, what would retire each, the ROS-version
constraint, the re-derivation recipe, and the testing caveats.

Three prep branches were built along the way and are kept, local and unpushed:
`upstream-zephyr-sync` and `upstream-addrset-stripe` (both on the fork's `master`
mirror, needing a real-upstream remote and a rebase before they are PR-able), and
`nano-ros-addrset-hash`, the one with near-term value — it makes the stripe hash
independent of the allocator, which matters because the shipped hash drops from
64 to 16 stripes if `sizeof (struct addrset)` ever reaches 48, and the allocator
it actually runs against is picolibc's, never measured. Landing that on the fork
branch is ordinary work, not a reason to hold this issue open.

**Reopen if** a ROS distro bump moves the cyclonedds line (the delta must be
rebased and re-enumerated), or if the fork gains commits without the reference
doc gaining rows.

One durable lesson, recorded because it cost time three separate ways here: every
false alarm on this issue — "the pin is on no remote branch", "these commits are
unfetchable", "the census says 58" — came from reading git state without first
running `git remote prune origin && git fetch --unshallow origin`. Stale
remote-tracking refs, a shallow checkout, and a vanished ref in the census recipe
each produce the same wrong conclusion.

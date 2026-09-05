---
id: 1094
title: "`runner-sweep`'s LRU sorts by DIRECTORY mtime, which a cargo target dir
  never updates — so it proposes deleting the 13 GiB group used yesterday to
  reclaim 60 MiB"
status: resolved
resolved: 2026-09-05
type: bug
area: ci, tooling
related: [0844, 0616]
---

## Symptom

`./scripts/ci/runner-sweep.sh --check`, 2026-09-05, on this host:

```
fixtures: 60.1 GiB used / 60.0 GiB budget (high-water 60.1 GiB)
fixtures: OVER BUDGET by 60 MiB — evicting least-recently-modified entries
  [would] rm -rf build/cmake-fixtures/l9_register_c    (488 KiB, mtime 2026-08-10)
  [would] rm -rf build/cmake-fixtures/l9_register_cpp  (488 KiB, mtime 2026-08-10)
  [would] rm -rf build/cargo-fixtures/linux            (12.8 GiB, mtime 2026-08-15)
fixtures: 12.8 GiB would be freed
```

**60 MiB over budget, 12.8 GiB deleted** — a 213× overshoot — and the entry it
picks is the shared cargo group for the NATIVE platform, which every tier-1 run
needs and which was written to the previous day.

## Cause: a directory's mtime is not a record of use

`_evict` orders candidates with `stat -c '%Y' "$child"`. A directory's mtime
changes when an entry is created or removed **in that directory**, and a cargo
`--target-dir` writes into `<profile>/deps/…` several levels down. So a group
dir's mtime freezes near creation and stays there while the group is used daily.

Measured across all 20 children of `build/cargo-fixtures`:

| child | dir mtime | newest content | gap |
| --- | --- | --- | --- |
| `linux` | 2026-08-15 | 2026-09-04 | **20 days** |
| `linux-1147932602` | 2026-08-15 | 2026-09-04 | 20 days |
| `linux-228170020` | 2026-08-15 | 2026-09-04 | 20 days |
| `linux-3000917972` | 2026-08-15 | 2026-09-04 | 20 days |
| `linux-3263301353` | 2026-08-15 | 2026-09-04 | 20 days |
| `linux-553222167` | 2026-08-15 | 2026-09-04 | 20 days |
| `linux-865285299` | 2026-08-15 | 2026-09-04 | 20 days |
| `threadx-riscv64` | 2026-08-18 | 2026-08-21 | 3 days |
| `qemu-arm-baremetal` | 2026-08-15 | 2026-08-15 | 0 — genuinely cold |

**Eleven of twenty are wrong by ~20 days, and the error is not random**: it is
biased toward the LARGEST and most-used groups, because a group that has existed
longest has both the oldest creation date and the most accumulated bytes. The
two entries that are genuinely cold (`qemu-arm-baremetal`,
`qemu-esp32-baremetal`) carry the same 08-15 stamp as the busiest one, so they
sort as ties and the tie is broken by listing order.

The comment above `_evict` states the intent this defeats:

> Never the dir itself: deleting `build/cargo-fixtures` wholesale turns a budget
> overrun into a full rebuild of everything, when dropping the three coldest
> coordinates would have done.

The reasoning is right; `linux` is simply not one of the coldest, and the
timestamp it is ranked by cannot say so.

## Fix

Rank by the newest mtime found INSIDE each candidate, not by the candidate's own
mtime. That is a real walk and this is already a between-jobs operation whose
header prices `du` at "seconds to a minute or two" — the walk can share it.

Cheaper alternative if the walk is unwelcome: rank a cargo-shaped child by the
mtime of its `<profile>/` subdirectory, which does move, rather than by the group
root.

## Should `cargo-sweep` replace this? No — it cannot reach the area

Asked, and answered by measurement rather than preference.

**Where cargo-sweep works, it works well.** On the two manifest-rooted target
dirs it reclaimed **41.3 GiB** here (31.53 from `target/`, 9.80 from
`packages/cli/target`) with no whole-group deletion — it prunes stale UNITS and
leaves current ones, which is exactly the granularity `_evict` lacks.

**But it requires a manifest.** Every mode routes through `cargo metadata` on the
path it is given:

```
$ cargo-sweep sweep --dry-run --maxsize 5GB build/cargo-fixtures/linux
Error during execution of `cargo metadata`:
  manifest path `build/cargo-fixtures/linux/Cargo.toml` does not exist
```

The phase-340 shared groups are DETACHED `--target-dir`s with no project of
their own, so cargo-sweep cannot address `build/cargo-fixtures` at all — 39 GiB,
and the reason the fixtures budget exists. `--file`, `--maxsize` and `--time`
all fail the same way.

So the two tools are complementary and the split is not a matter of taste:

| | `runner-sweep` | `cargo-sweep` |
| --- | --- | --- |
| operates on | any directory tree | a cargo PROJECT (needs `Cargo.toml`) |
| granularity | whole child directory | individual build artifacts |
| has a budget, pinning, high-water | yes | no |
| reaches `build/cargo-fixtures/*` | yes | **no** |
| reaches `target/`, `packages/cli/target` | **no** | yes |

## FIXED 2026-09-05 — (1) and (2) landed, (3) is refuted

**(1) The mtime proxy.** `_newest_mtime` walks each candidate and ranks by the
newest file inside it, falling back to the directory's own mtime when the walk
yields nothing — ordering by a stale timestamp is bad, and ordering by `0`,
which sorts FIRST and evicts hardest, is worse. `find` is correct here for the
reason `_evict`'s own comment already gives: issue 0844's rule is about TRACKED
files, and nothing under these roots has been in the index.

The same dry run, before and after, on the same tree:

```
before:  [would] rm -rf build/cargo-fixtures/linux                 (12.8 GiB)
after:   [would] rm -rf build/cargo-fixtures/qemu-esp32-baremetal   (2.2 GiB)
```

`qemu-esp32-baremetal` is one of the two entries whose directory mtime and
content mtime AGREE — genuinely cold. The busiest group is no longer a
candidate.

Gated by `check-runner-sweep-eviction-order` (fast line, ~1 s, tmpdir): two
5 MiB entries with IDENTICAL directory mtimes, one written today and one 40 days
ago, asserting the cold one is proposed first. It asserts its own premise too —
if the fixture's directory mtimes ever stop matching it no longer models the
hazard, and a pass would mean nothing. Mutation-verified: restoring
`stat -c '%Y' "$child"` fails it.

**(2) The host target dirs are an area now.** `cargo-targets` reports their size,
records a high-water mark, and prunes with `cargo-sweep --time ${NROS_SWEEP_CARGO_DAYS:-21}`.

They are pruned, never LRU-evicted, for the reason `_evict` already gives for
never deleting a candidate ROOT: a cargo target dir is one cache with internal
structure, so dropping it is a full rebuild when pruning the stale units would
have done. Measured on this host: **31.53 GiB** from `target/` and **9.80 GiB**
from `packages/cli/target/`, with current artifacts and the in-tree `nros`
binary left in place.

A runner without `cargo-sweep` reports the size and says what it could not do,
rather than failing a machine that is otherwise fine. `NROS_CARGO_FLAGS` is
cleared around the call: `scripts/bin/cargo` injects `--locked` project-wide
(0359/0378) and `cargo-sweep` shells out to `cargo metadata`, which then sees a
flag meant for a build — the same collision that breaks `cargo binstall`.

Also fixed while wiring it: `Would clean: nothing from "…"` is cargo-sweep's
NO-OP answer, and matching on `Would clean` alone marked the sweep dirty on a
clean tree — which under `--check` is an exit 1, a between-jobs hook failing a
healthy machine.

**(3) REFUTED, by measurement.** The proposal was to point `cargo-sweep` at a
detached group through a throwaway manifest. The manifest does let it resolve
the directory — but it then cleans **nothing**, at any age:

```
$ CARGO_TARGET_DIR=build/cargo-fixtures/linux cargo-sweep sweep --dry-run --time 3 .
[INFO] Would clean: nothing from ".../build/cargo-fixtures/linux"
```

against a group that really holds 7.18 GiB in the 7–21 day band and 1.24 GiB
over 21 days.

The reason is sharper than "it needs a manifest": **it needs the RIGHT manifest.**
`cargo-sweep` enumerates the packages the project's `cargo metadata` declares and
keeps artifacts belonging to them; a throwaway project declares one package, so
nothing in the group is explained and nothing is swept. A phase-340 shared group
serves many leaf workspaces at once, so no single manifest can explain it, and
there is no combination of flags that changes that.

So the fixtures area keeps whole-entry eviction, and (1) is what makes that
tolerable — it now drops the coldest coordinate rather than the largest one.

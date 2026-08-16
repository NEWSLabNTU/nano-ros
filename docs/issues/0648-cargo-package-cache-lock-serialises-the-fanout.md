---
id: 648
title: "The fixture fan-out serialises on cargo's GLOBAL package-cache lock: 23 cargo processes, 4 compiling, 274 blocks, 0 downloads"
status: open
type: performance
area: build
related: [issue-0509, issue-0604, phase-340, phase-365]
---

## Symptom

`htop` during `build-test-fixtures lane=all` shows many cargo processes and low
CPU. Sampled mid-run:

| signal | value |
| --- | --- |
| cargo processes | 23 (22 sleeping, 1 in `D`) |
| rustc actually compiling | **4** |
| `Blocking waiting for file lock on package cache` | **274** |
| blocks on a build dir / `Cargo.lock` | 1 |
| **downloads in progress** | **0** |

## What it is

Not the build directory, and not I/O. The contention is `$CARGO_HOME/.package-cache`
— a MACHINE-WIDE lock, shared by every concurrent cargo invocation regardless of
which target dir, workspace or lane it belongs to.

**Zero downloads is the load-bearing number.** Every crate is already fetched, so
this is not network work being serialised. The invocations queue for a global
lock, do almost nothing under it, and proceed.

## Where the block sits

At INVOCATION START, before compilation. Every blocked wait is immediately
followed by the first build line:

```
29  Compiling nros-rmw-cffi
16  Compiling nros-zpico-build
14  Compiling nros-c
11  Compiling proc-macro2
10  Checking byteorder
```

So the lock is on the resolution path each invocation walks before it can begin,
not something held across a build.

## Scale

Within the zephyr lane alone:

| | |
| --- | --- |
| leaf logs that ran cargo | 89 |
| leaf logs that blocked at least once | **68** (76 %) |
| total block events | 274 |
| worst single leaf | 8 blocks (`build-rust-service-server-zenoh`) |

## Why this matters beyond being slow

Issue 0509 measured this lane at 76 % idle with ~0 compilers live and concluded
"fixed per-leaf overhead dominates", locating it in cmake configure work. That
conclusion stands, and this is a SECOND component of the same overhead that
nobody had named — one that a disk or jobserver theory cannot explain, and that
the earlier storage A/B (iowait ~0 on both HDD and NVMe) had already ruled out
without identifying what was left.

It also bounds what phase-340's shared cargo groups can buy: fewer target dirs
do not reduce contention on a lock that is global to the machine.

## Candidate remedies, cheapest first

1. **Pre-warm once, then `--offline`.** One `cargo fetch` before the fan-out, and
   `--offline` for the leaves, so no invocation needs write access to the cache.
   Cheap to try; the open question below decides whether it is sufficient.
2. **Per-lane `CARGO_HOME`.** Removes contention outright, at the cost of a
   duplicated registry per lane.
3. **Fewer invocations.** The direction phase-340 already pushes.

## The open question, stated so it is not guessed

Whether the lock is taken because something still WRITES to the cache (index
`.cache/` entries are written on first use even offline), or whether cargo takes
it on every resolution regardless. Remedy 1 fixes the first case and not the
second.

The experiment: run N concurrent cargo invocations over one warm tree, with and
without `--offline`, and count `Blocking waiting for file lock` in each arm. It
must run on an otherwise idle box — the numbers above were sampled during a live
`lane=all`, so they establish that the contention EXISTS and is large, not how it
scales.

## Credit

Observed by the maintainer from `htop` (many cargo processes, low CPU) during a
2026-08-16 `lane=all`. The measurement above followed from that read; the
hypothesis it replaced — a shared BUILD directory — was wrong, and the log
message names the package cache explicitly.

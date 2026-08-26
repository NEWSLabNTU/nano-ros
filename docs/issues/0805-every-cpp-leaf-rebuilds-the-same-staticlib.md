---
id: 805
title: "Every C/C++ example leaf drives its own cargo build of the same staticlib, and sccache cannot dedupe it"
status: open
type: performance
area: build
related: [issue-0726, issue-0616, issue-0500, issue-0488, phase-340, phase-371]
---

# 0805 — one staticlib, twenty-one builds

Found while answering issue 0726's priority 3 ("why is `threadx_riscv64` the
tail"). The answer was not specific to that platform, so it is filed on its own.

## What happens

A C/C++ example leaf is a standalone cmake project (RFC-0026), and it reaches
the Rust side through Corrosion, which places the cargo `--target-dir` INSIDE
that leaf's own build dir:

```
examples/qemu-riscv64-threadx/c/errno-isolation/build-zenoh/cargo/nano-ros_1147c
```

So every leaf compiles the whole Rust dependency chain again. Sampled from a
live `threadx_riscv64` fixture build, five concurrent cargo invocations, with
byte-identical arguments — same package, features, target, crate-type, differing
only in `--target-dir`:

```
5  --target=riscv64gc-unknown-none-elf
   --features=ros-humble,cffi-zenoh-cffi,alloc,platform-threadx,panic-platform
   --package nros-c --crate-type=staticlib
1  ... --package nros-cpp --crate-type=staticlib
```

Counted by what one run actually wrote (mtime-filtered, so this is live work and
not the residue that makes a raw `find` misleading here):

| | fresh in one stage run |
| --- | --- |
| `libnros_c.a` | **21** |
| `libnros_cpp.a` | **14** |
| `libnetxduo.a` | 0 — correctly incremental |
| `.o` files | 1869 |

## Why sccache does not save it

It cannot. From sccache's own stats during that build:

```
Non-cacheable reasons:
crate-type                           94
```

sccache does not cache `--crate-type=staticlib`, which is exactly what every one
of these invocations produces. Measured hit rate on the run: 289 hits / 2275
requests (12.7%), and the cache was at capacity (30 GiB of 30 GiB max), so the
dependency rlibs that ARE cacheable are also evicting each other.

This is worth stating plainly because "sccache will absorb it" is the natural
assumption and it is wrong for the specific artifact that dominates.

## Measured cost

`just threadx_riscv64 build-fixtures`, quiet box, 32 cores, lineage-scoped
sampler:

| | |
| --- | --- |
| cold run | **1706 s** |
| immediate warm re-run | **494 s** |
| occupancy, cold | alive 27.4, **runnable 0.10** of 32 |
| occupancy, warm | alive 14.0, **runnable 0.05** of 32 |

The warm number is the sharper one. That run did **zero** cmake configures and
**zero** compilations — the log's only work is 70 `Finished` lines. 494 s to
decide there was nothing to do, at ~7 s per cargo invocation, because each of
the 70 invocations re-scans its own private fingerprint database.

The stage is also a serial pipeline. Over 60 sampled instants of the cold run:

| running leaf processes | share of instants |
| --- | --- |
| 0 | **93%** |
| 1 | 7% |
| 2+ | never |

## What this is NOT

* **Not a `threadx_riscv64` defect.** The per-leaf cargo dir is the shape
  everywhere: 29 such dirs under `qemu-riscv64-threadx`, 24 under
  `threadx-linux`, 59 under `native`. Native has MORE of them and was faster
  (720 s against 1302 s in the tier2 joblog), so what puts threadx on top is the
  per-invocation cost of the cross target, not the structure. Isolating that
  would need a comparable cold measurement per platform, which has not been run
  — do not quote a per-platform ranking from this issue.
* **Not issue 0491 churn.** Build-script output stamps did not move between the
  cold and warm runs, so nothing is re-running build scripts forever. (A
  hand-run cargo DOES re-run them, because it lacks the `THREADX_DIR` /
  `NETX_DIR` the recipe exports — worth knowing before reading a hand-run
  timing as a no-op.)
* **Not the cargo package-cache lock** (issue 0648). `~/.cargo/.package-cache`
  was checked in `/proc/locks` during the build and was not contended. The
  flocks held were each leaf's own `.cargo-lock`, one holder per target dir.

## The fix, and the constraint on it

Build `nros-c` / `nros-cpp` ONCE per (platform, feature-set) and have leaves
LINK the prebuilt artifact, instead of each leaf driving cargo. phase-340
already built the machinery for the Rust leaves —
`nros_fixture_target_dir_flag` / `nros_fixture_row_artifact_dir` — and
`just/threadx-riscv64.just` says so in a comment that turns out to describe only
the Rust half:

> issue 0488 residue 2 — the SHARED group, not a per-leaf `target-zenoh/`.

The constraint is that this is exactly the ground issues 0616 and 0500 mark as
dangerous, and the distinction matters: **0616 forbids one `--target-dir`
serving two workspace ROOTS**, and 0500 was Corrosion `< 0.6.0` sharing one
`cargo/build` across roots, producing duplicate `#[no_mangle]`. Here every one
of the 21 invocations names the SAME root and the SAME manifest, so sharing is
the correct configuration rather than the forbidden one — but any implementation
must prove that rather than assume it, because the failure mode is a link error
nobody reads as a target-dir problem.

Note also that sharing a target dir fixes the COLD duplication and not the warm
494 s: 70 leaves would still invoke cargo 70 times, each still scanning. Cutting
the warm floor needs fewer invocations, i.e. the prebuilt-artifact shape, not
just a shared directory.

## Acceptance

* One `libnros_c.a` / `libnros_cpp.a` per (platform, feature-set) per fixture
  build, verified by mtime-filtered count, not by reading the recipe.
* No duplicate-symbol or wrong-arch link regression — the 0500/0616 class
  checked explicitly, on `mixed` (the entry that has caught it both times).
* Cold and warm wall clock re-measured with `sample-build-lineage.sh` on a quiet
  box, both reported, with the box's own loadavg alongside.

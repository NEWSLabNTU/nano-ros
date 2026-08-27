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

## Fixed for `threadx_riscv64` (2026-08-26): 1706 s -> 898 s, binaries identical

### Mechanism

Corrosion 0.6.1 offers no knob for its cargo directory — it is a plain local,
computed as `${CMAKE_BINARY_DIR}/cargo/<folder>_<hash-of-manifest-path>`. Its
`CARGO_FLAGS` hook is not a substitute: a second `--target-dir` moves cargo's
OUTPUT while Corrosion keeps looking for the byproduct at the path it derived,
so the build stops finding its own artifact. The only override point is the path
Corrosion already computes, so `nros_share_corrosion_cargo_dir()` replaces
`<build>/cargo` with a symlink to a shared directory.

### The key is the whole design

Sharing is safe only between leaves whose cargo inputs are EQUAL, because cargo
uplifts the archive to an unhashed `libnros_c.a` and two feature sets sharing a
directory would overwrite each other's — the 0500/0616 failure, diagnosed far
from its cause. So:

* The key is the full input set `nros_feature_set()` is a function of —
  platform, rmw, board, **capabilities** — plus profile and target triple. Equal
  key implies equal features by construction, not by inspection. Keying on
  "platform + rmw" would have been WRONG: capabilities are per-leaf.
* It is computed in `packages/api/nros-c/CMakeLists.txt` AFTER
  `nros_feature_set()`, not in `nros_resolve_corrosion()` — capabilities are not
  resolved that early, and a key computed before its inputs exist is the bug
  rather than the fix. Corrosion only needs the link before the BUILD, so
  configure-time placement is free.
* Every field is LABELLED (`board=`, `caps=`). An empty element vanishes from a
  cmake list, so bare values would make `(board="", caps="x")` and
  `(board="x", caps="")` hash identically. That was a real bug in the first
  version of this change, caught by printing the key.
* The key text is written to `<dir>.key` beside the hash, because a hash in a
  path answers no questions when two leaves unexpectedly do not share.

### Measured, quiet box, 32 cores

| | |
| --- | --- |
| cold, before | 1706 s |
| cold, Rust leaves only | 1125 s |
| **cold, both paths** | **898 s (-47%)** |
| warm, before | 414-494 s |
| warm, after | 461 s — **unchanged, as predicted** |
| per-leaf cargo dirs | 29 -> **0** |
| shared dirs | 4, keyed |

**All 29 fixture binaries are byte-for-byte identical to the pre-change build.**
That is the correctness argument; the timing is only the point of doing it.

Wiring one path was not enough and the numbers say so: the Rust leaves go
through `build_threadx_cmake_rmw` while the C/C++ leaves reach cmake through
`fixtures-build.sh` via `NROS_CMAKE_EXTRA_DEFS`. Doing only the first left 17 of
29 per-leaf dirs in place and gave 1125 s instead of 898 s.

### Two things this surfaced

* **The platform has two spellings.** The four keys differ only in
  `platform=threadx_riscv64` (Rust path) vs `platform=threadx` (C/C++ path), at
  the same board, rmw and triple. So the tree keeps 4 shared dirs where 2 may
  do. That is the SAFE direction — over-separation costs sharing, and only
  under-separation can corrupt — but it is a real inconsistency worth its own
  look.
* **An existing build dir does not get this.** `<build>/cargo` is already a real
  directory there, and the function DEGRADES rather than failing: not sharing is
  the old behaviour, correct and slower, so a `FATAL_ERROR` would break every
  incremental build that predates the feature in exchange for a speedup. It
  prints a STATUS naming this issue so the degradation is not silent. Wipe the
  build dir to opt in.

### Still open

* **Only `threadx_riscv64` is wired.** Every other platform still gives each
  C/C++ leaf its own cargo dir. The mechanism is platform-neutral — it needs
  `-DNROS_SHARED_CARGO_ROOT` on the other lanes and the same before/after
  binary-identity check per platform. Do not wire them blind.
* **The warm floor is untouched and this cannot touch it**, now measured rather
  than predicted: 461 s against a 414-494 s baseline. It is 70 cargo invocations
  re-scanning fingerprints at ~7 s each, and it needs FEWER INVOCATIONS — the
  prebuilt-artifact shape — not a shared destination.

## `native` wired too (2026-08-26), plus four defects the guards caught

`native` is the tier-1 lane and had the most duplication: **59 per-leaf cargo
dirs, 186 GB**. Now **0 per-leaf dirs, 61 GB**, with the shared root at 8.5 GB
across both platforms.

Correctness, established the clean way after a false alarm (below): the same
leaf built through the SAME lane with sharing OFF and ON is **byte-identical**
(`485381fdc490af076a1e` both ways), and native lane builds are byte-reproducible
run-to-run, which is what makes that comparison meaningful.

### The false alarm, recorded because it nearly became a wrong conclusion

A checksum sweep after the native rebuild showed **53 of 53 binaries differing**
from the baseline captured earlier in the session. That looked like the change
corrupting output. It was not: between the two captures the tree had been
rebased onto ~50 upstream commits. The baseline was STALE, and it was measuring
upstream, not this change. The lane-level A/B above is the comparison that
actually isolates the variable.

Same shape as the `_cyclonedds`-suffixed "missing binaries" earlier in this
issue: a checksum set is only evidence if nothing else moved between captures.

### Four defects, each caught by a guard rather than by review

1. **The key was UNSTABLE.** The call sat after `nros_feature_set()` (right for
   capabilities) but BEFORE `nros_resolve_cargo_profile()`. `NROS_CARGO_PROFILE`
   is a cache variable, so it read empty on a fresh configure and `release` on
   the next — the same leaf computing two keys. The mismatch guard caught it.
   Placement must be after BOTH.
2. **The mismatch error printed two HASHES.** Diagnosing #1 was impossible until
   the error printed the key TEXT — the "two hex strings nobody can order by
   eye" problem CLAUDE.md records for submodule pins. The key file is now
   written BEFORE the checks so both sides of a mismatch exist on disk.
3. **An empty `-DNROS_SHARED_CARGO_ROOT=` was silently ignored.** `nros_build_dir`
   is not in scope in `just/native.just` (cargo.sh sources build-root.sh only
   from inside one of its functions), so the flag expanded empty and the cmake
   side fell back to per-leaf dirs — the fix reading as applied while doing
   nothing. Now a hard error: nobody passes this flag meaning nothing.
4. **A re-pointed symlink DANGLED.** The re-point branch created the link but
   not its target, and `mkdir` on a dangling symlink fails with EEXIST — cargo
   reports `failed to create directory ... File exists (os error 17)`, naming
   the link and not the missing target. The target is now created before any
   branch.

The mismatch guard also stopped demanding a wipe. It RE-POINTS: a symlink is not
data, the old key's directory keeps its contents, and the dir serves exactly one
key at a time so the ambiguity it guards against cannot arise. Failing would have
forced every leaf's build dir to be deleted for a rename.

### One failure mode that survives, and its exact scope

A leaf build dir wiped while its shared dir stays populated fails to rebuild
under a BARE `cmake -S … -B …`: cargo sees the build script as fresh, so
`write_header_to_corrosion` never runs, and the per-leaf
`nros_cpp_config_generated.h` is never written — the mirror copy then fails with
`No such file or directory`. This is the sizes-header class (0088 -> 0114 ->
0122 -> 0123 -> 0245 -> 0268) reappearing through a shared target dir.

**It does NOT occur through the lanes.** Verified on both: wipe one leaf, run the
lane, header and binary come back, rc=0 (threadx 473 s, native 253 s). I could
not determine WHY the lane's cargo re-runs the script where a bare invocation
does not, and I am recording that rather than inventing a mechanism. Nothing in
the tree passes `NROS_SHARED_CARGO_ROOT` outside a lane, so the exposure today is
zero — but anyone enabling sharing by hand should know this edge exists.

### The shared root ACCUMULATES

A key change orphans the old directory rather than deleting it. This session's
profile-field fix stranded 8 dirs holding 8.6 GB, removed by hand. That is the
issue-0500 SDK-store class one directory over: a store that only grows. A GC —
drop key dirs whose `.key` no longer matches any live configuration — is worth
having before this spreads to more platforms.

### Still open

* **Other platforms.** freertos, nuttx, threadx-linux, esp32, zephyr all still
  give each C/C++ leaf its own cargo dir. Every one reaches cmake through
  `NROS_CMAKE_EXTRA_DEFS`, so wiring is one line each — but each needs its own
  lane-level byte-identity A/B, and `nros_build_dir` must be verified IN SCOPE
  in that recipe first (defect 3 above).
* **The warm floor**, unchanged and untouchable by this: 70 cargo invocations
  re-scanning fingerprints. It needs fewer invocations, not a shared destination.
* **Two smells surfaced by the keys, neither investigated**: `caps=safety,safety`
  carries a duplicate, and one observed cargo invocation passed
  `panic-platform` SEVEN times. Harmless to cargo and to the key, but both point
  at a list that accumulates instead of replacing.
* **The platform has two spellings** — `threadx` and `threadx_riscv64` at the
  same board/rmw/triple, so that platform keeps 4 key dirs where 2 may do.

## `threadx-linux` wired (2026-08-26) — third lane, same verification

Lane-level A/B on the current tree, same leaf both ways:

```
sharing OFF   b60d444f95579f1ca04d1199
sharing ON    b60d444f95579f1ca04d1199   -> byte-identical
```

Both halves built fresh on the same commit, deliberately: the earlier native
scare came from comparing against a baseline captured before a 50-commit rebase.
A checksum pair is evidence only when nothing else moved between the two.

`nros_build_dir` was verified IN SCOPE in that recipe BEFORE wiring, per defect 3
— a missing helper expands the flag empty and the cmake side quietly keeps
per-leaf dirs, so the change would read as applied while doing nothing.

Note what the run also shows: **23 of 24 per-leaf dirs remained.** That is the
documented degrade path, not a failure — an existing build dir already has a
real `cargo/` directory, so sharing applies to leaves configured fresh. The
platform converts as its build dirs are wiped, and `just gc-shared-cargo` is what
keeps the store from accumulating while that happens.

### Lanes now wired

| lane | per-leaf dirs before | verified |
| --- | --- | --- |
| `threadx_riscv64` | 29 -> 0 | byte-identical, 1706 s -> 898 s |
| `native` | 59 -> 0, 186 GB -> 61 GB | byte-identical |
| `threadx-linux` | 24 (converts on wipe) | byte-identical |

Remaining after this: see the survey below — "one line each" turned out to be
true of exactly ONE of the four.

## Surveyed all four remaining lanes (2026-08-27) — and the estimate was wrong

This issue said of freertos, nuttx, esp32 and zephyr:

> Every one reaches cmake through `NROS_CMAKE_EXTRA_DEFS`, so wiring is one line
> each

**That holds for one of the four.** I wrote it from the two lanes I had already
wired and never checked the others. Surveyed properly:

| lane | Corrosion per-leaf dirs | verdict |
| --- | --- | --- |
| **freertos** | **12** (6 C + 6 C++, all one hash) | APPLIES — wired, verified |
| nuttx | **0** | does not apply — see below |
| esp32 | **0** | does not apply — no C/C++ leaves exist at all |
| zephyr | **0** of 141 build dirs | does not apply — not a Corrosion path |

### freertos — wired and verified

Lane A/B, same leaf, both halves fresh on the same commit:

```
sharing OFF   d2775a32302f2a98364504a1
sharing ON    d2775a32302f2a98364504a1   -> byte-identical
```

`nros_build_dir` verified in scope first. Worth recording WHERE it comes from:
only `scripts/build/lane-skip.sh` exports it (it dots `build-root.sh` at file
scope); `cargo.sh` dots the same file INSIDE a function, so it does not. The
freertos recipe happens to source `lane-skip.sh` first, so the wiring is correct
— but that dependency is incidental rather than declared, which is the same
shape as defect 3 on native and will bite whoever reorders those lines.

### nuttx — the flag would be INERT, and the duplication is real anyway

`CMakeLists.txt:105` reads `if(NOT NANO_ROS_PLATFORM MATCHES "^nuttx")` around
`add_subdirectory(packages/api/nros-c)`. The only call site of
`nros_share_corrosion_cargo_dir()` is inside that subdirectory, so on nuttx it
is never invoked and `-DNROS_SHARED_CARGO_ROOT` would do nothing. Note the
empty-value FATAL guard lives inside the function too, so even a broken value
would fail silently here — a lane where the wiring cannot announce its own
breakage.

But nuttx HAS the same problem through a different mechanism: 13 leaves each
with a private `cargo-target/`, measured **716 MB** on `c/talker` alone (~9 GB
total), set at `packages/api/nros-c/cmake/nros-nuttx.cmake:228`:

```cmake
set(_cargo_target_dir "${CMAKE_CURRENT_BINARY_DIR}/cargo-target")
```

That line's own comment already states the key argument this issue arrived at
independently — "concurrent/sequential builds from different examples silently
clobber each other" — so any sharing there needs the same keying discipline.
Also note nuttx has TWO platform coordinates (`nuttx` arm, `nuttx-riscv`), and a
key that ignored the arch would be wrong.

### zephyr — not Corrosion, and the census is complete

`zephyr/CMakeLists.txt:192-194` says so outright ("not a Corrosion target"), and
`zephyr/cmake/nros_cargo_build.cmake:412` pins its own
`CARGO_TARGET_DIR=${CMAKE_BINARY_DIR}/nros-rust`. Verified by scanning the whole
out-of-tree workspace rather than sampling: **0** `nano-ros_*` dirs across all
**141** `build-*` directories.

Zephyr does have the duplication — 141 per-leaf `nros-rust/` dirs — but fixing it
is a change to `nros_cargo_build.cmake`, not a lane wire-up.

### esp32 — nothing to wire

`examples/qemu-esp32-baremetal/` is Rust-only: two leaves, no `CMakeLists.txt`
anywhere, built by cargo directly. The lane's single Corrosion dir belongs to one
`idf.py` TEST FIXTURE, not an example leaf, and `idf.py` does not read
`NROS_CMAKE_EXTRA_DEFS`.

### Lanes wired, final

| lane | before | verified |
| --- | --- | --- |
| `threadx_riscv64` | 29 -> 0 | byte-identical, 1706 s -> 898 s |
| `native` | 59 -> 0, 186 GB -> 61 GB | byte-identical |
| `threadx-linux` | 24 (converts on wipe) | byte-identical |
| `freertos` | 12 (converts on wipe) | byte-identical |

**Every lane that can use this mechanism now does.** What remains is not more
wiring:

* **nuttx**, `nros-nuttx.cmake:228` — ~9 GB across 13 leaves, needs the same
  keyed sharing built for its own cargo driver, and must key on the arch.
* **zephyr**, `nros_cargo_build.cmake:412` — 141 per-leaf dirs, same shape.
* **the warm floor**, which no target-dir change can touch: it is 70 cargo
  invocations re-scanning fingerprints and needs FEWER INVOCATIONS.

### Closed while here: the two duplication smells

* `caps=safety,safety` — `packages/api/nros-c/CMakeLists.txt` appends `safety`
  without checking whether the declaration already named it. Fixed at source
  with `list(REMOVE_DUPLICATES _caps)`; the key normaliser keeps it harmless
  either way.
* `panic-platform` seven times — `nros_apply_panic_policy()` is called once per
  `nano_ros_entry()` and `corrosion_set_features` APPENDS. Investigated and
  deliberately NOT changed: cargo dedupes features, and conflicting policies are
  already caught by a `FATAL_ERROR` against the global
  `NROS_ENTRY_PANIC_POLICY`. De-duplicating the CALLS would risk skipping a
  target that only appears later in the configure, which is a real hazard traded
  for a cosmetic one.

## nuttx fixed (2026-08-27) — a different mechanism, same keying discipline

The survey above found nuttx does not reach the Corrosion path at all. Its
duplication is real anyway and larger per leaf: a hand-rolled cargo build of
`nros-nuttx-ffi` with `CARGO_TARGET_DIR` set per example at
`packages/api/nros-c/cmake/nros-nuttx.cmake`.

### What the existing comment got right, and what it hid

> Without this every example's cargo build lands at the same path under the FFI
> crate's `target/`, and concurrent / sequential builds from different examples
> silently clobber each other.

Correct, and measurably so: the per-example artifacts genuinely DIFFER — three
leaves, three distinct hashes — because build.rs compiles each app's own C
sources in. So this is NOT the nros-c case, where every leaf built the identical
staticlib.

What the comment does not say is the proportion:

| | |
| --- | --- |
| `nros-nuttx-ffi` (per-example) | **736 KB** |
| everything else (deps + build scripts) | **715 MB** |

13 leaves x ~716 MB is ~9 GB of the same dependency graph, protected by a rule
that only needed to protect 736 KB of it.

### The split

Share the target dir; keep the two per-example outputs out of it.

* **The artifact** via cargo's own `--artifact-dir`. That matters over an
  external copy: cargo places the file inside its own run, so no other leaf's
  cargo can get between the build and the copy. Unstable, which costs nothing
  here — the crate is already pinned to a nightly and already passes
  `-Z build-std`.
* **The depfile** by copying it in the same command. `--artifact-dir` does not
  copy it (verified on a scratch crate), and it is per-example: measured, it
  names this leaf's own `*_includes.txt`, `*_ffi_libs.txt` and `src/main.c`. A
  shared depfile would give every other leaf the wrong rebuild triggers — issue
  0820's museum binary, which that file's own comment records costing 90 s and
  a long investigation.

### The key must separate the ARCHES, and nearly did not

`<target>/release/build/` holds HOST build-script output and is **not**
triple-separated inside a target dir, while NuttX's kernel tree is reconfigured
in place between arm and rv-virt. A dir shared across arches would hand one
arch's build scripts output compiled against the other's headers. Key is
triple + profile + FFI crate + NUTTX_DIR + defconfig.

Same ordering trap as the native lane, avoided this time by remembering it: the
key is computed AFTER `nros_resolve_carve_out_profile()`, because that call is
what defines `_NROS_NUTTX_PROFILE`. Computed before, the profile field would
read empty on a fresh configure and populated on the next — one leaf, two keys.

The keying rule itself is now factored into `nros_shared_cargo_dir()` and shared
by both consumers, rather than copied. Corrosion still needs its symlink; nuttx
takes the path directly.

### Verified

| | |
| --- | --- |
| A/B, same leaf, sharing off vs on | `306f411dd134a63d` both — byte-identical |
| all 12 arm leaves through ONE shared dir | **12 artifacts, 12 distinct** — no clobber |
| depfiles | per-leaf, each naming its own leaf |
| per-leaf `cargo-target/` dirs | 12 -> **0** |
| shared dir | **502 MB** (was ~716 MB per leaf) |
| rebuild of all 12 leaves against a warm shared dir | **68 s** |

### The GC was wrong, and this found it

`gc-shared-cargo` counted reachability by `cargo` symlinks only — the Corrosion
mechanism. NuttX has no symlink; its path lives in the leaf's `build.ninja`. So
the tool reported the LIVE nuttx directory as unreachable, and `--prune` would
have deleted 502 MB that twelve leaves were building against. Not corruption,
but a full rebuild plus a tool that lies about what it deletes. Reachability now
counts both mechanisms, verified: the nuttx dir disappeared from the report and
the genuinely-orphaned ones stayed.

A tool that decides what to delete needs to know every way a thing can be
referenced. One consumer was added after it, and that was enough.

## zephyr (2026-08-27): sharing is the WRONG fix here, and the real mass is elsewhere

Investigated with the intent of wiring it like the others. The evidence says do
not, and it is specific rather than cautious:

* **The per-image generated headers live INSIDE the target dir.**
  `<target>/nros-c-generated/nros/nros_config_generated.h` is a byproduct of
  this very build, and it differs by image — a zenoh leaf carries
  `NROS_EXECUTOR_STORAGE_SIZE 308976`, a cyclonedds leaf `89512`. A shared
  directory hands one image the other's sizes. That is the mirror class this
  repo has been burned by six times (0088, 0114, 0122, 0123, 0245, 0268), and it
  fails as a wrong runtime, not a build error.
* **There is little to reuse.** 199 `libnros_c*.a` across the workspace are
  **147 distinct**. Unlike NuttX — where the per-example difference was confined
  to a 736 KB final crate over 715 MB of identical deps — here it goes deep.
* **A key cannot be shown complete.** Kconfig reaches deep crates' build scripts
  through `$DOTCONFIG` and does NOT reliably reach cargo's fingerprint (issue
  0460). An incomplete key is the failure mode, not the fallback.
* **The file already records a MEASURED decision not to share it.**
  `nros_cargo_build.cmake` carries an issue-0616 guard whose comment says
  sharing "bought nothing" and "produced only the collision", with the
  measurement attached.

Four independent reasons pointing the same way is enough. Zephyr keeps per-leaf
target dirs.

### What the duplication actually is

141 west build dirs, **358 GB**. But the mass is not one leaf duplicating
another — it is each leaf accumulating **one cargo directory per profile** and
never dropping the ones a later configure stopped naming. A single build dir held
four: `nros-fast-release` 3.8 GB, `nros-relwithdebinfo` 1.2 GB, `release`
220 MB, plus 4.7 GB under the host triple.

`just gc-zephyr-builds [--prune]` reclaims them. Live profile is read from that
build dir's own `NROS_CARGO_PROFILE_DIR`; everything else under `nros-rust/` is
stale by definition, since a profile the current configure does not name cannot
be feeding the current build. 32 dirs with no readable profile are SKIPPED
rather than guessed at.

**Reported: 88 stale trees, 149.2 GB.** Reporting is the default — deleting that
much build output is the maintainer's call, not a tool's.

### A measurement bug worth recording

The first version identified profile directories by NAME, excluding anything
with two or more dashes as "probably a target triple". `nros-fast-release` has
three. It was skipped — and it is the single largest stale tree on this host. The
tool reported **3.9 GB** where the real figure is **149.2 GB**, a 38x undercount
that looked entirely plausible.

Fixed structurally: a profile directory is one that contains `deps/`. That also
finds stale profiles nested under a triple dir, which the name test missed
entirely. Name shape is not evidence; layout is.

### Lanes, final

| lane | mechanism | outcome |
| --- | --- | --- |
| `threadx_riscv64` | Corrosion | shared, 29 -> 0, 1706 s -> 898 s |
| `native` | Corrosion | shared, 59 -> 0, 186 GB -> 61 GB |
| `threadx-linux` | Corrosion | shared, converts on wipe |
| `freertos` | Corrosion | shared, 12 -> 1 |
| `nuttx` | own cargo driver | shared with `--artifact-dir`, 12 -> 0, 502 MB |
| `zephyr` | own cargo driver | **NOT shared, deliberately** — GC instead, 149 GB |
| esp32 | cargo direct | nothing to share |

Left: the warm floor, which no target-dir change can touch.

### Pruned (2026-08-27): 149 GB reclaimed, builds unaffected

Ran with `--prune` on the maintainer's instruction.

| | |
| --- | --- |
| removed | 88 stale profile trees |
| reclaimed | **149 GB** (454 GB free -> 603 GB free) |
| wall | 529 s |

Matched the reported figure exactly, which is the first thing to check when a
tool deletes at this scale — a prune that frees less than it promised has been
looking at different files than it counted.

Verified afterwards, in the order that matters:

* **45 build dirs still hold their live profile, 0 missing.** This is the claim
  the whole tool rests on.
* Generated headers survived (`nros_config_generated.h` and its stamp) — they
  live beside the profile trees, not inside them.
* The GC re-reports clean.
* **`just zephyr build-c` rc=0 in 337 s.** Structure surviving is not the same
  as builds working, so this is the one that settles it.

## The warm floor was MY OWN regression (2026-08-27)

This issue has said throughout that the warm floor is "70 cargo invocations
re-scanning fingerprints at ~7 s each" and that only fewer invocations could fix
it. That was wrong, and the measurement that shows it is one command:

```
run1  7.59 s   Compiling nros-c, nros-cpp
run2  0.07 s   Finished
run3  0.07 s   Finished
```

**A true no-op cargo is 0.07 s.** The ~6.5 s average was not scanning — it was
recompiling, every single invocation.

### Cause: sharing the target dir falsified a gate's exemption

`nros-build-helpers/src/{c,cpp}.rs` declared

```rust
println!("cargo:rerun-if-env-changed=CORROSION_BUILD_DIR");
```

`CORROSION_BUILD_DIR` is `${CMAKE_CURRENT_BINARY_DIR}` — a PATH, and watching a
path as TEXT is exactly what issue 0491 forbids. It was legitimately exempt in
`check-path-env-fingerprints`, on a stated premise:

> Two builds with different values are different fingerprint namespaces by
> construction, so the two spellings can never meet in one `.fingerprint/`.

True until this issue shared the target dir. Then ~70 spellings landed in ONE
fingerprint namespace, every leaf invalidated the previous leaf's build script,
and `nros-c` + `nros-cpp` recompiled 87 times per warm run.

The gate could not catch it: an exemption is a claim about a fact OUTSIDE the
file it lives in, and a change elsewhere falsifies it silently. The table's own
header says so — "if two builds sharing one `--target-dir` can disagree about
the string, it belongs in the fix, not here". The exemption is now removed, so
the rule is enforced rather than assumed.

### It also explains the earlier mystery

The nuttx section records being unable to explain why a wiped leaf rebuilt
correctly through the lane but not under a bare `cmake`. This is why: the env
watch forced the build script to re-run per leaf, which wrote that leaf's
header. The churn and the correctness were the same mechanism.

So removing the watch broke the fresh-leaf case — reproduced immediately: rc=2,
no header, no binary. Fixed by `scripts/build/mirror-generated-header.sh`, which
prefers the leaf's own copy and falls back to the leaf-independent
`$CARGO_TARGET_DIR/nros-{c,cpp}-generated/` one that build.rs already writes.
Same bytes, verified: two leaves in a key group carry identical headers, which is
guaranteed by the same key that permits them to share at all.

### Measured

| | before | after |
| --- | --- | --- |
| cargo time, warm rebuild | **459.6 s** across 70 (mean 6.57) | **6.7 s** across 70 (mean 0.13) |
| crates recompiled, warm | 87 | **0** |
| wall, warm | 459 s | **362 s** |

Determinism re-checked afterwards: wipe a leaf, rebuild, identical binary
(`7b67e3b99d97ab4cb0131593`).

### The floor moved rather than vanished

Cargo went from 100% of the warm wall to 2%, but the wall only fell 21%. What is
left is not cargo compiling — it is cargo WAITING:

```
112  Blocking waiting for file lock on package cache
 22  Blocking waiting for file lock on build directory
```

That is issue 0648, and phase-371 measured it at 20 samples and called it "NOT
the bottleneck". It was right at the time: compilation dominated. Now that
compilation is gone, the lock is what remains, and the package-cache lock is
GLOBAL — not a consequence of sharing.

Recording rather than chasing: it is a different question, and this issue has
enough evidence of what happens when a tired conclusion outruns its measurement.

## 0648 is NOT the floor — and I made the error that issue exists to warn about

The previous section pointed at cargo's lock waits (112 package-cache, 22 build
dir) as what remained. **Issue 0648 is resolved and refutes exactly that**, with
its durable lesson stated in the title: *"the 274 blocks are real and nearly
free"*. Its measurement: 32x the work in 6.8x the time, per-invocation cost
FALLING monotonically, `--offline` making no difference to block counts.

> **Block COUNT is not a cost measure.** Blocks grow linearly with N ...
> including where scaling is healthy.

I quoted a block count as a cost. That is the same reading error 0648 was closed
for, one issue over, on the same day I had read the file.

### Measured properly, by cost

Lineage + wchan sampling over a 425 s warm run:

| | |
| --- | --- |
| build's own processes alive | 9.4 |
| **runnable** | **0.06** |
| disk-wait | 0.92 |
| leaf state | S 62%, **D 36%**, R 2% |

Leaf blockers by SAMPLE COUNT (occupancy, not events):

```
153  llvm-ar       rq_qos_wait      block-layer writeback throttling
 41  llvm-objcopy  rq_qos_wait
```

Not locks. Archive post-processing, blocked on disk.

### What it was

`cmake/strip-compiler-builtins.sh` runs from the LINK WRAPPER — once per archive
per link, ~190 times per warm rebuild. Each invocation extracts EVERY member
(`llvm-ar p` per object), runs an ELF reader on each, then makes six
`llvm-objcopy` passes. Measured 4.3 s on a 1.6 MB archive **and not faster the
second time**: nothing recorded that the work had already been done.

~817 s of work in a build whose compile step is 6.7 s.

Fixed with a stamp of the archive's size+mtime as the script left it. Unchanged
archive → skip; rebuilt archive → reprocess. Verified both directions: 5.66 s →
0.00 s on repeat, 4.69 s again after a `touch`.

| | before | after |
| --- | --- | --- |
| archive processings, warm | 190 | **17** |
| single invocation, repeat | 4.3 s | **0.00 s** |
| wall, warm | 362 s | **332 s** |

### And the wall barely moved, which is the finding

Removing ~750 s of work bought 30 s. That work was overlapped, not on the
critical path. Three hypotheses have now been measured and each was real but not
the bound: cargo recompilation (fixed, 459.6 s → 6.7 s), lock waits (refuted by
0648), archive post-processing (fixed, 190 → 17).

**A component being large is not evidence that it bounds the wall.** The next
step is a CRITICAL-PATH measurement — per-leaf spans and how many run at once —
not another component hunt. At `alive 9.4` and `runnable 0.06` over 425 s, the
suspicion is that leaves are largely serialised, but that is a hypothesis and
this issue has already recorded what happens to those when they go unmeasured.

The remaining 17 processings are structural rather than wasteful: Corrosion
copies the UNLOCALIZED archive from the shared dir into each leaf, localization
modifies it, so the next build's `copy_if_different` sees a difference and
re-copies, invalidating the stamp. Localizing once in the shared dir would fix
it, but cargo owns that file.

## Critical path measured (2026-08-27): it was a bash for-loop

The previous section said the next step was a critical-path measurement rather
than another component hunt. Done, with a new instrument —
`scripts/build/sample-build-leaves.sh`, which attributes each of the build's
descendants to a LEAF (by lineage, then by build-dir path) and records how many
leaves are active per instant. The existing samplers answer "how busy" and "what
is it blocked on"; neither answers "how many at once", which is the only
question that turns a component's size into its effect on the wall.

Warm `threadx_riscv64`, 331 s, 81% of process-samples leaf-attributed:

```
t+0..60s     6-7 leaves concurrent, 45-59 procs
t+90..330s   ONE leaf, 7-8 procs
```

| leaves active | share of wall |
| --- | --- |
| **1** | **73.8%** |
| 6-7 | 21.0% |

And the tail names itself:

```
t+ 75s  rust/talker/build-cyclonedds
t+ 99s  rust/talker/build-zenoh
t+121s  rust/listener/build-cyclonedds
 …
t+311s  rust/action-client/build-zenoh
```

Twelve rust leaf builds, ~20 s each, strictly one at a time — a plain serial
`for` loop in `just/threadx-riscv64.just`, running beside C/C++ leaves that
`fixtures-build.sh` was already dispatching 7-wide under `make -j8`.

**256 s of a 331 s wall — 77% — was that loop, on a 32-core box.**

### Why every earlier fix disappointed

This issue removed ~750 s of archive post-processing and gained 30 s of wall,
and 452 s of cargo recompilation for 97 s. Both were real; neither was the
bound. They were overlapped work sitting beside a serial critical path, and no
amount of shrinking them could move a wall that loop determined.

That is the durable lesson, and it is the same shape as issue 0648's: **a
component being large is not evidence that it bounds anything.** 0648 warns
against reading an event count as a cost; this warns against reading a cost as a
bound. Both need the measurement that answers the actual question.

### Fixed

The loop now dispatches concurrently at the same width the C/C++ leaves get
(`nros_cargo_frontend_jobs`). Failures stay fatal: a backgrounded job escapes
`set -e`, so each records its own status and the join re-raises — a silent pass
would be the "reports PASS on unmet preconditions" failure gated against
elsewhere in this tree.

| | before | after |
| --- | --- | --- |
| wall, warm | 331 s | **227 s** |
| 1 leaf active | 73.8% of wall | — |
| 12 leaves active | — | **63.2% of wall** |

rc=0, 29 binaries, every rust leaf produced one, 133/133 gates green.

### Warm floor, end to end

| stage | wall |
| --- | --- |
| start of this work | 459 s |
| after the cargo fingerprint fix | 362 s |
| after the archive-processing stamp | 331 s |
| after parallel rust dispatch | **227 s** |

**51% off the warm rebuild**, and the shape is now healthy: 12 leaves in flight
for most of it rather than one.

Not claimed: that this is the floor. The next bound is whatever the new
concurrency profile exposes, and it should be measured the same way rather than
guessed. The same serial-loop shape may exist on other lanes; this measured only
`threadx_riscv64`.

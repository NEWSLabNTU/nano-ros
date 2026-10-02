---
id: 1599
title: "nros-zpico-build's cc-rs compiles watch zenoh-pico by directory, and adopting the depfile pair costs sccache"
status: resolved
type: tech-debt
area: [build, zenoh]
severity: low
found: 2026-10-01
related: [issue-1580, issue-1570, issue-0491]
resolved_in: "branch fix/build-correctness-1593-1596-1599-1605"
---

## What is left

Issue 1580 put every board crate's cc-rs compile, and every other build-script
`cc::Build::compile` in the tree, on issue 1570's helper pair
(`nros_cc_flags::header_deps::{track_header_deps, emit_header_deps}`), and
widened `check-cc-header-deps` to all of them. One file is EXEMPT, by name,
pointing here: `packages/rmw/zenoh/nros-zpico-build/src/runner.rs`, whose four
compiles (`zpico_platform_aliases` on two paths, `zpico`, `zenohpico`) and one
`try_compile` (`size_probe`) still watch their inputs by hand —
`zenoh-pico/{src,include}` as directories, the shim's own `c/` files per file.

So the shape 1580 closed is still open here: a header the shim or the library
includes from outside those paths (a platform header from
`packages/platform/*`, a board include dir a manifest adds) can change without
rebuilding `zpico-sys`.

## Why it was not simply converted in 1580

1580 measured that sccache 0.15.0 restores an object but NOT its implicitly
named `-MMD` depfile on a cache hit, so `track_header_deps` now pins the
compiler and bypasses the `RUSTC_WRAPPER=sccache` fallback cc-rs applies. For
the board archives that is seconds of C. zenoh-pico is the largest C compile
in every image group (the whole library, per platform arm), and it is the one
sccache saves the most on. Converting it trades a rare missed header edge for
an uncached zenoh-pico compile on every cold group — a cost to decide, not to
take silently inside a tech-debt sweep.

## Direction

Measure first: the zenoh-pico compile's cold time with and without the
wrapper, across the fixture groups that build it. Then either convert it
(and drop the directory watches the depfiles make redundant), or find a way
to keep the cache — e.g. a depfile produced by a separate `-M -MM` pass that
sccache never sees. Remove the `runner.rs` entry from `EXEMPT` in
`scripts/check/check-cc-header-deps.py` in the same change.

## Resolution — measured, then converted

**Measurement** (`tmp/zp-measure.sh`: `cargo build -p zpico-sys --features
posix`, a fresh `--target-dir` per row, the build-script RUN duration from
`--timings`; a PRIVATE sccache server — own port and `SCCACHE_DIR` — so the
shared one was untouched; 32-core host, sccache 0.15.0):

| mode | build-script run | sccache C hits |
| --- | ---: | --- |
| no wrapper (what the pair does), fresh target dir | 16.35 s, 17.12 s | — |
| sccache, COLD cache, fresh target dir | 24.38 s | 0 / 134 |
| sccache, WARM cache, ANOTHER fresh target dir | 23.83 s | 8 / 134 |
| sccache, warm, re-run in the SAME target dir | 10.62 s | 142 |
| no wrapper, re-run in the same target dir | 13.83 s | — |
| **converted** (pair adopted, `RUSTC_WRAPPER=sccache` set), fresh dir | 15.54 s | 0 new C requests |

The premise of the exemption was wrong. sccache's key includes the command
line, and every zenoh-pico compile carries absolute `OUT_DIR` paths
(`-I <out>/zenoh-config`, the object path), so a warm cache MISSES in any new
target dir — which is what a new fixture group, a fresh worktree or a CI
checkout at a new path is. There the wrapper's overhead made the build script
~7 s SLOWER. Its one win is a re-run inside the same target dir, ~3 s.

**Decision: adopt the pair.** The `-MF` / separate `-MM` pass alternatives
were not built: they exist to keep a cache that, measured, does not hit where
it would matter. (The same reasoning is why the board crates are not given
sccache back.)

- `nros-zpico-build/src/runner.rs`: all five compiles (two alias TUs, the
  `size_probe` `try_compile` — declared only on success, since a failed probe
  leaves no object — `zpico`, `zenohpico`) go through `track_header_deps` +
  `emit_header_deps`.
- The hand watches the depfiles make redundant are gone: the shim's `c/` files
  and headers, `c/size_probe.c`, and `zenoh-pico/include`. **`zenoh-pico/src`
  STAYS**: `zenoh-sources.txt` selects sources by directory GLOB, so a `.c`
  added under a selected directory is a new member no depfile can name.
- `scripts/check/check-cc-header-deps.py`: `EXEMPT` is empty; the self-test
  exercises the exemption arm with its own table so it stays tested.
- `nros-cc-flags::header_deps`'s doc carries the measurement.

**Edge, measured:** declared inputs went 35 → 405 `rerun-if-changed` lines.
Newly declared, among others: `c/zpico/zpico_config_keys.h` (watched by
nothing before) and `nros-platform-api/include/nros/{platform.h,platform_net.h}`
(previously covered only by a directory watch). `touch
c/zpico/zpico_config_keys.h` → `Dirty zpico-sys … zpico_config_keys.h has
changed` and the build script re-ran. A no-op build before and after the touch
is `Fresh` (no treadmill).

Not measured here: each cross platform individually (their gcc toolchains all
take `-MMD`; they are exercised by this branch's tier-2 build) and the time on
a cross arm.

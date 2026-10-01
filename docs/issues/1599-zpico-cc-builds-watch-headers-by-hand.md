---
id: 1599
title: "nros-zpico-build's cc-rs compiles watch zenoh-pico by directory, and adopting the depfile pair costs sccache"
status: open
type: tech-debt
area: [build, zenoh]
severity: low
found: 2026-10-01
related: [issue-1580, issue-1570, issue-0491]
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

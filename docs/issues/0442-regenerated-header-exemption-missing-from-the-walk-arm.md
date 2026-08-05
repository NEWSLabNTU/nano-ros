---
id: 442
title: The regenerated-header exemption is applied on one arm of the cmake freshness probe and not its sibling, so every freertos/threadx C and C++ zenoh fixture reads STALE
status: resolved  # fixed 2026-08-06
type: bug
area: testing
related: [issue-0222, issue-0196, issue-0433, phase-339]
---

## Symptom

Every freertos and threadx-linux C / C++ zenoh action cell reports:

```
Test fixture is STALE — a source is newer than the built binary:
  binary: examples/qemu-arm-freertos/c/action-server/build-zenoh/c_action_server
  newer:  packages/rmw/zenoh/zpico-sys/c/include/zpico.h
```

The cells never run. Rebuilding does not help for long: the next build of any
other feature set re-stales them.

## Cause — one arm of a two-arm probe

`cmake_dep_info_newer_source` has two arms:

1. the ninja `-t deps` loop, which **does** skip
   `REGENERATED_INPLACE_HEADERS`;
2. `zpico_c_source_newer` → `newest_source_after`, a recursive walk of
   `packages/rmw/zenoh/zpico-sys/c` added to close a gap the dep info misses
   (sources compiled in via a Rust dep's `build.rs`/`cc`), which **did not**.

`zpico.h` is in the exemption list and lives inside the walked tree, so the walk
reported what the loop was written to ignore.

The exemption exists because these headers are cbindgen output written IN PLACE:
a build with a different feature set moves the mtime without changing a byte, so
"newer than my binary" says nothing about that fixture's inputs (issue #222's
cross-family false-stale). Measured here: `zpico.h` mtime 23:46:40 against a
binary at 21:23:15, with `git status` showing the file unmodified — only the
timestamp moved.

Issue 0196's rule, one layer in: a guard whose coverage is narrower than the
rule it enforces. The rule was right and one of its two enforcement points did
not know about it.

## Why it looked like something else

This was first written off as "a core-crate change on a feature branch staled
fixtures built on main" — plausible, wrong, and it would have stayed wrong
because the observable (cells skip) is identical. What settled it was reading
the actual `newer:` path and noticing it was a file the probe already claims to
exempt.

## Fix

`newest_source_after` skips `is_regenerated_inplace_header`, like the loop.

Verified on the `rtos_e2e` action cells: **3 passed → 7 passed** of 9, with
`Freertos::{C,Cpp}` and `ThreadxLinux::{C,Cpp}` recovering. The two still
failing report "not prebuilt" — genuinely never built on that branch — which is
the correct answer rather than a false stale.

## Worth noting for next time

The exemption is now consulted from three places. If a fourth freshness path
appears it will have to be remembered again; the durable shape would be a single
"is this an edit signal?" predicate every arm must route through, rather than a
list each arm opts into.

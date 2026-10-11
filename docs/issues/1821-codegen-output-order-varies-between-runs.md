---
id: 1821
title: "Two runs of the same `nros` binary over the same message closure emit some files with lines in a different order"
status: open
type: bug
severity: low
area: [codegen]
related: [0466]
found: 2026-10-12
---

## Measured

While verifying the phase-390 W6 renames, a 15-package Humble closure
(std/sensor/geometry/nav/example/action_tutorials/rcl/action/lifecycle/
diagnostic/visualization/tf2/map/uuid msgs, with `view` and `heap` fields
configured on msg, srv and action payloads) was generated in Rust, C and C++.
That is 1780 files. Comparing two runs of the SAME binary on the same input,
17 to 20 files differed, and only in LINE ORDER:

- dependency rows in a generated `Cargo.toml`;
- the `#include` order in a generated umbrella header.

Content was otherwise identical, and the W6 renames were checked against this
by sorting before comparing.

## Why it matters

A generator whose output is not a function of its input defeats every
freshness check built on content. Codegen writes through `write_if_changed`, so
a reordered file is a CHANGED file: its mtime moves, and every build edge
downstream of it (fixtures, the C/C++ configure-time emitters of issue 1018)
re-runs on an input that did not change. It also makes a byte comparison of two
generations, the check W6 relied on, report false differences.

## Where to look

Not located yet. The per-message Rust generators already sort their dependency
list (`generator/{msg,srv,action}.rs` collect a `HashSet` into a `Vec`, then
`sort()` and `dedup()`), so the unordered site is elsewhere. The suspected cause
is an unsorted iteration over a `HashSet`/`HashMap` on the package or umbrella
level. Reproduce by generating one closure twice into two directories and
running `diff -r`. Fix by sorting at the emitter, or by switching the
collection to `BTreeSet`/`BTreeMap`, and add a test that generates twice and
asserts the two outputs are byte-identical.

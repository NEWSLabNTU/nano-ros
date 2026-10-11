---
id: 1821
title: "Two runs of the same `nros` binary over the same message closure emit some files with lines in a different order"
status: resolved
type: bug
severity: low
area: [codegen]
related: [0466]
found: 2026-10-12
---

## Resolved 2026-10-12 (PR #1995)

Reproduced with the CLI: two runs of `nros generate all` over a 10-package
Humble closure differed in 10 of 696 files, all package `Cargo.toml`
dependency rows and C umbrella `<pkg>.h` includes. There were two causes:
rosidl-bindgen's `generate_cargo_toml` iterated a `HashSet` straight into
`[dependencies]`, and the C/C++ umbrella emitters wrote their lists in caller
order. The fix covers the class. Every dependency set in rosidl-codegen and
rosidl-bindgen is now a `BTreeSet`, and both umbrella emitters sort and dedup
at the sink. After the fix, 4 runs gave 0 differing files. Old against new,
13 files differ, and each is identical once sorted. The committed
`packages/interfaces` tree is unchanged. Test:
`umbrella_headers_do_not_depend_on_input_order`.

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

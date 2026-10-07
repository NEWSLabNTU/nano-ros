---
id: 1654
title: "Two fixture-free nros-tests targets are red on main and no gating lane runs
  them: `fixture_source_coverage` (an unrowed bin) and `multihost_partition_bake`
  (a retired verb)"
status: resolved
type: bug
area: testing
severity: medium
found: 2026-10-03
related: [1620, 1340, phase-475, phase-432]
---

## How it was found

The phase-475 lane census, three runs in the gate image
(`nros-ci-local:humble-zenoh`, i.e. `nano-ros-ci:humble` with
`ros-humble-rmw-zenoh-cpp`) with nothing staged, on a tree 16 commits behind
`origin/main` at 2026-10-03. Both reds were confirmed still present on
`origin/main` by reading the sources. Both FAIL in all three runs, so neither
target is admitted to `test-lane-contracts`, and nothing else that gates runs
them — issue 1620's class, two more instances.

## 1. `fixture_source_coverage::every_test_bin_is_a_row_or_a_tracked_exception`

```
1 test bin(s) with NO `dir =` row in examples/fixtures.toml and no tracked
exception. ... Add a row, or add a BINS_ALLOWLIST entry naming the lane that
builds it:
  - in-place-subscriptions
```

`packages/testing/nros-tests/bins/in-place-subscriptions` landed in `629b24b6a`
(issue 1340, the eight-subscription in-place measurement) without a
`[[fixture]]` row. The gate is right; the bin needs a row (preferred — it then
gets a coordinate and a lane) or an allowlist entry naming the lane that builds
it.

## 2. `multihost_partition_bake::multihost_bake_emits_only_the_hosts_node`

```
Error: --lang rust entry is retired (phase-432 W2.4): a Rust entry is emitted by
the `nros::main!()` proc-macro at compile time.
```

The test's third seam runs `nros codegen entry --lang rust --model <per-host
model>` (line 194). phase-432 W2.4 retired that verb for Rust, so the seam it
checks — "the bake of a per-host model registers only that host's node" — now
has no Rust spelling at the CLI. It also compiles nothing but does run the CLI
at test time. Options: bake through a language whose entry the CLI still emits
(a C/C++ workspace has the same `multihost.launch.xml`), or move the seam to
the proc-macro side as a build-stage fixture. Not decided here.

## Acceptance

Both targets PASS in the census, and `lane-census-diff.py` reports them NEWLY
ADMISSIBLE.

## 2026-10-03 — item 1 fixed on `main`; item 2 open

A census of `main` after #1610 measured `fixture_source_coverage` PASS in all
three runs — `in-place-subscriptions` got its row there — and it is admitted
whole again. `multihost_partition_bake` still FAILs on the retired
`codegen entry --lang rust`.

## 2026-10-06 — item 2 fixed by issue 1692; its target is no longer fixture-free

[Issue 1692](archived/1692-multihost-bake-test-uses-retired-rust-entry-verb.md)
rewrote `multihost_bake_emits_only_the_hosts_node` to ask the BUILT per-host
entries (rust/c/cpp/mixed x robot1/robot2) which nodes they register, through a
census run. That surface exists in every language, but it needs the multihost
fixtures, so the test now belongs to a fixture lane, not to the gate lane: the
TARGET cannot be admitted whole, and this issue's acceptance ("both targets
PASS in the census ... NEWLY ADMISSIBLE") no longer fits item 2. Its two
fixture-free siblings were already in `.config/lane-admission/gate.txt`. Left
open for whoever owns the census to restate item 2's acceptance or close it.

## 2026-10-07 — resolved

Both reds are gone from `main`. Item 1: `in-place-subscriptions` got its row,
and `fixture_source_coverage` is admitted whole. Item 2: issue 1692 rewrote
`multihost_bake_emits_only_the_hosts_node` to assert on the BUILT entries,
so it resolves a fixture and runs in fixture lanes (tier 1), not the gate lane.
That is the right home for it, not a gap, so item 2's acceptance is restated as
"green where its fixture is built". The target's two fixture-free siblings
stay admitted per test. Nothing left here.

---
id: 540
title: "packages/testing/nros-tests/bins/int32-observer was retired by issue 0128 and the crate survived: no row, no builder, no consumer"
status: resolved
type: tech-debt
area: testing
related: [issue-0128, phase-276]
---

## Problem

29 crates live under `packages/testing/nros-tests/bins/`. 26 have a
`[[fixture]]` row; three do not:

| bin | builder | consumer |
| --- | --- | --- |
| `logging-smoke-zephyr-native-sim` | `zephyr-fixture-leaves.sh:444` | `logging_smoke.rs:261` |
| `ros-edition-pose-pub` | `just ros_editions build-fixture` | `ros_editions_nano_interop.rs:30` |
| **`int32-observer`** | **none** | **none** |

The first two are real fixtures missing a row (issue 0535's class). The third is
dead. `grep -rn 'int32.observer\|int32_observer'` outside its own directory
returns only archived docs, one of which is its own retirement note —
`docs/issues/archived/0128-zephyr-entry-macro-no-params-tiers-lifecycle.md:170`:

> **T0**: `int32-observer` retired; qos/safety e2es ride `int32-sink`.

The retirement landed; the crate directory did not go with it. It still carries
a `Cargo.toml` describing itself as "the cross-process assertion half for
embedded-image e2es" (phase-276 W5) — a role `int32-sink` has held since 0128
T0.

## Why it survived

It has no fixture row, so no coverage gate looks at it:
`examples_fixture_coverage.rs` walks `examples/**` for `package.xml` and never
sees `packages/testing/nros-tests/bins/`. A bin here can exist with no row, no
builder and no consumer and nothing reports it — which is also how the two live
ones above went unnoticed.

## Direction

Delete `packages/testing/nros-tests/bins/int32-observer/`.

Then close the class rather than the instance: extend the coverage gate (or add
its sibling) so every crate under `packages/testing/nros-tests/bins/` is either
a `[[fixture]]` row or a tracked exception with a reason. Without that, the next
retired fixture bin sits here just as long.

## Status: instance deleted (phase-350 W0, 2026-08-13), class still open

The crate is gone. **This issue stays open for the class half** — nothing yet
stops the next orphan, and the two LIVE bins with no row
(`logging-smoke-zephyr-native-sim`, `ros-edition-pose-pub`) are still unrowed;
they are issue 0535's set and phase-350 W6's gate. Closing this on the deletion
alone would be fixing the reported site and not the class.

## Closed 2026-08-13 — the class is enforced now

The deletion was never the point; this stayed open for the class. phase-350 W6
closed it: `packages/testing/nros-tests/tests/fixture_source_coverage.rs`
asserts every crate under `bins/` is a manifest row or a tracked exception with
a reason, and fails in three directions (uncovered bin, exception that gained a
row, declared producer that vanished) — each verified red before being trusted.

The two LIVE bins this issue named as unrowed are handled: `logging-smoke-zephyr-
native-sim` has a `builder = "west"` row (and issue 0549 removed its duplicate
builder), and `ros-edition-pose-pub` is an allowlisted exception naming the
RFC-0058 edition axis that builds it.

So the next orphan fails a gate instead of sitting for months.

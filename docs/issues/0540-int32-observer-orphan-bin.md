---
id: 540
title: "packages/testing/nros-tests/bins/int32-observer was retired by issue 0128 and the crate survived: no row, no builder, no consumer"
status: open
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

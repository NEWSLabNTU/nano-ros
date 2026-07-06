---
id: 142
title: "`examples/stm32f4/rust/talker` declares BOTH `[…nros.entry]` and `[…nros.node]` — `example_shape::component_or_application_classification_present` red since the 0100.W4 collapse"
status: resolved
type: tech-debt
area: examples
related: [issue-0100, phase-244]
resolved_in: "phase-142-example-shape-entry-node (2026-07-07)"
---

## Resolution

Direction 1 (teach the test). The test contract was stale, not the manifest:
the CLI schema `PackageMetadataNros::validate`
(`packages/cli/nros-cli-core/src/orchestration/cargo_metadata_schema.rs`) is the
SSoT and it deliberately makes `{component/node, application}` mutually
exclusive **but leaves `entry` out of that mutex** — a collapsed
self-dispatching Entry crate (the issue-0100 W1–W7 Entry/Node collapse)
legitimately declares BOTH `[package.metadata.nros.entry]` (its deploy board)
and `[package.metadata.nros.node]` (the node it registers via `nros::node!(…)`
in the same crate). Every 0100 collapse uses this shape and all build-verify.

`example_shape::component_or_application_classification_present`
(`packages/testing/nros-tests/tests/example_shape.rs`) counted
`is_component + is_application + is_entry` as exactly-one, so it flagged the
valid entry+node combo as "declares MORE than one." Replaced that count with
the CLI's own rule: `application` must stand alone; `entry` may coexist with a
node/component; every leaf must classify as at least one shape.

Only `examples/stm32f4/rust/talker` was red because `qemu-arm-baremetal/`
(which collapsed the same way in W1–W7) is not in the test's
`MIGRATED_PREFIXES`, while `stm32f4/rust/` is — so the collapsed baremetal
crates were never checked. The new rule is forward-compatible: when the
baremetal trees migrate, their collapsed entry+node crates pass.

All 7 `example_shape` tests green.

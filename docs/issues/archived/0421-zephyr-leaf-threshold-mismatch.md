---
id: 421
title: "zephyr_leaf_buildrs_uses_shared_bake demanded >=13 leaves after phase-331 deleted four"
status: resolved
type: bug
area: testing
related: [phase-291, phase-331, issue-0196]
---

## Symptom

`just ci` (tier 1) failed `test-all`:

```
thread 'zephyr_leaf_buildrs_uses_shared_bake' panicked at
  packages/testing/nros-tests/tests/example_shape.rs:841:5:
expected >=13 zephyr rust leaf build.rs, walked only 10 — layout moved?
```

## Cause

The assertion is a silent-empty guard: the test walks `examples/` for zephyr
Rust leaves, checks each `build.rs` calls the shared
`nros_zephyr_build::bake_nros_config()`, then asserts a floor so a layout change
cannot turn the check into a vacuous pass.

The floor was phase-291's count of 13. phase-331 W3/W4 then deleted four themed
micro-workspaces (`ws-{lifecycle,params,qos,safety}-rust`), each of which
carried a `zephyr_entry` leaf, leaving 10 — seven under `examples/zephyr/rust/`
plus `realtime-rust`'s and `rust`'s (`zephyr_entry` + `zephyr_entry_robot1`).
The floor was not moved with the deletion, so the guard failed on its own stale
constant rather than on drift.

## Resolution

Fixed upstream in `1f19ea937` ("fix: four stale gates, one of them a regression
this phase introduced"): the floor is 10, and the comment records WHICH
deletion moved it and that a floor moves with a real deletion, never to make a
red go away.

Verified on 2026-08-05: `cargo nextest run -p nros-tests --test example_shape`
→ 11 passed, 0 skipped.

## Correction to the original report

This issue was first filed claiming the tree has **7** leaves and that
phase-277 was responsible. Both were wrong. The 7 came from grepping only
`examples/zephyr/rust/**/build.rs` while the discovery rule ALSO matches any
`zephyr_entry*` package directory — three more. The real count is 10, the real
cause is phase-331's deletion, and the fix had already landed upstream and was
in my tree via rebase when I wrote the report; I was reading a failure captured
before that rebase.

The filing note about the walker counting untracked build output (10 vs 7 on
disk) was the same mistake seen from the other side — not a `find`-vs-index
defect. `example_shape` walks the filesystem by design because it audits
example SHAPE, including files that are not tracked.

---
id: 2992
title: "The shared launch-seed loop holds an ~8 KiB parameter value in its frame, over the 4 KiB frame cap"
status: resolved
type: bug
severity: high
area: [params, node]
related: [phase-566, 2815, 2969]
found: 2026-11-27
resolved: 2026-11-27
resolved_in: "fix(params, issue 2992): a launch seed is parsed into its store slot, not the seeding frame"
---

## What happened

Tier 2's `check max-frame` failed on `qemu_bsp_talker_bringup@mps2-an385-baremetal`:
`nros::node_runtime::seed_launch_params_on` had an 8,728 B frame against the
4,096 B cap, as a function the shrink-only baseline had never seen.

Phase 566 W4 moved the launch-seed loop out of `ExecutorSink::create_node`
into `seed_launch_params_on`, shared with the Rust metadata probe. The loop
parsed each launch value into a local `ParameterValue` and declared it by
reference. A `ParameterValue` is as large as its largest variant, ~8 KiB at
the default capacities, so that local was the whole frame. Inside
`create_node` the same bytes had been part of a frame the baseline already
held; as its own function it was new.

## Fix

The store parses the launch text straight into the slot it declares
(`ParameterServer::seed_launch_text`, reached through `ParameterServerOps`
and `Executor::seed_launch_text_on`). The loop holds no `ParameterValue`. A
text the store cannot hold, and `[]`, leave the slot empty again with no
count and no event, so a refusal leaves the table as it was.

Measured on the same row after the fix: `seed_launch_params_on` is
`push {r4-r7, lr}` + `sub sp, #364`, about 384 B, and `check max-frame`
passes over 92 reports with no baseline row and no `frame_allow` added.

One behaviour moved: a launch text for a name the store already holds is now
reported as already declared before its text is parsed, so a malformed value
for such a name is no longer refused; the existing value stands either way.

## Regression

`check max-frame` (tier 2, nightly, full) fails any nano-ros frame over
4,096 B that the baseline does not hold, and this function has no baseline
row. `typed::tests::a_launch_seed_from_text_is_parsed_into_its_slot` checks
the seeding itself: a seed is adopted by the first typed declaration, and a
refused text or `[]` declares nothing.

The C and C++ seed (`nros-cpp`'s `params_shim.rs`) still parses into a local
`ParameterValue`. No `check max-frame` run has reported its frame, and it
can take the same call.

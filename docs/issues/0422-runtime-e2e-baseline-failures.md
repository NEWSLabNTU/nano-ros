---
id: 422
title: "19 runtime E2E failures on a clean tree — triage index (8 diagnosed, 11 open)"
status: open
type: bug
area: testing
related: [issue-0427, issue-0428, issue-0429, phase-336, rfc-0051]
---

## Symptom

`just ci` (tier 1) passes every gate, then fails `test-all`. On 2026-08-05,
fixtures freshly built for the native lane:

```
Summary [115.885s] 1257 tests run: 1231 passed, 25 failed, 1 timed out, 72 skipped
Real failures: 19
```

The same families failed on a fresh clone before any of this session's work, so
this is the standing state of the tree, not a regression.

## Triage result (2026-08-05)

**The original framing of this issue was wrong and is corrected here.** It said
the failures were "plausibly environmental — zenohd / CycloneDDS / ROS not
confirmed present". They are present (`zenohd` in the SDK store, `ROS_DISTRO=
humble`, `ros2` on PATH), and every failure examined so far has been a real
defect. Triage, not environment, was the right instinct; the guess about WHICH
way it would fall was not.

Eight of the nineteen now have root causes, in three separate bugs:

| Failures | Cause | Issue |
| --- | --- | --- |
| `cpp_multi_node_entry` | a resolver fix never reaches an existing SystemModel — freshness ignores the resolver | **0427** |
| 5 × `native_api` cyclonedds + `test_threadx_linux_cyclonedds_service` | node registration fails on the Cyclone backend; error collapsed to opaque `NodeRegister` | **0428** |
| `nano2nano` gid + sequence | tests grep listener trace output the binary no longer emits | **0429** |

`cpp_multi_node_entry` is verified fixed by regenerating the model (4 passed).

## Remaining, not yet triaged

- `large_msg::test_xrce_e2e_integrity` — "Expected 0 invalid messages, got 15"
- `xrce_ros2_interop::test_ros2_action_xrce_client` — accepted, no feedback
- `native_orchestration_tiers` ×2 — binary never reaches the run_tiers boot path
- `native_orchestration_misuse::launch_arm_is_a_removal_error` — expected a
  refusal, the check succeeded
- `zero_copy::test_zero_copy_message_info` — listener emitted no sequence
  markers (possibly the same shape as 0429)
- `native_example_pubsub_e2e`, `native_example_reqresp_e2e`,
  `realtime_tiers_e2e` — cell failures inside larger matrices
- `logging_smoke_mps2_baremetal_emits_every_severity` — fixture not built for
  this lane (`just qemu build-fixtures`), likely a lane-coverage artifact rather
  than a defect

## Method note

Each of the three diagnosed bugs was found the same way: reproduce OUTSIDE the
test harness, then compare against a working sibling. The Cyclone failure looked
identical to the zenoh path until the binary was run directly; the nano2nano
"got 0" looked like a delivery failure until the listener was run against a
router and observed to publish fine while emitting no trace markers at all.

Reading the failure message alone would have produced three plausible and wrong
fixes.

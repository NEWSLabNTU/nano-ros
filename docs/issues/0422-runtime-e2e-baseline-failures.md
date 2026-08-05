---
id: 422
title: "~16 runtime E2E tests fail on a clean tree — triage needed before they are trusted"
status: open
type: bug
area: testing
related: [issue-0421, phase-336, rfc-0051]
---

## Symptom

`just ci` (tier 1) ends with `test-all` failing. On 2026-08-05, with every gate
green and fixtures freshly built:

```
Summary [121.784s] 1251 tests run: 1226 passed, 23 failed, 2 timed out, 72 skipped
Real failures: 18 / 18
```

The same families failed on a fresh clone at the START of that session (18
failures, before any local work), so this is the standing state of the tree, not
a regression from a particular change.

## The set

Runtime E2E, grouped by what they exercise:

- **CycloneDDS interop** — `native_api::test_native_cyclonedds_rust_service`,
  `…_rust_talker_to_listener::{C,Cpp}`, `…_talker_to_rust_listener::{C,Cpp}`
- **Zenoh transport** — `nros-rmw-zenoh::zenoh_integration
  two_sessions_deliver_cross_session_through_router`, `nano2nano
  test_gid_consistency`, `nano2nano test_sequence_number_increment`
- **Orchestration runtime** — `native_orchestration_tiers` (x2),
  `native_orchestration_misuse launch_arm_is_a_removal_error`,
  `realtime_subnode_cpp_e2e`, `realtime_subnode_cpp_portable_e2e`
- **Board / run-plan** — `baremetal_run_plan_runtime`,
  `board_agnostic_run_plan`, `logging_smoke_mps2_baremetal_emits_every_severity`
- **Workspace build** — `cpp_multi_node_entry`, `entry_e2e entry_matrix`,
  `nav2_compat n11_launch_xml_ros2_compat_smoke`
- **Timeouts (60s)** — `native_example_pubsub_e2e`,
  `native_example_reqresp_e2e`

## Why this needs triage before fixing

The failure MODES differ, and at least three explanations are live:

1. **Environmental.** Several need a running `zenohd`, a CycloneDDS install, or
   a ROS 2 environment. `just doctor` and `source ./activate.sh` are the
   documented prerequisites, and it is not established that this host satisfies
   all of them. A test that fails for a missing service is a setup gap, not a
   bug — but per CLAUDE.md it must still FAIL rather than silently skip, so
   "fails here" does not by itself locate the problem.
2. **Genuinely broken.** The two 60-second timeouts are not a missing-service
   shape; something starts and never delivers.
3. **Stale expectations.** `example_shape::pre_212_files_forbidden_in_migrated
   _examples` fails on stray `metadata/*.json` build artifacts that `nros sync`
   leaves in example dirs. Those are gitignored and absent from a clean clone,
   so this one is an artifact-hygiene issue rather than a product defect — it
   passes once the 84 stray `metadata/` dirs are removed.

Fixing before triage risks papering over (1) with a skip, which is exactly the
failure mode CLAUDE.md calls out: a test that reports PASS on an unmet
precondition is worse than one that fails.

## Suggested first step

Run the suite on a host where `just doctor` is fully green, and diff the failure
set. What survives is the real list. Then split this issue: environmental gaps
become setup/doc work, the timeouts get their own investigation.

## Notes

Recorded while verifying phase-336 (build-profile propagation). Not caused by
it: the same families fail on a clean checkout with none of that work applied.
Two failures that WERE in this set are already resolved — the empty-profile bug
(fixed in phase-336 W7) and the committed-SystemModel class (upstream #414).

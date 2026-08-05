---
id: 422
title: "10 runtime E2E failures on FRESH fixtures — triage index"
status: open
type: bug
area: testing
related: [issue-0427, issue-0428, issue-0429, phase-336, rfc-0051]
---

## Symptom

`just ci` (tier 1) passes every gate, then fails `test-all`. On 2026-08-05, with
the native lane rebuilt IMMEDIATELY before the run:

```
Summary [118.899s] 1259 tests run: 1242 passed, 17 failed, 72 skipped
Real failures: 10
```

## The number depends on fixture freshness — measure after a rebuild

An earlier run of the same tree reported **19**. Nine of those were STALE
FIXTURES, not defects: upstream's `ad7752bc9` arrived in a rebase and touched
`packages/api/nros/src/node.rs`, so every fixture built before it read stale.
Any pull or rebase does this (CLAUDE.md, "fixture mtime treadmill").

That cost a wrong issue — 0428, filed against CycloneDDS, was the stale-fixture
symptom. **Rebuild the lane, then triage.** A failure list captured across a
rebase is measuring the rebase.

## Diagnosed

| Failures | Cause | Issue |
| --- | --- | --- |
| `nano2nano` gid + sequence | tests grep listener trace output the binary no longer emits; re-verified on fresh fixtures | **0429** |
| ~~cyclone × 5~~ | ~~backend~~ — stale fixtures; 8/12 pass after rebuild | 0428 (filed in error) |
| ~~`cpp_multi_node_entry`~~ | stale SystemModel — a resolver fix never reaches an existing model | **0427** (real; test passes after regenerating) |

## Remaining, untriaged (8)

- `large_msg::test_xrce_e2e_integrity` — "Expected 0 invalid messages, got 15"
- `xrce_ros2_interop::test_ros2_action_xrce_client` — accepted=true,
  got_feedback=false
- `native_orchestration_tiers` ×2 — binary never reaches the run_tiers boot path
- `native_orchestration_misuse::launch_arm_is_a_removal_error` — expected a
  refusal, the check succeeded
- `realtime_tiers_e2e::realtime_tiers` — 1 of 16 rows
- `zero_copy::test_zero_copy_message_info` — no sequence markers (may be 0429's
  shape; the other two zero_copy tests pass now)
- `logging_smoke_mps2_baremetal_emits_every_severity` — fixture from the qemu
  lane, not built here; likely lane coverage rather than a defect

## Method note

Reproduce OUTSIDE the harness, then compare against a working sibling — that is
how 0427 and 0429 were found. But 0428 shows the limit: reproducing a SYMPTOM
outside the harness proves the symptom is real, not that you have its cause. The
binary was stale, and a stale binary fails the same way a broken backend does.

Check freshness the way the harness does (the whole input set), not by
hand-picking one source file to compare against.

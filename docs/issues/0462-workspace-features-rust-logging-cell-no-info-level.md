---
id: 462
title: "`workspace_features` rust/logging: the node's log lines carry no `[INFO]` level tag — 0 of an expected ≥3"
status: open
type: bug
area: runtime
related: [issue-0422, phase-264]
---

## Symptom

`workspace_features_e2e::workspace_features`, cell `rust/logging`, reproducible
SOLO (not a sweep flake — see below):

```
workspace_features: 1 of 17 cell(s) FAILED:
  rust/logging: [rust logging] expected ≥3 node log lines carrying `[INFO]` on
  stdout, got 0. A line with the marker but no level tag means the record
  bypassed the logging facade (a direct write) or lost its metadata.
nros: session open
talker publishing chatter seq=0
talker publishing chatter seq=1
talker publishing chatter seq=2
```

The node IS running and IS producing its marker lines — three of them, in order.
What is missing is the level tag: the test wants `[INFO]`-carrying records and
counts zero.

The assertion's own message names the two candidate causes, and the captured
output does not yet distinguish them:

1. the record **bypassed the logging facade** (a direct `println!`/write), or
2. it went through the facade but **lost its metadata** (level/target dropped by
   the sink or the formatter).

## Why this is not the sweep flake class

The full `test-all` run surfaced seven junit failures. Re-run individually, with
the fixture mtime probe bypassed:

| test | solo |
| --- | --- |
| `large_msg::test_xrce_e2e_integrity` | passes |
| `xrce_ros2_interop::test_xrce_action_ros2_client` | passes |
| `native_example_reqresp` | passes |
| **`workspace_features`** | **fails** |

So three were genuine load flakes under a 1259-test parallel run (the class
CLAUDE.md says to retest solo before filing) and this one is real.

`native_orchestration_tiers` ×2 (issue 0438) and
`zero_copy::test_zero_copy_message_info` (issue 0441) are already-known and
account for the rest.

## Not in the #0422 baseline

#0422's host baseline and its "remaining, untriaged" list do not include
`workspace_features`. Its logging entry is a different test —
`logging_smoke_mps2_baremetal`, which was diagnosed there as a lane-coverage
NAMING problem and renamed to
`logging_smoke_qemu_baremetal_mps2_emits_every_severity`. That fix does not
touch this cell.

## First step

Read the fixture's talker source and establish which of (1)/(2) it is before
touching either side — a direct `println!` and a metadata-losing sink need
opposite fixes, and the test's own message already frames the question. If it is
(1), the cell is asserting a contract the fixture never promised and one of the
two has to move.

## Notes

Found running `test-all` at native scope while verifying the #0447/#0458 fixes
(2026-08-06). Unrelated to those: that work touched `nros-cpp`'s handle tag,
`nros::send_goal`'s CDR framing, and the linux board's tier-registration mutex —
no path to a talker's log level. Recorded here rather than left in a sweep log so
it is tracked rather than rediscovered.

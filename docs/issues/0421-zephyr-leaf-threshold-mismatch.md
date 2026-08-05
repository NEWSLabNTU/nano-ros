---
id: 421
title: "zephyr_leaf_buildrs_uses_shared_bake demands >=13 leaves; the tree has 7"
status: open
type: bug
area: testing
related: [phase-277, phase-291, issue-0196]
---

## Symptom

`just ci` (tier 1) fails `test-all`:

```
thread 'zephyr_leaf_buildrs_uses_shared_bake' panicked at
  packages/testing/nros-tests/tests/example_shape.rs:841:5:
expected >=13 zephyr rust leaf build.rs, walked only 10 — layout moved?
```

Reproduces on a clean `origin/main`. The count in the message varies with what
else is on disk (10 when stray build dirs are present, 7 on a clean tree) —
which is itself a signal, see below.

## Cause

The assertion is a silent-empty guard: the test walks `examples/` for zephyr
Rust leaves, checks each `build.rs` calls the shared
`nros_zephyr_build::bake_nros_config()`, and then asserts it walked at least 13
so that a layout change cannot turn the check into a vacuous pass.

The tracked tree has **7**:

```
$ git ls-files 'examples/zephyr/rust/**/build.rs' | wc -l
7
  action-client, action-server, listener, service-client,
  service-server, talker, talker-aemv8r
```

phase-277 consolidated the zephyr examples (`4cbdf8dc1` matched the official ROS
2 demos, `1e2ce89aa` removed a leftover `service-client-async`), and the
threshold was not moved with them. So the guard has been failing since — not
detecting drift, just reporting its own stale constant.

## Fix

Decide which the number is meant to be, then make it derived rather than
literal:

- If 7 is correct, the assertion should be `== <count of tracked leaves>`,
  computed from `git ls-files` the way other gates in this file do, so the next
  consolidation updates it automatically instead of failing.
- If leaves are genuinely missing (phase-291 declared 13), the gap is the bug
  and the threshold is right — but then the message should name WHICH leaves it
  expected, because "walked only 7" does not tell the reader what to restore.

Note the count also moves with untracked build output on disk (10 vs 7), so the
walker is reaching into directories a tracked-file walk would exclude. That is
the `check-no-tracked-file-find` class (an index lookup, not a filesystem walk)
and probably wants fixing at the same time.

## Notes

Found while verifying phase-336 (build-profile propagation); unrelated to it.
Left open deliberately: the correct threshold is a phase-277/291 judgment about
which leaves should exist, not a number to pick to make the gate pass.

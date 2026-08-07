---
id: 469
title: "Phase 209's C++ port templates were in no lane, so the acceptance stopped holding unnoticed"
status: resolved
type: bug
severity: medium
area: testing, cpp
related: [issue-0465, issue-0317, issue-0196]
resolved_in: phase-209
---

## Finding

The three phase-209 port templates — `cpp-port-minimal-publisher`,
`rclcpp-compat-smoke`, `topic-state-monitor-port` — were referenced by no
fixture row, no test, and no recipe:

```console
$ for t in cpp-port-minimal-publisher rclcpp-compat-smoke topic-state-monitor-port; do
    git grep -l "$t" -- examples/fixtures.toml packages/testing just scripts | wc -l
  done
0
0
0
```

Nothing built or ran them between 2026-05-30 and 2026-08-07. In that window the
acceptance stopped holding (issue 0465 — the rclcpp shim opened a second RMW
session, so the node died at startup) and the phase read "MVP DONE" throughout.

## Resolution

**Build:** three `compile_check_fixture` rows (`cmake-configure`) in
`examples/fixtures.toml`, so `build-test-fixtures` produces the binaries into
`build/cmake-fixtures/<id>/`.

**Run:** `packages/testing/nros-tests/tests/port_templates_e2e.rs` starts a
router and asserts the vendored tutorial node actually publishes
(`nros_tests::output::CPP_PORT_PUBLISH_MARKER`).

A build row alone would NOT have caught 0465 — **the template compiled and
linked cleanly the entire time it was broken.** The acceptance is "compiles +
links + RUNS" and only the third part was lost, so the third part is what the
test asserts.

## The test was verified to FAIL

A test that cannot fail is worth nothing, so it was checked against the real
regression rather than assumed. Rebuilding the fixture with the pre-fix shim
(`git show <fix>^:…/rclcpp_compat.hpp`):

```
with the 0465 bug:   FAILED — "did not publish through nano-ros"
with the fix:        1 passed
```

That exercise caught a defect in the test's first version, which is recorded
separately: `wait_for_output_pattern` returns `Ok(output)` on TIMEOUT whenever
the process printed anything at all, so checking only the `Result` asserts
nothing. The first version passed against the deliberately broken fixture — the
failing node's `Transport(InvalidConfig)` line is non-empty output. The test now
asserts on the returned CONTENT. See the follow-up issue for the other ~233 call
sites with that shape.

## Not covered

Only the minimal publisher has a runtime assertion. `rclcpp-compat-smoke` and
`topic-state-monitor-port` get build coverage from their rows; giving them
runtime cells needs a router-per-fixture story their READMEs do not yet define.

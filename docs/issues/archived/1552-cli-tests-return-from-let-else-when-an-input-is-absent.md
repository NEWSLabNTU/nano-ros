---
id: 1552
title: "Fourteen `packages/cli` tests `return` from a `let … else` when an input is absent, so they PASS having run nothing"
status: resolved
resolved: 2026-09-29
type: bug
area: [testing, cli]
severity: medium
found: 2026-09-28
related: [1539, 1544, 0693, 1160, phase-472]
---

## What happens

Issue 1539 widened `check-test-precondition-guards` with a rule on the `else`
arm of a `let … else` in a test body: that arm runs only when the pattern did
not match, so it is always the failure path, and a bare `return` there is a
PASS. The rule found 19 sites beyond 1539's five. The five in `nros-tests` were
converted to `skip!` in the same change. These fourteen, all under
`packages/cli/`, were not:

| File | Sites | Absent input |
| --- | --- | --- |
| `packages/cli/rosidl-codegen/tests/parity_test.rs` | 9 | a ROS 2 install / msg package (`ros_input_dir`) |
| `packages/cli/rosidl-codegen/tests/comparison_test.rs` | 3 | the same |
| `packages/cli/nros-cli-core/src/orchestration/metadata_refresh.rs` | 1 | the sibling `node_metadata.rs` ("packaged crate") |
| `packages/cli/nros-cli-core/src/source_stamp.rs` | 1 | a git checkout |

They are held by a per-file, shrink-only `ARM_RULE_BASELINE` inside
`scripts/check-test-precondition-guards.py`, so a new site fails, but these do
not.

## Why they were not converted mechanically

`packages/cli` is a separate cargo workspace with no `nros_tests` dependency and
so no `skip!`. The parity tests return BY DESIGN (issue 0693 / 1160: the
`[NO-ROS]` / `[NO-PKG]` lines in `parity_helpers.rs` say "this test did not
run"), because the ROS-less `check-cli-tests` lane runs plain `cargo test`,
where a skip-panic is a FAILURE. Turning each `return` into a `panic!` would
make that lane red on every ROS-less host, which is where it exists to run.

So the fix needs a decision, not an edit: how does the CLI workspace spell a
skip that its own lane can tolerate? (Options: run `check-cli-tests` through
nextest with the same junit skip rewrite `test-all` uses; or move the
ROS-dependent parity tests behind `#[ignore]` with a lane that runs them
`--ignored` where ROS exists.) The two `nros-cli-core` unit tests are simpler —
the input always exists in-tree, so an `.expect()` is honest — but they share
the lane question.

## Fix

Decide the CLI skip spelling, convert the fourteen, and delete
`ARM_RULE_BASELINE` (it only exists for these).

## Resolution (2026-09-29, phase-472 F4)

The CLI workspace's skip is libtest's own IGNORED verdict, plus a lane that
runs the ignored tests where their input exists:

- The twelve rosidl-codegen parity/comparison tests are
  `#[ignore = "needs a ROS 2 install; …"]`, and their `else` arm calls
  `parity_helpers::ros_input_absent() -> !`, a PANIC. A ROS-less `cargo test`
  therefore REPORTS them ignored (`N ignored` in the summary) instead of
  passing them empty; run with `--ignored` on a host that cannot supply the
  input, they FAIL, after the existing `[NO-ROS]`/`[NO-PKG]` line that says
  which state the host is in.
- `just check cli-tests` runs the workspace as before, then — when a ROS 2
  install is found by the same rule as `parity_helpers::ros_share_root`
  (`$ROS_DISTRO`, else exactly one `/opt/ros/<d>/share`) — runs
  `-p rosidl-codegen --test parity_test --test comparison_test -- --ignored`.
  Where none is found it records `NOT VERIFIED` in the `nros_check_skip`
  ledger (`nros_check_unverified`, phase-472 F2), so the lane's closing line
  names what did not run.
- The two nros-cli-core unit tests PANIC: the crate is built and tested from a
  checkout, so the sibling `node_metadata.rs` and `git ls-files` are always
  there, and their absence is a moved file or a broken environment.

`ARM_RULE_BASELINE` is deleted from `check-test-precondition-guards`: every
rule-2 site fails with no per-file allowance. Proof: the new gate over the
fourteen unconverted sites rc=1 (14 findings), the old gate rc=0.

**Measured on this host** (ROS 2 Humble at `/opt/ros/humble`):
`cargo test --manifest-path packages/cli/Cargo.toml --workspace` green, and
the lane's `--ignored` pass ran all twelve: 3 + 9 passed. `cargo clippy -p
rosidl-codegen -p nros-cli-core --all-targets -D warnings` clean.

**Not verified:** a ROS-less host end to end (the `/opt/ros` fallback cannot
be hidden without root); the CI `check` job, which has no ROS, will now print
the ignored count and a ledger line instead of twelve empty passes.


---
id: 1718
title: "The live-peer host lane runs cells whose test binaries need fixtures its
  build step never builds — `cpp_robot_entry` and `rosout-talker-c` fail as
  FixtureNotBuilt"
status: resolved
resolved: 2026-10-07
type: bug
area: [ci, testing]
severity: medium
found: 2026-10-06
related: [1687, 1691, 1693, 1666, 1353, 0584]
---

## What was measured

Scheduled `live-peer regression` run **37413064249** (head `0a3c601ce`), job
**112105584775** "rows whose board IS this runner" (host runner). For the first
time in a while this job got past its build step ("Build the fixtures those rows
resolve" — success) and ran its cells ("Run the cells with a recorded PASS" —
failure). Six tests failed. Two have issues of their own:

- `qos_override_e2e::a_ros2_peer_sees_the_overridden_publisher_profile` → issue 1687
- `ros2_action_e2e::the_nano_ros_action_client_drives_a_stock_ros2_server_over_zenoh` → issue 1691

The other four are not runtime failures. The fixture they need was never built
in this job:

```
nros-tests::cpp_multi_node_entry multi_node_workspace_cpp_typed_configures_and_builds
nros-tests::cpp_multi_node_entry multi_node_workspace_cpp_typed_pubsub_e2e
nros-tests::cpp_multi_node_entry multi_node_workspace_cpp_per_node_graph_nodes
    Error: FixtureNotBuilt("Test fixture binary not prebuilt:
      .../build/cmake-fixtures/cpp_robot_entry/build/posix-zenoh-native/cmake/native_entry")

nros-tests::rosout_interop a_c_log_call_reaches_ros2_topic_echo_rosout
    [SKIPPED] fixture not built: prebuilt rosout-talker-c: Test fixture binary not prebuilt:
      .../packages/testing/nros-tests/bins/rosout-talker-c/build-zenoh/rosout_talker_c
```

The lane selects cells by their ledger row (`native-multinode-cpp-zenoh`,
`native-logging-rust-zenoh-n2r`, …) and builds "the fixtures those rows
resolve". It then runs the whole test BINARY behind each cell. Those binaries
also contain cases whose fixtures the rows do not resolve:

- `cpp_robot_entry` is a `[[compile_check_fixture]]` (`build-compile-check-fixtures`).
- `rosout-talker-c` is the C case in a binary whose recorded row is the Rust one.

## What it is not

- Not disk (issue 1353) and not a missing tool (issue 1666): the build step
  succeeded and the cells ran.
- Not issue 1693: that was the sibling job's census prepass, and it was fixed
  after this run's head.
- Not a runtime defect in the four tests: none of them ran its fixture.

## What would close it

Either of these:

- The lane's build step builds every fixture the selected test binaries resolve.
- The run narrows to the recorded cases, so a binary's other cases are
  deselected (a skip with a reason, not FixtureNotBuilt).

The check is the next scheduled `live-peer regression`: its board job should
report no FixtureNotBuilt.

## Resolution

PR #1753, by the first road. `check-interop-verdicts.py --list-passing
--runner host --host-compile-checks` derives the `[[compile_check_fixture]]`
ids the passing host cells' test files name as quoted literals (ids read from
`examples/fixtures.toml`, tests from `nros-tests/tests/`), and the live-peer
host fixture step builds each with `NROS_FIXTURE_ID=<id>
compile-check-fixtures.sh`. On the current ledger that is `cpp_robot_entry`.

Measured locally: the by-id build produces `native_entry`, and
`cargo nextest run -p nros-tests --test cpp_multi_node_entry` passes 4/4.

`rosout-talker-c` needs nothing: its case belongs to
`native-logging-c-zenoh-n2r` (`interop::CASE_CELLS`), which has never passed,
and the run already reports it as a skip with a reason
(`rewrite-skipped-junit: skips by class: capability=1`), not as a regression.
Still to confirm: the next scheduled host job reports no FixtureNotBuilt.

## 2026-10-07 — the confirmation run still failed, because the lane skipped

The "still to confirm" run came back red. `live-peer regression` run
**37570821619** (schedule 04:18Z, head `edbe96f92`, which includes #1753), job
**112628797050** `rows whose board IS this runner`: `cpp_multi_node_entry` 3 of 4
FAIL with `FixtureNotBuilt("Test fixture binary not prebuilt: …/cpp_robot_entry/
build/posix-zenoh-native/cmake/native_entr…")`.

The derivation worked and named the right id, but the build skipped:

```
host compile-check fixtures (derived from the cells' tests):
  cpp_robot_entry
=== compile-check-fixtures.sh (NROS_FIXTURE_ID=cpp_robot_entry) ===
cmake-fixtures: play_launch_parser not found (source ./activate.sh) — skipping (recorded in the summary)
compile-check: 2 lane(s) SKIPPED — their fixtures are NOT built:
```

The live-peer host job never provisions `play_launch_parser`, and
`compile-check-fixtures.sh` treats its absence as a host-capability SKIP that
exits 0, so the step went green with nothing built. The local measurement in the
Resolution passed because that host has the parser on PATH.

Follow-up fix: the step runs `nros setup --tool play_launch_parser` and
re-sources `activate.sh` before the by-id build whenever the derivation names
an id. It also runs `command -v play_launch_parser`, so a missing parser fails
the step instead of skipping. Confirmation is still the next scheduled host job
reporting no FixtureNotBuilt.

---
id: 1758
title: "A missing prerequisite was a SKIP that read as a pass, at ~500 test sites and in the fixture builders; scope now decides what runs and everything in scope fails"
status: open
type: tech-debt
area: [testing, ci, build]
severity: high
found: 2026-10-09
related: [584, 1161, 1685, 1718, 1454, 650, 1802]
---

## The rule

A test never skips because something it needs is absent. It **fails**, naming
what is missing. What a run does not attempt is decided by its **scope**, which
is a property of the lane and is checked before any probe of the host:

- **Coordinates.** `NROS_TEST_COORDS` is read through
  `fixtures::lane::require_*_in_lane`.
- **Host capabilities the lane does not claim.** This is the new
  `NROS_TEST_UNCLAIMED` (`lane_scope::Capability`). Its first and only token is
  `ros2`, which means a stock ROS 2 install: the CLI, stock RMWs and ament
  message packages.

Both unset means the run claims everything, so a bare `cargo nextest` on a host
that lacks X is red.

The same rule applies to the fixture **build**. A prerequisite that is missing
for a row the invocation selected is a failure. The only way to build less is
to narrow the invocation.

## Measured: what read as a pass

1. **Tier 1 (run 37685900447).** 61 ROS 2 interop tests skipped as
   `capability` on the self-hosted runner, which has the zenoh router and no
   ROS 2 CLI. `check-skip-budget` caught them only because they were outside
   its allowlist. Before issue 1161 they were a pass.
2. **`.config/capability-skip-baseline.txt`.** This was an allowlist. Its two
   router lines let a lane without `rmw_zenohd` report green over every zenoh
   test. Since 54a6b5296 the gate image ships the router, and the runner image
   has shipped it since issue 1695, so nothing still needed those lines.
3. **`nros_tests::skip!` (~500 sites) and the `require_*() -> bool` helpers.**
   The helpers printed "Skipping test:" and left the caller to decide; 283
   callers did `skip!`.
   - Three sites did something else. `workspace_features_e2e` ran its
     advertised-profile check only `if require_ros2()`, so on every host
     without ROS 2 the cell passed with the check silently absent.
     `cyclonedds_descriptors` returned `None` and skipped. `exec_depend_drift_check`
     and `workspace_lints_check` discarded the bool.
   - `params.rs` and `params_per_node_interop.rs` turned a peer that never
     discovered our node into a `capability` skip. That is a real failure.
4. **`RequireFixture::require` on an UNGATED run.** `FixtureNotBuilt` became a
   skip. `NROS_FIXTURES_OPTIONAL=1` (the "light tier") did the same in a gated
   run.
5. **`compile-check-fixtures.sh`.**
   - `_note_lane_skip` exited 0 when cmake, `play_launch_parser`, a C++
     compiler or the PX4-Autopilot submodule was absent.
   - It checked those prerequisites in every pool unit, whether or not that
     unit's row needed them.
   - The PX4 block ran inside every unit: 87 redundant `px4_msgs` codegens per
     sweep.
   - A failed `cargo check` or a failed PX4 row was counted, and the script
     still exited 0. Issue 1718 (`cpp_robot_entry` never built on live-peer)
     was this.
6. **ThreadX Linux and ThreadX RV64 Cyclone passes.** A missing `idlc` or
   riscv64 gcc printed "skipped" and carried on, even in tier 2, whose
   coordinates claim `threadx-*,c,cyclonedds`.
7. **`west-fixtures.sh` and `ci::provision-zenohd`.** The first exited 0 with no
   west or `ZEPHYR_BASE`. The second fell off the end with exit 0 when apt could
   not install the router.
8. **`host-tests.yml`.** The `play_launch_parser` PATH export ended in `|| true`.

## What changed

- **The macros.**
  - `nros_tests::unmet!` replaces `skip!` and panics `[UNMET PRECONDITION]`,
    which no rewrite turns into a skip.
  - `nros_tests::lane_skip!` replaces `skip_class!(lane, …)` and is the only
    skip.
  - `skip_class!(capability|resource, …)` is gone.
  - All 18 `require_*()` helpers return `()` and fail by themselves. The ROS 2
    ones call `lane_scope::require_ros2_claimed()` first.
- **Scope.** `just ci tier1|tier2|tier2-nightly` export
  `NROS_TEST_UNCLAIMED=ros2`. Those ROS 2 tests belong to live-peer
  (`.config/interop-verdicts.toml`). Tests that probed a platform tool before
  the lane check now check the lane first: the FreeRTOS and Zephyr ROS 2
  interop cells, `orchestration_tiers_freertos` and `freertos_run_plan_runtime`.
- **`check-skip-budget`.** Any non-`lane` skip fails, with no allowlist, and the
  baseline file is deleted. A missing junit is a failure, not "nothing to
  check". `test` and `test-all` now pass the junit their run
  actually wrote.
- **Fixture resolver.** An ungated `FixtureNotBuilt` fails, and the
  `NROS_FIXTURES_OPTIONAL` arm is removed.
- **`compile-check-fixtures.sh`.**
  - Prerequisites are checked only when a selected row needs them, and are then
    fatal.
  - The `play_launch_parser` check is dropped; issue 1454 measured it unused.
  - PX4 rows are selectable ids and pool units.
  - Row build failures fail the invocation.
- **`lane-skip.sh`.** `nros_lane_named` treats a platform included by a
  coordinate-scoped lane as named. Its rows are claimed, so a missing
  prerequisite fails. Out-of-scope halves still go through
  `nros_lane_out_of_scope_note`. Only the unscoped `lane=all` sweep keeps
  SKIPPED (78), and its test run fails on the skipped modules
  (`_require-fixtures`).
- **The other sites.** ThreadX Cyclone passes ask `nros_lane_wants_rmw` before
  probing. `west-fixtures.sh`, `provision-zenohd` and `host-tests.yml` fail.
- **Docs.** CLAUDE.md, AGENTS.md, `docs/development/test-harness.md`, the audit
  checklist and the book page state the rule.

## Residue — what this makes RED that was green, on purpose

- **The tier-1 runner.** It still has the three real failures it had:
  - `~/.cache/nano-ros/models` is not writable.
  - The PX4-Autopilot submodule is not checked out. `px4_bridge_ffi` now fails
    at BUILD time instead of at the test.
  - The `native_async_action_client_awaits_goal_and_result` stall.

  The first two are runner provisioning.
- **ROS 2 tests no lane claims.** `workspace_shadowing` and
  `cyclone_slirp_pair` are not in `.config/interop-verdicts.toml`, so only
  tier 3 (`just ci full`) claims them. Before this they "passed" in tier 1 by
  skipping, so the coverage did not shrink; it is now visible.
- **`params*` discovery waits.** These now fail instead of skipping. If they
  flake under sweep load, the fix is the wait, not a skip.
- **Fixture-existence probes (done).** Three sites probed a real fixture with
  `is_file()` instead of `require_prebuilt_artifact(..).require(..)`:
  `freertos_run_plan_runtime` (`generated/`), `fvp_runtime_ws` and
  `fvp_smoke`. All three now go through the shared absence funnel, so they get
  the `.build-failed` marker and the lane attribution like every other
  resolver. `freertos_run_plan_runtime`'s other probe was for the Entry
  package's own source directory, which is tracked, so it is an `assert!`
  now. `check-fixture-require`'s existence ratchet dropped from 10 sites to 6;
  the six left are tree and tool checks (`examples/`, a PX4 checkout, the
  patched QEMU), not fixtures.
- **Explicit operator opt-outs stay.** These are operator choices, not silent
  ones, and no workflow sets any of them:
  - `NROS_SKIP_FIXTURE_CHECK`
  - `NROS_SKIP_STALE_CHECK`
  - `NROS_THREADX_RV64_CYCLONEDDS_FIXTURES=0`
- **Historical prose (done).** About 90 comments, recipe notes, workflow
  notes and two RFCs described the CURRENT behaviour as `skip!`. Each now says
  `unmet!` (a red) or `lane_skip!` (the scope skip), whichever the code calls.
  Comments that narrate what used to happen keep the old name, because that
  is what happened. The gate scripts' regexes keep matching `skip!` on
  purpose: they refuse the old spelling.

## Follow-up: the scope must reach every runner, and every link of the chain

### Reds since the merge (976ab0e0d, 2026-10-09 10:04 UTC)

- **Merge queue.** Eight batches were ejected, all with
  `error[E0433]: cannot find skip in nros_tests`. They came from PRs written
  against the old API: #1764 and #1805 (`violation_channel_e2e`), and #1808
  and #1823 (`native_api`). 2b8153621 fixed the first on main, the PRs
  rebased, and all four have merged.
- **Scheduled lanes.** None has run since the merge. Every red on the
  2026-10-09 nightly predates it.

### Gaps found by walking `just setup|doctor|build|test <scope>`

1. **`just test tier1` did not deselect what `just ci tier1` deselects.**
   `NROS_TEST_UNCLAIMED=ros2` was a literal in three `ci.just` recipes, and
   the scope verb never set it. The value is now a lane property:
   `CiLane::unclaimed` declares it, `nros_lane_unclaimed` in
   `fixture-lane.sh` implements it, and `lane_build_covers_run.rs` binds the
   two. Both runners read it from there.
2. **`just test <platform>` ran unscoped.** `just test native` selects 770
   tests, including `rtos_e2e`, `threadx_riscv64_qemu`,
   `zephyr_cortex_m_qemu` and `cli_bringup_nuttx`. Before this issue, an
   unbuilt fixture for another platform skipped. After it, such a fixture
   fails. A platform run is now narrowed to the rows that platform owns (`lane-coords
   --scope <module>`, `nros_scope_coords_file`), which is exactly what
   `just <module> build-fixtures` builds. A module that owns no fixture row
   (`px4`), or is not a platform (`xrce`, `cyclonedds`), stays unscoped and
   claims everything.
3. **`just doctor` said OK over a stale CLI.** It printed
   `[OK] nros CLI` while `just build` refused the same binary within a
   second. A missing CLI also printed `[MISSING]` and still exited 0. Both
   now fail, through `scripts/check-cli-fresh.sh`, the binary's own
   `source-stamp`. The same block now fails on a submodule BEHIND its pin
   (`scripts/check-submodule-drift.sh`). Measured: `just test native` died
   54 minutes in, on `nros-launch-resolve`'s `--locked` refusal, with
   `play_launch` 123 commits behind, while `doctor` had said OK.
4. **`just doctor zephyr` said OK without `jsonschema`.** `west build` needs
   it. The host block reported it only as a WARN shared by every scope. The
   Zephyr doctor now checks the interpreter the lane resolves
   (`nros_zephyr_python`) and fails.
5. **The light tier's opt-out was still read in four places.** These were
   `fixtures::lane::absent_row_breaks_promise`, `zenoh_archive_symbols`,
   `zenoh_header_parity` and `zpico_build_matrix`. All four now ignore it.

### Verified

- `lane-coords --scope native` selects the 10 `linux,*` coordinates.
- Under that scope, `rtos_e2e`, `threadx_riscv64_qemu`,
  `zephyr_cortex_m_qemu`, `cli_bringup_nuttx` and `emulator` gave 4 ran and
  54 deselected as `[SKIPPED:lane]`. None failed on an unmet precondition,
  and `check-skip-budget` passed.
- `lane_build_covers_run::shell_unclaimed_matches_the_rust_declaration` and
  the other 428 lane-contract tests pass.

### Round two: `just test native` against `just build native`

Running the chain for real found what the probes above could not.

6. **The native run and the native build disagreed about the scope.** `just
   test native` went to the module recipe. That recipe selects every
   nros-tests binary except a hand-kept group list, but its build (`just native
   build-fixtures`) builds only the example rows. 53 tests in scope failed:
   - compile-check, cmake and west fixtures that the module build never
     builds, with no coordinate to deselect them by;
   - the ros-editions docker harness.

   `native` now runs through the lane like its build does: `test-all` with
   `lane=native` plus the native coordinates. That brings the fixture
   preflight, the junit rewrite and the skip budget with it.
7. **`test-all` deselected suites by probing the host.** No `arm-none-eabi` or
   `riscv64-elf` dropped the embedded Cyclone tests. No `west` or Zephyr
   workspace dropped the west-fixture suites. No FVP binary, or no `espflash`,
   dropped theirs. Each of these vanished from the run without even counting
   as a skip. What the sweep does not claim is now declared, the same on every
   host:
   - `ros_editions` is opt-in.
   - FVP has no lane and no row.
   - ESP32 is dormant (issue 1525).

   Coordinates decide the rest. `require_west_fixture` now asks the Zephyr
   scope first. `check-fixtures-stale.sh` no longer drops a workspace row
   whose toolchain is absent, and `scripts/test/toolchain-gate.sh` is gone.
8. **`check-xrce-source-manifest` crashed on a host where `/nonexistent`
   exists.** It is `nobody`'s 0700 home on Debian and Ubuntu, so `exists()`
   raises `PermissionError`. The self-test now uses a temp dir.
9. **`just doctor` missed a stale `nros-launch-resolve`.** A `play_launch`
   move stales it too (issue 1487), and `nros sync` refuses it. `doctor` now
   fails on it.

Measured after the fixes:
- `just doctor native`: rc 0.
- `just build native`: rc 0 (62 min).
- `just test native`: 2,622 ran, 153 deselected, 0 unmet, 0 real failures
  (13 min).

### First scheduled red: live-peer host, the never-passing rosout C cell (2026-10-10)

`live-peer regression` run **38023549063** (schedule 04:17Z, head
`58d7ea22b`, which includes 976ab0e0d), job **114129481378** `rows whose board
IS this runner`, step `Run the cells with a recorded PASS`. Of the 25
recorded-PASS host cells, 25 produced a result. The red came from a cell
OUTSIDE that set:

```
FAIL [0.289s] nros-tests::rosout_interop a_c_log_call_reaches_ros2_topic_echo_rosout
[UNMET PRECONDITION] fixture not built: prebuilt rosout-talker-c: Test fixture binary not prebuilt:
  .../packages/testing/nros-tests/bins/rosout-talker-c/build-zenoh/rosout_talker_c
```

The lane runs `rosout_interop` because `native-logging-rust-zenoh-n2r` has a
recorded PASS (2026-09-29). Running that test file also runs its C and C++
cases, whose cells, `native-logging-c-zenoh-n2r` and
`native-logging-cpp-zenoh-n2r`, have NEVER passed. The fixture step built the
`linux cpp zenoh` family, which contains `rosout-talker-cpp`, so the C++ case
ran and passed. No `linux c zenoh` family was built, so `rosout-talker-c` was
absent.

**This falsifies a ruling in issue 1718.** Its resolution says
"`rosout-talker-c` needs nothing: its case belongs to
`native-logging-c-zenoh-n2r` … which has never passed, and the run already
reports it as a skip". That was true when an unmet fixture was a skip. Under
this issue's rule it is a failure, so the cell the lane does not claim now
reds the lane that never meant to run it.

Two sites can close it, and choosing between them is a scope decision:

- **Build what the run reaches.** The host fixture step builds `rosout-talker-c`.
  It already builds `linux cpp zenoh` for the C++ case, and the C case would
  then produce a real verdict for a cell nobody has recorded.
- **Run only what the lane claims.** The focused run filters to the test
  NAMES of recorded-PASS cells, not the whole test FILE. Then the C and C++
  cases do not run until someone records them.

Either one makes the next scheduled host job green. The second is the one that
matches "the lane's membership is exactly what a human has already measured",
which this lane's own summary prints.

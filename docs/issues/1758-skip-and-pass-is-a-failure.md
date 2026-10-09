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
- **Fixture-existence probes.** Three sites still probe a real fixture with
  `is_file()` instead of `require_prebuilt_artifact(..).require(..)`:
  `freertos_run_plan_runtime` (`generated/`), `fvp_runtime_ws` and
  `fvp_smoke`. They now fail rather than skip, and all three check their
  platform's lane first. `check-fixture-require`'s existence ratchet still
  holds them at their counts.
- **Explicit operator opt-outs stay.** These are operator choices, not silent
  ones, and no workflow sets any of them:
  - `NROS_SKIP_FIXTURE_CHECK`
  - `NROS_SKIP_STALE_CHECK`
  - `NROS_THREADX_RV64_CYCLONEDDS_FIXTURES=0`
- **Historical prose.** About 80 comments still say `skip!`.

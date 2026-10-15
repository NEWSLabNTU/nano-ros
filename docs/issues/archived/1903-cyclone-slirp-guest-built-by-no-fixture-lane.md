---
id: 1903
title: "`just test linux` claims `cyclone_slirp_pair`, but no fixture lane builds its guest (`nros_rmw_cyclonedds_ros2_pub`), so the test fails as a broken fixture promise"
status: resolved
resolved: 2026-10-15
type: bug
area: [testing, ci]
severity: low
found: 2026-10-15
related: [1800, 1656, 0828, 0482, 1758, 1251]
---

## Measured

Tree `e8400d5a92`, this host has ROS 2 Humble with `rmw_cyclonedds_cpp`,
`unshare`, `slirp4netns`, unprivileged user namespaces and a default route.

1. `just build-test-fixtures lane=linux`: rc=0. The guest was not built:

   ```
   $ ls packages/rmw/cyclonedds/nros-rmw-cyclonedds/build/tests/nros_rmw_cyclonedds_ros2_pub
   ls: cannot access '…/nros_rmw_cyclonedds_ros2_pub': No such file or directory
   ```

2. The test, run with the scope `just test linux` exports
   (`NROS_FIXTURE_LANE=linux`, `NROS_TEST_COORDS=<nros_scope_coords_file linux>`,
   10 `linux,*` rows, `NROS_TEST_UNCLAIMED=` empty):

   ```
   $ cargo nextest run -p nros-tests --test cyclone_slirp_pair
   FAIL [1.126s] nros-tests::cyclone_slirp_pair a_tag_isolated_host_profile_reaches_a_slirp_guest_and_nothing_else
   panicked at packages/testing/nros-tests/src/fixtures/binaries/mod.rs:756:9:
   Test fixture binary MISSING for an in-lane coordinate: …/nros-rmw-cyclonedds/build/tests/nros_rmw_cyclonedds_ros2_pub
   A gated run already asserted this lane's fixtures are built and fresh, so this is a broken promise …
   Build it:   just build-test-fixtures
   ```

   Every host precondition passed (`preconditions()` runs first). The test
   died on the artifact.

## Why

- The test resolves its guest through
  `require_prebuilt_artifact(path, "just cyclonedds build-rmw")`
  (`tests/cyclone_slirp_pair.rs:58`).
- `require_prebuilt_artifact` asks `fixtures::lane::require_in_lane` first.
  That attributes a path to a manifest row. This path matches no
  `examples/fixtures.toml` row, so it is never out of lane, and it can never be
  skipped by scope. That is correct: an unattributable path fails closed.
- No fixture lane produces it. `git grep 'build-rmw'` over `justfile` and
  `just/` finds the recipe and its callers in `just/cyclonedds.just`
  (`build`, `test`, `ci`), plus `just cyclonedds ci` in `check rmw-cyclonedds`
  (`just/check/lanes.just:3282`, the `check-build` lane). There is no
  fixture-build caller, and the measured `lane=linux` build above left it
  absent.
- So the `linux` scope claims the test (it claims ROS 2 too) and its build
  cannot satisfy it. This is the 0828 / 0482 shape: the run set is bigger than
  the build set. `lane=all` has the same gap. That is not measured, but it follows from
  the grep above. `just ci full` would
  pass only because `check` (with `check rmw-cyclonedds`) runs before
  `test-all`. That is order, not a promise.
- The doc on `require_prebuilt_artifact` already says this is the wrong
  funnel: "NOT for an artifact only a `ci gate`-reachable recipe produces …
  Give such an artifact a `[[compile_check_fixture]]` row instead (issue 1656
  did, for `borrowed_e2e`)".
- Second defect, measured in the panic above: the GATED message says
  `Build it: just build-test-fixtures`. The site's `remedy` (`just cyclonedds
  build-rmw`) is used only on the ungated `Err` path. So the message names a
  command that does not build this artifact.

Issue 1800 covers the other half: no scheduled lane runs this test, because
the runner container refuses user namespaces. This issue is about the local
`just test linux` run, which does claim it.

## Not measured

- `just test linux` end to end. The single test was run with the same three
  variables `_test-scope linux` exports.
- Whether other `require_prebuilt_artifact` call sites name an artifact that
  no fixture lane builds.

## Fix direction

1. Give the guest a build-stage row: a `[[compile_check_fixture]]` with builder
   `fixture-script` that runs `just cyclonedds build-rmw` (or its cmake target
   for `nros_rmw_cyclonedds_ros2_pub`), with the `linux` coordinate. Then
   resolve it with `require_compile_check`, as 1656 did for `borrowed_e2e`.
   The linux build then produces it and the run's claim holds.
2. Make the gated absence message print the site's `remedy` beside the generic
   line.

Gate for the class: every `require_prebuilt_artifact` call site names a path
that some lane's build produces. A static check can harvest the literal paths
and remedies and refuse any remedy that is not a fixture-build verb. Stronger:
fold these paths into `lane_build_covers_run`, which today checks only manifest
rows and so cannot see an unattributable artifact.

## Resolution (2026-10-15)

Both halves of the fix direction, plus the class gate.

1. **The guest has a row.** `cyclone_slirp_guest` is a
   `[[compile_check_fixture]]` with builder `fixture-script`
   (`packages/testing/nros-tests/fixtures/cyclone-slirp-guest/build.sh`). It
   configures the backend in the row's own CMake tree with the flags
   `just cyclonedds build-rmw` passes, builds only
   `nros_rmw_cyclonedds_ros2_pub`, copies it into the row dir, and writes
   ninja's recorded compile inputs as dep-info, so `.inputsig` covers the
   backend and Cyclone sources it read (25,552 recorded inputs). Every lane
   builds it through `build-compile-check-fixtures`. The cold build took about
   22 s on this host, and later runs reuse it. The test resolves it with
   `require_compile_check_bin`, and it checks the `linux` coordinate first, as
   `borrowed_e2e` does (issue 1685's ordering).
2. **The gated message names the site's remedy.** `Build it:` prints the
   `remedy` the call passed. The row-keyed resolvers already pass
   `just build-test-fixtures`, so their message is unchanged. The unit test
   `gated_absence_names_the_sites_remedy` pins this.
3. **Gate: `just check prebuilt-artifact-producers`** (fast lane). It harvests
   every `require_prebuilt_artifact` call. It follows helpers that forward
   their `remedy` parameter (`ros_env::require_edition_example`). It maps each
   site to the test binaries that reach it, and exempts only binaries that
   `test-all` declares out of scope (`not binary(..)`: the ros_editions docker
   harness and the FVP). Every other site's remedy must be a recipe in the
   closure of `build-test-fixtures`, read from `just --dump`. The row half
   requires every id passed to `require_compile_check{,_bin}` or
   `require_compile_verdict` to be a row that `compile-check-fixtures.sh`
   builds.

Sweep result: 8 sites after the fix. 2 are lane-built
(`freertos::build-examples`, `freertos::build-fixture-extras`; both are
lane-scoped). 6 are opt-in (2 FVP, 4 ros_editions). `cyclone_slirp_pair` was
the only violation. That answers the "other call sites" question under
"Not measured" above.

Measured after the fix: `just build-test-fixtures lane=linux` rc=0. With the
row dir deleted, `just build-compile-check-fixtures` (the line the lane runs)
rebuilt the binary. The test passed SOLO under the scope `just test linux`
exports (`NROS_FIXTURE_LANE=linux`, 10-row `NROS_TEST_COORDS`), in 36.6 s.

Still open: issue 1800 (no scheduled lane runs this test). The gate checks
that a producer exists in SOME lane. Per-lane scoping of the two lane-built
FreeRTOS sites is checked by attribution at run time (a manifest row, or
`require_platform_in_lane`), not statically.

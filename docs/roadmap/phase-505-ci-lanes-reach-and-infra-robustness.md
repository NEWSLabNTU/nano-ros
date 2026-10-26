# Phase 505 — every gate runs somewhere, and a red names the code

**Status (2026-10-27). #2052 is resolved: `8b95e90dd6` had already fixed it
when it was homed here, without citing it.**

**Status (2026-10-26). Three issues with no active phase were homed here on
2026-10-26: #2066 and #2067 under W1, and #2052 under W2. W2 also has #2069,
filed the same day: the out-of-lane gate, a merge-group step, reports a
host-only test's timeout as a scope violation. #2082 (a `ci gate` unit test
raced a global counter under load), filed the same day, is resolved.
Everything else below is unchanged.**

**Status (2026-10-21). Every work item is done: W1 by PRs #2394, #2429 and
#2430, W2 by #2426 and #2428 plus the ruleset edit that closed #1629, and W3 by
#2427.** What remains is the acceptance window: two weeks of merge-queue
ejections with no INFRA ejection from a W2 cause, then this phase is archived.
The 2026-10-18 notes follow. Every issue in the first table was open with no active
phase when the open-issue survey of 2026-10-17 ran.

- **Three W1 issues were already fixed by earlier PRs, never closed:**
  #1380 (PR #2290), #1795 (PR #1938) and #1784 (PR #1864, as issue 1779).
  Each was re-measured on 2026-10-18 before it was archived.
- **#1309 is fixed here.** The feature sweep's `STANDALONE` half lints the
  excluded roots, and its first runs found 50 latent `-D warnings` errors in
  five crates.
- **#1960 was filed while closing #1309, and is now fixed.** The sweep lints
  all 33 excluded standalone roots (31 runs, 2 reasoned skips), which found
  20 clippy errors in 8 roots and rustfmt diffs in 5. It also filed #1966.
- **#1800 is resolved:** live-peer's host job runs both of its tests.

## Why this phase exists

A CI result is useful only when it answers one question: is the code under test
broken? Two kinds of open issue stop it from answering:

- **Reach.** A gate or test that no lane runs is never asked, so its red is
  found by whoever runs it by hand, days later. Issue 1177 (homed in
  [phase-489](phase-489-ci-build-speedup.md)) is the precedent. The issues below
  are the same class for an excluded crate, a clippy feature combination, a
  carrier link route and two ROS 2 tests.
- **Infrastructure noise.** A red caused by the network, the host or the CI
  machinery reads exactly like a red caused by the code. In the merge queue it
  ejects a whole batch (`grouping_strategy` is ALLGREEN). A hang or an orphaned
  fan-out reads as success or as nothing.

Scheduled lanes that produce no verdict at all are
[phase-466](phase-466-scheduled-lanes-produce-verdicts.md)'s subject, not this
phase's. Build speed is phase-489's.

## Work items

### W1 — reach: every gate and test is run by a named lane

- [x] [#1309](../issues/archived/1309-excluded-crate-is-built-by-no-lane.md): a
      root-workspace `exclude` entry names the lane that builds and tests the
      crate, or the crate becomes a member.
- [x] [#1380](../issues/archived/1380-zenoh-backend-clippy-red-for-platform-bare-metal.md):
      `nros-rmw-zenoh` is clippy-clean under `platform-bare-metal`, and a lane
      runs that combination.
- [x] [#1784](../issues/archived/1784-workspace-features-threadx-posix-duplicate-platform.md):
      `check::workspace-features` stops linking two platform ABIs into one test
      binary.
- [x] [#1795](../issues/archived/1795-rtos-carrier-route-no-fixture.md): a fixture row
      builds a ThreadX or NuttX TYPED carrier image.
- [x] [#1800](../issues/archived/1800-ros2-tests-no-scheduled-lane-claims.md): a
      scheduled lane that claims ROS 2 runs `workspace_shadowing` and
      `cyclone_slirp_pair`. Both run in live-peer's host job, which is a
      GitHub-hosted VM: `workspace_shadowing` since PR #1942, and
      `cyclone_slirp_pair` since its container gained `seccomp=unconfined` and
      `/dev/net/tun`.
- [x] [#1960](../issues/archived/1960-standalone-cargo-roots-linted-by-no-lane.md): the
      28 excluded roots that are built but linted by nothing each get a
      `STANDALONE` sweep run, or a reason.
- [ ] [#2066](../issues/2066-linux-sched-deadline-throttle-unverified.md): the
      Linux `SCHED_DEADLINE` throttle half runs where it can (a job with
      `CAP_SYS_NICE`, or a recorded manual run), or RFC-0108 states it is
      unverified.
- [x] [#2067](../issues/archived/2067-freertos-level-kernel-slicing-off-not-booted.md): a
      FreeRTOS QEMU level row, C and Rust, asserts `nros: kernel time slicing
      OFF`; a non-level row asserts it absent or ON.

### W2 — the merge gate fails only for the code

- [x] [#1626](../issues/archived/1626-api-parity-fast-line-needs-crates-io.md):
      `check-api-parity` survives a crates.io hiccup.
- [x] [#1868](../issues/archived/1868-ci-rustup-downloads-flake-merge-gating-jobs.md):
      merge-gating jobs stop downloading toolchains at run time.
- [x] [#1760](../issues/archived/1760-ci-just-installer-403.md): installing `just` is
      pinned and retried.
- [x] [#1629](../issues/archived/1629-queue-check-timeout-outgrown-by-gate-lane.md): the
      queue's check timeout and the gate lane's duration are kept in agreement.
- [x] [#1912](../issues/archived/1912-push-gate-pin-baseline-is-live-main.md): the
      push-event pin gate compares against the pushed commit's parent, not the
      live `origin/main`.
- [x] [#1745](../issues/archived/1745-probe-gates-race-mbedtls-submodule-fetch.md):
      parallel probe gates provision a submodule once.
- [x] [#1621](../issues/archived/1621-sdk-store-submodule-provision-breaks-worktree-git.md):
      a failed submodule provision leaves the worktree's git usable.
- [ ] [#2069](../issues/2069-out-of-lane-gate-reports-a-timeout-as-a-scope-violation.md):
      `check-out-of-lane-deselected` reports a host-only test that timed out
      under load as a scope-order violation, and names the scope remedy.
- [x] [#2052](../issues/archived/2052-pristine-ament-surface-stale-home-cli.md):
      `check::pristine-checkout`'s `cmake-ament-surface` uses the checkout's
      own CLI, or reports NOT VERIFIED, and never reaches a stale
      `~/.nros/bin/nros`. Resolved by `8b95e90dd6`, which predates the
      homing.
- [x] [#2082](../issues/archived/2082-dropped-take-log-test-races-the-global-counter.md):
      `dropped_take_log_tests` reset the process-global `DROPPED_TAKES` that
      other tests in the binary advance, so `node-std-tests` goes red under
      parallel load. Resolved: the tests call the logger with a count of
      their own and find their line by buffer size.

### W3 — a stopped or failed lane says so, with the cause

- [x] [#1350](../issues/archived/1350-killed-lane-orphans-parallel-gate-children.md):
      killing a lane kills its gate fan-out and exits non-zero.
- [x] [#1403](../issues/archived/1403-fixture-jobserver-deadlock-looks-like-a-hang.md):
      the jobserver deadlock is fixed, or it is detected as a failure.
- [x] [#1785](../issues/archived/1785-stall-watchdog-diagnostics-hang-on-fuse-fd.md):
      the stall watchdog's diagnostics cannot hang.
- [x] [#1625](../issues/archived/1625-dep-chain-nuttx-cell-one-run-red.md): a
      `dep-chain` FAIL prints cargo's error.

## Acceptance

Every issue below is resolved or re-homed with a reason. For W2: two weeks of
merge-queue ejections, classified by `just queue-triage`, with no INFRA
ejection from a cause listed here.

## Issues homed here (2026-10-17)

| issue | why it belongs here |
| --- | --- |
| [#1309](../issues/archived/1309-excluded-crate-is-built-by-no-lane.md) | A root-workspace `exclude` entry takes eight crates out of every lane, so their unit tests never run. It is a gap in what the lanes reach. |
| [#1350](../issues/archived/1350-killed-lane-orphans-parallel-gate-children.md) | Killing a `ci gate` lane leaves its `-P 32` gate fan-out running as orphans, and the killed lane reads as success. |
| [#1380](../issues/archived/1380-zenoh-backend-clippy-red-for-platform-bare-metal.md) | `nros-rmw-zenoh` fails `clippy -D warnings` under `platform-bare-metal`, a feature combination no lane runs clippy on. |
| [#1403](../issues/archived/1403-fixture-jobserver-deadlock-looks-like-a-hang.md) | `build-examples` deadlocked on its own make jobserver FIFO. In CI that reads as a hang, not a failure. |
| [#1621](../issues/archived/1621-sdk-store-submodule-provision-breaks-worktree-git.md) | A fast-line probe gate in a worktree without zenoh-pico leaves a broken gitlink, and after that `git status` fails for the whole worktree. |
| [#1625](../issues/archived/1625-dep-chain-nuttx-cell-one-run-red.md) | A `dep-chain` FAIL printed `thiserror` tree rows instead of cargo's error, so the cause of that red cannot be recovered. |
| [#1626](../issues/archived/1626-api-parity-fast-line-needs-crates-io.md) | `check-api-parity` runs `cargo doc`, so the one required check goes red on a crates.io hiccup, with no retry. |
| [#1629](../issues/archived/1629-queue-check-timeout-outgrown-by-gate-lane.md) | The merge queue's 60-minute check timeout is shorter than the gate lane, so entries are ejected while their gate goes on to pass. |
| [#1745](../issues/archived/1745-probe-gates-race-mbedtls-submodule-fetch.md) | Two parallel probe gates provision the mbedtls submodule at the same time, and one of them dies on git's shallow-file race. |
| [#1760](../issues/archived/1760-ci-just-installer-403.md) | CI installs `just` by piping an installer script with no retry, and one 403 failed a lane. |
| [#1784](../issues/archived/1784-workspace-features-threadx-posix-duplicate-platform.md) | `check::workspace-features` links two platform ABIs into one test binary. The gate runs only in `ci gate` (issue 1177's class). |
| [#1785](../issues/archived/1785-stall-watchdog-diagnostics-hang-on-fuse-fd.md) | The make-stall watchdog hangs in its own diagnostics on a stuck FUSE mount, so the watchdog never fires. |
| [#1795](../issues/archived/1795-rtos-carrier-route-no-fixture.md) | No lane builds an RTOS TYPED carrier image, so the link of `nano_ros_node_register`'s ThreadX and NuttX arms has no coverage. |
| [#1800](../issues/archived/1800-ros2-tests-no-scheduled-lane-claims.md) | `workspace_shadowing` and `cyclone_slirp_pair` claim ROS 2 and are not interop cells, so no scheduled lane runs them. |
| [#1868](../issues/archived/1868-ci-rustup-downloads-flake-merge-gating-jobs.md) | Merge-gating jobs download Rust toolchains at run time, and a failed download ejects the whole batch. |
| [#1960](../issues/archived/1960-standalone-cargo-roots-linted-by-no-lane.md) | Filed 2026-10-18 while closing #1309: 28 excluded cargo roots are built, as a fixture or a locked root, and clippy reaches none of them. |
| [#1912](../issues/archived/1912-push-gate-pin-baseline-is-live-main.md) | The push-event submodule-pin gate compares against the live `origin/main`, so a superseded commit reads as a REWIND. |
| [#1964](../issues/1964-pre-push-fast-red-unreproduced.md) | The pre-push `check fast` went red once and green on an immediate retry, and which gate it was cannot be recovered. The hook now keeps its full output outside the checkout. |
| [#1984](../issues/archived/1984-run-matrix-coordinate-truth-nm-sweep-timeout.md) | The `nm` audit of every workspace row timed out on a slow runner with no row named. Each pass and the sweep are now bounded and timed. |
| [#1985](../issues/archived/1985-identity-budget-counts-retired-cargo-root.md) | `artifact-identity-budget` counted a cargo root the image no longer builds, so the persistent fixture tree failed it after phase-506 W4a. |
| [#2069](../issues/2069-out-of-lane-gate-reports-a-timeout-as-a-scope-violation.md) | Filed 2026-10-26. A merge-group gate step turns a load timeout into a scope-order verdict, so a batch is ejected for a cause that is not in the code and the ejection names the wrong fix. |
| [#2052](../issues/archived/2052-pristine-ament-surface-stale-home-cli.md) | Resolved 2026-10-26 by `8b95e90dd6`, which landed before it was homed. `ci gate`'s pristine-checkout step reaches a stale `~/.nros/bin/nros` through `cmake-ament-surface`, so the gate's verdict depends on the host rather than the code. |
| [#2066](../issues/2066-linux-sched-deadline-throttle-unverified.md) | Homed 2026-10-26 from archived phase-509. The `SCHED_DEADLINE` throttle needs `CAP_SYS_NICE`, which no lane has, so that half of the level budget is run by nothing. |
| [#2067](../issues/archived/2067-freertos-level-kernel-slicing-off-not-booted.md) | Homed 2026-10-26 from archived phase-509. No lane boots a FreeRTOS level image and reads its time-slicing line, so the kernel setting is built and never checked. Resolved 2026-10-27: `realtime_tiers_e2e` asserts the line OFF on the C, C++ and Rust level rows, archived. |
| [#2082](../issues/archived/2082-dropped-take-log-test-races-the-global-counter.md) | Resolved 2026-10-26. A `ci gate` unit test asserted on a global counter other tests share, so the queue could eject a batch for load, not code. |

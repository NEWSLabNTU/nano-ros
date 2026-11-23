# Phase 590 -- the next release past nros11 (0.6.0-nros12): plan

**Status (2026-11-23). OPEN: plan approved by the user (blockers first); W1 live, tracked below. The date stays tentative and the user confirms before the cut and before publish. No tag, no publish, no
dispatch with `publish=true` until the user names the day.** Tracks issue 2882 (filed in the
phase-478 batch as item I42 of
[phase-478](phase-478-island-developer-experience-findings.md)); release rules are
[phase-567](archived/phase-567-regression-prevention.md) W1-W3, as AGENTS.md
"Release branches" states them.

## Why

The latest release is `nros-0.6.0-nros11` (`09219edc75`, 2026-11-16). `main` is 1,067 commits
past it (`b61082063c`, 2026-11-23). A consumer on the released CLI, such as the safety island,
whose default branch pins `0.6.0-nros11`, cannot take any fix made since. The island's
`nano-ros-next` work and the commit pin (issue 2645, phase-478 I26) both wait for a release.
An nros12 candidate was already rehearsed: dry run 44170501730 on 2026-11-17, at `2ee0286144`,
built `0.6.0-nros12` and passed its installed probe. A red `host-tests` run on 2026-11-18 then
blocked it, which is what made `host-tests` and `live-peer` release lanes (phase-567 W1,
amended 2026-11-18).

## What is in it since nros11

284 changelog fragments were added to `changelog.d/` between the nros11 tag and `main`
(`comm -13` of the two trees' `changelog.d/` listings): 160 fix, 72 feat, 21 breaking,
11 changed, 16 docs, 4 perf. The rendered notes come from `just changelog` (a towncrier draft).
`book/src/migrating/releases/0.6.0-nros12.md` already holds the migration page, written at the
first candidate, and it needs the breaking changes below added before the cut.

The 21 breaking changes, grouped by who has to act:

- **Configuration a user writes.**
  - 41 built-in settings are renamed (one is removed) so that every key is `<owner>.<name>` and
    every size is in bytes. The old names are refused, with the new name in the message.
  - `nros build` refuses a nano-ros knob passed as a CMake `-D`, in any spelling.
  - The Zephyr options `CONFIG_NROS_SNTP_EPOCH` and `CONFIG_NROS_EPOCH_FROM_ZENOH` are retired:
    the wall-clock source is stated in `system.toml`.
- **Contracts.**
  - The old ros-launch-manifest spellings are refused, each naming its replacement
    (`play_launch migrate <contract> --write` rewrites them).
  - QoS values and launch arguments are checked.
- **Parameters.**
  - A string or array parameter needs a stated capacity, whether it is declared in the
    contract or in code.
  - `string_array` has its own element bound.
  - A parameter descriptor no longer copies the name.
  - An image with no stated parameter count gets 8 store slots, not 32.
  - C++ `declare_parameter<T>` stops the boot on a type mismatch.
- **Images.**
  - Every image serves the stock ROS 2 node interfaces (the six parameter services, `/rosout`)
    by default.
  - Each C/C++ workspace image has its own CMake configure: its build dir names the image
    (issue 2702).
  - A registered component's program is named for its `EXECUTABLE`.
  - A `linux` image uses the `domain_id` and `locator` its `system.toml` states.
  - Rust images derive `spin_period` as C/C++ do.
  - The ThreadX byte pool is derived, no longer a fixed 4 MiB.
- **Ports and ABI.**
  - A custom platform port must define `nros_platform_wall_clock_state()`.
  - The RMW backend C ABI enums are `int`-sized.
  - A CAN FD frame on a non-FD handle returns `NROS_PLATFORM_CAN_ENOFD`.
- **Provisioning.** The Arm FVP is no longer provisioned or distributed (`[tool.arm-fvp]` is
  gone).

The codegen version matters most to a consumer: a project generated with nros11 needs no
regeneration only if `NROS_CODEGEN_VERSION` did not move. Check it at the cut by running
`nros toolchain manifest` against the candidate asset, and say so on the migration page.

## Blockers

A release cuts only from a commit that all five scheduled lanes passed: `nightly.yml`,
`run-matrix.yml` tier 1 and tier 2, `host-tests.yml` and `live-peer.yml`. A docs-only
descendant of such a commit also counts (`scripts/release/release-lanes-green.py`). Run against
`main` today, it refuses all five. The newest scheduled runs, 2026-11-22/23:

| lane (cron, UTC) | last green | failing now |
| --- | --- | --- |
| `host-tests` (03:00) | 2026-11-20 `659ee14b07` | **book probe, installed track**: step 27 of `book/src/start/install.md` runs `nros completions` with a `$SHELL` it cannot name (`$SHELL names no shell nros completes in`); **feature-sweep**: `nros-node [alloc,rmw-lending]` fails clippy `-D warnings`; **rt-sched levels**: `linux_rust_thread_census_is_placed_by_the_priority_plan`, after `levels-cpp`'s Zephyr configure refused (`the metadata probe generated no bound table for builtin_interfaces`) |
| `live-peer` (04:00) | 2026-11-20 `0a3e2755f3` | `pubsub_zephyr_cortex_m_ros2_interop_e2e` (`nros_zephyr_cortex_m_publisher_reaches_ros2_topic_echo`), and the fixture build for the runner-board rows |
| `nightly` (05:00) | 2026-11-20 `b28c0fbaca` | **island-shaped-zephyr**: the merge-queue snapshot is stale (`NROS_DERIVED_MAX_SUBSCRIBERS` 3 vs a live 4; regenerate with `just check island-shaped-zephyr-regen`); **stack-floor drift**: 1 of 7 rows failed (issue 2795 is the open report-path item); **ThreadxLinux e2e**: `test_rtos_session_open_outlasts_a_late_router` |
| `run-matrix` tier 1 + tier 2 (06:00) | none since nros11: red every night from 2026-11-16 | **tier 1 and tier 2-nightly builds**: the `linux` fixture stage stops at `features/src/demo_bringup`, `the parameter store cannot be sized: the entity inventory REFUSED`. This is likely a consequence of the parameter-capacity breaking changes above, with a feature-demo image not stating a capacity. **tier 2**: 6 real failures, including `zephyr_connect_budget` (3 cases), `zephyr_rust_zenoh_action_goal_completion_rate` and `zephyr_nucleo_renode_c_zenoh_serial_pubsub_e2e` (possibly issue 2815) |

`run-matrix` is the critical path: no run of it has been green since the last release.

### Blocker tracker

This table is the W1 tracker. Each row gets its owner and pull request as they are routed, and
its state as it lands. The state is closed only when a scheduled run of its lane is green after
the fix.

| # | lane | blocker | issue | owner | PR | state |
| --- | --- | --- | --- | --- | --- | --- |
| B1 | tier 1, tier 2-nightly | linux fixture stage: `features/demo_bringup` parameter store refused | [2885](../issues/2885-linux-features-demo-param-store-refused.md) | to route | -- | open |
| B2 | tier 2 | Zephyr connect-budget (3) and Rust action goal-completion rate | [2891](../issues/2891-tier2-zephyr-connect-budget-and-action-rate.md) | to route | -- | open |
| B3 | tier 2 | Renode twin: the nucleo C talker faults in logging | [2815](../issues/2815-zephyr-nucleo-c-talker-faults-in-logging.md) | to route | -- | open |
| B4 | nightly | island-shaped Zephyr snapshot stale | [2886](../issues/2886-island-shaped-zephyr-snapshot-stale.md) | to route | -- | open |
| B5 | nightly | stack-floor row: Rust action server on nucleo-f767zi-zephyr overflows RAM by 77,440 B | [2887](../issues/2887-stack-floor-nucleo-action-server-rust-ram-overflow.md) | to route | -- | open |
| B6 | nightly | ThreadX-Linux `session_open_outlasts_a_late_router` | [2888](../issues/2888-threadx-linux-late-router-session-open.md) | to route | -- | open |
| B7 | host-tests | installed-track probe: `nros completions` with an unnamed `$SHELL` | [2892](../issues/2892-installed-probe-completions-shell.md) | 139 AUO book | -- | open |
| B8 | host-tests | feature-sweep: `nros-node [alloc,rmw-lending]` clippy | (PR only) | 241 | #4679 | in review |
| B9 | host-tests | rt-sched: `levels-cpp` Zephyr bound table missing; census test | [2890](../issues/2890-levels-cpp-zephyr-bound-table-probe.md) | to route | -- | open |
| B10 | live-peer | Zephyr Cortex-M publisher to `ros2 topic echo`; runner-board fixture build | [2889](../issues/2889-live-peer-zephyr-cortex-m-ros2-echo.md) | to route | -- | open |

The 118 session owns the `host-tests` / `live-peer` lane wiring itself.

The release workflow path itself is not a blocker. W3 of phase-567 rehearses it end to end
(the island-shaped workspace job, both starter images, `verify-starter-image.sh` and the
installed-track probe against the candidate asset), and the 2026-11-17 dry run built
`0.6.0-nros12` and passed `PROBE PASS`. The red installed-track probe above is the same probe
on `main`, so W1 has to fix it before the rehearsal can pass again.

## Release steps

### W1 -- every lane green on main

- **Status:** OPEN.

Fix each red above on `main`, as its own pull request: file an issue, fix the class, and
re-measure on the next scheduled run. Done when each lane has one green scheduled run on a
commit at or after its fix. They do not yet have to be the same commit.

### W2 -- one commit all five lanes passed (a quiet night)

- **Status:** OPEN.

The four crons start at 03:00, 04:00, 05:00 and 06:00 UTC, each on whatever `main` is then.
So a commit passes all five only if no code lands between about 02:50 and 06:20 UTC. Docs-only
merges are fine. On the chosen night, the merge queue lands docs only in that window, or is
paused. Then `release-lanes-green.py --shas-from <that sha>` (run read-only, as this plan did)
must print no `REFUSED` line once the 06:00 `run-matrix` finishes, about 10:30 UTC.

### W3 -- cut and rehearse

- **Status:** OPEN.

1. Create `release/nros12` at the W2 commit. The name may be re-cut from `main` while no
   `nros-0.6.0-nros12` tag exists.
2. Update `book/src/migrating/releases/0.6.0-nros12.md` with the breaking changes above and
   the codegen answer. A docs-only commit on `main`, cherry-picked with `-x`, keeps W1 of
   phase-567 satisfied.
3. `gh workflow run release-nros.yml --ref release/nros12 -f version=0.6.0-nros12`,
   `publish=false`. The lanes job must pass W1 and W2; the build, island and starter-image jobs
   must pass, including the installed-track probe against the candidate asset.

### W4 -- publish (the user's step)

- **Status:** OPEN, the user dispatches it.

Re-dispatch the same ref with `-f publish=true`. This creates the `nros-0.6.0-nros12` tag, the
GitHub release with its asset, `.sha256`, SBOM and manifest, and pushes the starter images.
After that the branch is frozen (phase-567 W2): a later fix reaches it only as a
`git cherry-pick -x` of a `main` commit, under a new version name.

### W5 -- consumers move

- **Status:** OPEN.

Tell the island it can move: `nros-toolchain.toml`, `modules/nros` and `west.yml` move
together to `0.6.0-nros12`, or to a commit pin (I26), and `nano-ros-next` folds into its
default branch. Close issue 2882 once a release past nros11 is published, which is its
acceptance.

## Recommended date window

**2026-11-26 to 2026-11-28 (UTC): cut on the first of these mornings where W2 holds.** The
reasoning:

- W1 is about seven independent fixes, none of them large on the evidence above. Most are a
  refusal the tree now makes correctly, against an image or snapshot that did not follow it
  (the demo param capacity, the island snapshot, the bound table). Two to three days covers
  them with the merge queue's usual turnaround.
- W2 needs one quiet night after the last fix. The night of 2026-11-25/26 is the earliest
  realistic one; 26-28 leaves two retries for a lane that flakes.
- The rehearsal (W3) and publish (W4) fit inside the same day, after `run-matrix` finishes at
  about 10:30 UTC.

If W1 is not done by 2026-11-27, the window slides day for day. The plan holds no date the user
has not picked.

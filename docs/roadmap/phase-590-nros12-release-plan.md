# Phase 590 -- the next release past nros11 (0.6.0-nros12): plan

**Status (2026-11-23). OPEN: plan approved by the user (blockers first); W1 live, tracked below. The date stays tentative and the user confirms before the cut and before publish. No tag, no publish, no
dispatch with `publish=true` until the user names the day.** Tracks [issue 2882](../issues/2882-nros12-release-for-consumers.md) (filed in the
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
    (`nros configure --migrate` rewrites them;
    [issue 2923](../issues/archived/2923-nros11-contract-spelling-refused-no-shipped-migrate.md)).
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
descendant of such a commit also counts (`scripts/release/release-lanes-green.py`). Since the
user's ruling of 2026-11-24, a green `workflow_dispatch` of a lane counts as well, but only on
`main`, at the release commit itself (not a docs-only ancestor), with every job of the run green.
A dispatch on a branch never counts. Run against
`main` today, it refuses all five. The newest scheduled runs, 2026-11-22/23:

| lane (cron, UTC) | last green | failing now |
| --- | --- | --- |
| `host-tests` (03:00) | 2026-11-20 `659ee14b07` | **book probe, installed track**: step 27 of `book/src/start/install.md` runs `nros completions` with a `$SHELL` it cannot name (`$SHELL names no shell nros completes in`); **feature-sweep**: `nros-node [alloc,rmw-lending]` fails clippy `-D warnings`; **rt-sched levels**: `linux_rust_thread_census_is_placed_by_the_priority_plan`, after `levels-cpp`'s Zephyr configure refused (`the metadata probe generated no bound table for builtin_interfaces`) |
| `live-peer` (04:00) | 2026-11-20 `0a3e2755f3` | `pubsub_zephyr_cortex_m_ros2_interop_e2e` (`nros_zephyr_cortex_m_publisher_reaches_ros2_topic_echo`), and the fixture build for the runner-board rows |
| `nightly` (05:00) | 2026-11-20 `b28c0fbaca` | **island-shaped-zephyr**: the merge-queue snapshot is stale (`NROS_DERIVED_MAX_SUBSCRIBERS` 3 vs a live 4; regenerate with `just check island-shaped-zephyr-regen`); **stack-floor drift**: 1 of 7 rows failed (issue 2795 is the open report-path item); **ThreadxLinux e2e**: `test_rtos_session_open_outlasts_a_late_router` |
| `run-matrix` tier 1 + tier 2 (06:00) | none since nros11: red every night from 2026-11-16 | **tier 1 and tier 2-nightly builds**: the `linux` fixture stage stops at `features/src/demo_bringup`, `the parameter store cannot be sized: the entity inventory REFUSED`. This is likely a consequence of the parameter-capacity breaking changes above, with a feature-demo image not stating a capacity. **tier 2**: 6 real failures (its `junit-real.xml`; a first reading of this plan named four `[SKIPPED:lane]` tests instead, see B2): `image_features_shared_configure`, `out_of_tree_rmw_e2e`, `zephyr_cortex_m_cpp_zenoh_pubsub_e2e`, `zephyr_renode_twin` (issue 2815), `zero_copy::test_zero_copy_message_info` and `sched_dims_applied`. **From 2026-11-23 every scheduled tier 1/2 run stops before building** on `unknown lane 'lane=tier…'` (B11). |

`run-matrix` is the critical path: no run of it has been green since the last release.

### Blocker tracker

This table is the W1 tracker. Each row gets its owner and pull request as they are routed, and
its state as it lands. The state is closed only when a scheduled run of its lane, or a dispatch
on `main` that meets the rule above, is green after the fix.

| # | lane | blocker | issue | owner | PR | state |
| --- | --- | --- | --- | --- | --- | --- |
| B1 | tier 1, tier 2-nightly | linux fixture stage: `features/demo_bringup` parameter store refused | [2885](../issues/archived/2885-linux-features-demo-param-store-refused.md) | nros 175 | `2a83e9aa61` (on main) | gone: a local `lane=linux` fixture build passes the linux stage; archived |
| B2 | tier 2 | the real tier 2 reds of run 44993007292: `image_features_shared_configure` (nros 139), `out_of_tree_rmw_e2e` acme `pool_slots` knob (nros wtf), `zephyr_cortex_m_cpp_zenoh_pubsub_e2e` timeout (nros 139), `zero_copy::test_zero_copy_message_info` (nros 139), `sched_dims_applied` CorePinPlacement a53 (nros 241: placement is correct on 3 solo runs at `811716f239`; the red is B15); the Renode twin is B3 | [2891](../issues/archived/2891-tier2-zephyr-connect-budget-and-action-rate.md) | per test | -- | open: re-read from a tier 2 `workflow_dispatch` on main after #4685 lands |
| B3 | tier 2 | Renode twin: the nucleo C talker faults in logging | [2815](../issues/archived/2815-zephyr-nucleo-c-talker-faults-in-logging.md) | nros 118 | `9974ca0fc9` (on main) | likely fixed: the red run (44993007292 at `98b5d8e0c7`) predates the frames fix; three runs on current main decide, then 118 archives 2815 |
| B4 | nightly | island-shaped Zephyr snapshot stale | [2886](../issues/archived/2886-island-shaped-zephyr-snapshot-stale.md) | NXP 175 | #4688 (merged, `d9b387f9ef`: regenerated snapshot; every delta is the stock node interfaces, phase-556 W13, plus issue 2493; one fixture defect fixed) | fixed: on main; verify on the next scheduled nightly |
| B5 | nightly | stack-floor row: Rust action server on nucleo-f767zi-zephyr overflows RAM by 77,440 B | [2887](../issues/archived/2887-stack-floor-nucleo-action-server-rust-ram-overflow.md) | nros 175 | `617e65bd94`, `ba2fca6ee4` (on main) | fixed: the 2026-11-23 nightly links and walks the row (90,708 B); archived |
| B6 | nightly | ThreadX-Linux `session_open_outlasts_a_late_router` | [2888](../issues/archived/2888-threadx-linux-late-router-session-open.md) | nros 133 | #4699 (docs closure, merged) | closed: a duplicate of issue 2777; the 2026-11-23 nightly passes the test |
| B7 | host-tests | installed-track probe: `nros completions` with an unnamed `$SHELL` | [2892](../issues/archived/2892-installed-probe-completions-shell.md) | nros 139 AUO book | #4683 (merged: install step 5 runs `nros completions bash`) | fixed (issue resolved); verify on the next scheduled `host-tests` installed-track probe |
| B8 | host-tests | feature-sweep: `nros-node [alloc,rmw-lending]` clippy | (PR only) | nros 241 | #4679 (merged, `39561fa93b`) | fixed: on main; verify on the next scheduled `host-tests` |
| B9 | host-tests | rt-sched: `levels-cpp` Zephyr bound table missing; census test | [2890](../issues/archived/2890-levels-cpp-zephyr-bound-table-probe.md) | nros 133 (probe), nros 118 (lane) | #4682 (lane half, merged); #4699 (code half: the probe cache discard keeps the outputs, merged) | fixed: both halves on main; verify on the next scheduled `host-tests` |
| B10 | live-peer | Zephyr Cortex-M publisher to `ros2 topic echo`; runner-board fixture build | [2889](../issues/archived/2889-live-peer-zephyr-cortex-m-ros2-echo.md) | nros 118 | #4684 (lane half, merged); #4631 (the cell failure, [2825](../issues/2825-test-wait-outlasts-nextest-kill.md), merged); `deb4c05454` (the fixture step stopped on a unit-test module's id on 11-22 and 11-23); #4718 (nros 139: the host `ros2_cli_parity_interop` cell compared `service list -t` unsorted after renaming nodes) | dispatch 45148744207 (main `4e4004bd14`): both board cells passed; the host cells failed on that ordering defect and on B17. DONE: scheduled run 45324656188 (main `11a82cf908`, 2026-11-24) green in every job; 2889 resolved |
| B11 | tier 1, tier 2, tier 2-nightly | `fixture-lane: unknown lane 'lane=tier1'` (and `lane=tier2`): every scheduled tier 1/2 run stops here. This, not B1, stopped the 2026-11-23 run | [2893](../issues/archived/2893-lane-arg-spelling-stops-fixture-build.md) | nros 139 | #4685 (merged, `476a3735ba`; `lane-arg-spelling` test) | fixed: on main; verify on the next scheduled tier 1 and tier 2 runs |
| B12 | nightly | `ClockBoard.cpp:35` uses `PRId64`, which the embedded C++ libc lacks: the QemuBaremetal, FreertosMps2 and ThreadxRiscv64 builds stop (nightly 45115997343) | [2894](../issues/archived/2894-clock-board-cpp-pri-macros-embedded.md) | nros 175 | #4707 (merged, `bd723d5a40`: the example prints without `PRId64`) | fixed: on main; verify on the next scheduled nightly |
| B13 | nightly | freertos-posix C-port smoke does not link: the ARM TLS shim (`__aeabi_read_tp` to `__tls_base`) is compiled into the x86 host build (nightly 45115997343) | [2895](../issues/archived/2895-freertos-compat-tls-on-hosted-smoke.md) | nros 175 | #4707 (merged, `ef6cfb8122`: a hosted FreeRTOS build defaults the bare-metal shims off) | fixed: on main; verify on the next scheduled nightly |
| B14 | nightly | ThreadxLinux C-port smoke hangs after `thread stack of 400 bytes is below PTHREAD_STACK_MIN`: the ThreadX fork's linux port refuses a below-minimum timer-thread stack instead of raising it (CLAUDE.md: a stack size is a floor the port raises, issue 0667) | [2896](../issues/archived/2896-threadx-linux-port-below-floor-stack-hangs.md) | nros 133 | #4699 (fork `ee703dae`, pin bump, merged) | fixed: on main; verify on the next scheduled nightly |
| B15 | tier 2 | a readiness marker rides Zephyr's deferred log, which drops lines under load, so a cell that worked reads as a timeout; `sched_dims_applied` CorePinPlacement a53 (B2's last row) is the second site. The release check has no flake allowance, so this blocks | [2847](../issues/archived/2847-zephyr-readiness-marker-rides-droppable-log.md) | nros 241 | #4704 (issue widened to both sites, merged); #4712 (code fix: Zephyr threads wait for log-buffer room instead of dropping, on every deferred-log board, plus a fast gate against opt-outs) | fix in review: draft until its full gate and the deferred-log board cells are green (the class is every Zephyr row except native_sim, 18 rows) |
| B16 | live-peer | the host (`regression`) job's fixture step took 309 min of its 350-min cap on 11-23 (149 builds): every row re-runs a workspace-wide configure that never converges on a runner without the Zephyr SDK, and about 141 rows are built for 41 cells | [2905](../issues/archived/2905-live-peer-regression-fixture-rows-outgrow-hosted-cap.md) | nros 118 | #4715 (configure only the row's image; name rows by test id) | fix in review (draft until its full gate) |
| B17 | live-peer | the two zenoh `ros2_action_e2e` host cells hand the nano binary its router as `NROS_ENTRY_LOCATOR`, a build-time name no run-time reader reads, so it dials the default port (renamed in phase-581 W6) | 2908 (filed in #4718) | nros 139 | #4718 (both sites, plus a gate that a test's run-time env name has a reader) | fix gated green; in review |
| B18 | nightly, tier 2 | a stale `CLI_CONFIG_` cache entry from before a setting's retirement trips the retired-name refusal on a persistent runner tree | [2906](../issues/archived/2906-zephyr-stale-cli-config-cache-trips-retired-refusal.md) | nros wtf (142 carried it) | #4719 (`fb09d58cc5`) | fixed: green in the evidence at `d92c369a14` |
| B19 | tier 1, tier 2 | the census pre-pass skips the `nros-build` compile-check image rows | [2911](../issues/archived/2911-census-prepass-skips-compile-check-nros-image-rows.md) | NXP 175 | #4723 (`2067fd2ff9`) | fixed: green in the evidence at `d92c369a14` |
| B21 | W3 (upgrade) | a project `nros new` made with nros11 writes `rate_hz`, which this release refuses, and the installed upgrade path had no runnable remedy | [2923](../issues/archived/2923-nros11-contract-spelling-refused-no-shipped-migrate.md) | nros 139, 139 AUO book, nros 133 | #4738 (`15e11ffbad`), #4755 (`b493149c41`), #4753 (`e335da7519`), #4736 (`d5c0c0d5b4`, the page) | fixed: green in the evidence at `d92c369a14` |
| B22 | tier 1 | the west seed looks for a bound table under one language root only | [2924](../issues/archived/2924-seed-bound-tables-span-language-roots.md) | NXP 175 | #4739 (`fb2c34e379`) | fixed: green in the evidence at `d92c369a14` |
| B23 | tier 2 | a Zephyr configure caches a `make` the fifo jobserver pool cannot run (`invalid --jobserver-auth string 'fifo:…'`), run 45351389552 | (PR only) | nros 118 | #4787 (`63928bcaa6`) | fixed: green in the evidence at `d92c369a14` |
| B24 | tier 1, tier 2 | a Zephyr leaf signature omitted the renamed locator define, so persistent trees kept baking the default port | [2938](../issues/archived/2938-zephyr-fixture-locator-define-rename.md) | nros 139 | #4790 (`730dfbebeb`) | fixed: green in the evidence at `d92c369a14` |
| B25 | tier 2 | rclc's default parameter server asked for a quota of 4 in a store sized smaller from the declarations | [2939](../issues/archived/2939-runtime-param-store-smaller-than-rclc-default-quota.md) | nros 142 | #4788 (`2414191207`) | fixed: green in the evidence at `d92c369a14` |
| B26 | tier 2 | an audit test ran every workspace row's binary, out-of-lane museum binaries included | [2940](../issues/archived/2940-audit-test-runs-out-of-lane-museum-binary.md) | nros 118 | #4793 (`325fcfce28`) | fixed: green in the evidence at `d92c369a14` |
| B27 | tier 2 | a bringup-scoped root test hard-coded its launch packages | [2944](../issues/archived/2944-bringup-scoped-root-test-hardcodes-launch-packages.md) | nros 118 | #4796 (`de4d542c91`) | fixed: green in the evidence at `d92c369a14` |
| B28 | tier 2 | an interface-subset linkage test lost its subject when the subsets became disjoint | [2945](../issues/archived/2945-interface-subset-linkage-subject-lost-disjoint-subsets.md) | nros 118 | #4796 (`de4d542c91`) | fixed: green in the evidence at `d92c369a14` |
| B29 | tier 2 | threadx-linux's nsos receive notifier asked for a stack below the port's floor and never started | [2946](../issues/archived/2946-threadx-linux-nsos-notifier-below-stack-floor.md) | nros 142 | #4798 (`142994ff77`) | fixed: green in the evidence at `d92c369a14` |
| B30 | nightly | the island-shaped Zephyr snapshot drifted from the derived heap floor; the class (the queue never runs the live snapshot check) is [2957](../issues/2957-knob-change-drifts-island-zephyr-snapshot-unseen.md) | [2780](../issues/archived/2780-zephyr-images-boot-then-stall.md) | NXP 175 | #4823 (`d7c1c8ece7`) | fixed: green in the evidence at `d92c369a14` |
| B31 | tier 2-nightly | the wall-clock latency cells went red beside the rest of the run and host load | [2959](../issues/archived/2959-latency-cells-red-beside-run-and-host-load.md) | nros 118 | #4835 (`e5dcabd3ca`) | fixed: green in the evidence at `d92c369a14` |
| B32 | tier 2-nightly | the census host build shared the image's build tree, so four threadx-linux C++ Cyclone rows failed to build | [2960](../issues/archived/2960-census-host-build-shares-image-build-tree.md) | nros 142 | #4844 (`939ac840b2`) | fixed: green in the evidence at `d92c369a14` |
| B33 | host-tests | rt-sched S1: a level parked in whole milliseconds, then the verdict compared miss rates against a hosted VM's stalls; it now judges telemetry's longest uninterrupted run against 2000 us | [2965](../issues/archived/2965-band-park-rounds-up-to-millisecond.md) | nros 118 | #4851 (`8c936f649a`), #4863 (`5058187aec`) | fixed: green in the evidence at `d92c369a14` |
| B34 | tier 2 | `check::max-frame`: 20 frames over 4096 B, latent since 2026-11-22 and first reached by this tier 2; keys made stable, reports scoped to the lane, latent frames baselined | (PR only) | nros 175 | #4857 (`0b770ede24`) | fixed: green in the evidence at `d92c369a14` |
| B35 | tier 2 | a census recorded in the shared tree before B32 was refused instead of re-taken | [2960](../issues/archived/2960-census-host-build-shares-image-build-tree.md) | nros 142 | #4858 (`cffdca5c9b`) | fixed: green in the evidence at `d92c369a14` |
| B36 | tier 2 | the QEMU violation cell counted verdicts, and an ordinary mps2/an385 tick straddles the 50 ms bound under `shift=auto`; it now identifies the provoked overrun. The drift is the emulator ([2971](../issues/archived/2971-qemu-violation-dispatch-cost-unexplained.md)) | (PR only) | nros 241 | #4864 (`01388e07a3`) | fixed: green in the evidence at `d92c369a14` |
| B37 | live-peer | XRCE never saw a peer leave when its departure sample was lost inside a CPU-starved Agent; the client now re-syncs the graph every `xrce.graph_resync_ms` (default 5000) while graph features are in use (user ruling 2026-11-25: fix in nros12) | [2970](../issues/archived/2970-xrce-graph-misses-departure-lost-in-agent.md) | nros 139 | #4865 (`b3ffe8a739`) | fixed: green in the evidence at `d92c369a14` |
| B38 | tier 2-nightly | QEMU idle-time wakes judged as an on-time share carried the host's idle-wake latency; idle wakes are held to a cap below every non-interrupt fallback | (PR only) | nros 241 | #4866 (`66efdd5593`) | fixed: green in the evidence at `d92c369a14` |
| B39 | tier 2 | the coordinate-truth audit timed out signing 401 rows one at a time; it signs them in one batch | (PR only) | nros 175 | #4871 (`5897d5ac13`) | fixed: green in the evidence at `d92c369a14` |
| B40 | `ci gate` | an example that aborted while its core dump outlasted the deadline read as `timeout`'s 124, i.e. booted | [2973](../issues/archived/2973-abort-during-core-dump-reads-as-timeout.md) | nros 243 | #4872 (`446c8e8d8d`) | fixed: green in the evidence at `d92c369a14` |
| B41 | `ci gate` | a reset-download resume test could pass without resuming | [2974](../issues/archived/2974-download-reset-resume-offset-timing.md) | nros 175 | #4873 (`ee6dfe8ff4`) | fixed: green in the evidence at `d92c369a14` |
| B42 | tier 2-nightly | `test-ignored` ran the cli tests `cli-tests` owns, on a runner without colcon | [2975](../issues/archived/2975-cli-ignored-tests-owned-by-other-lanes.md) | nros 118 | #4880 (`d92c369a14`) | fixed: green in the evidence at `d92c369a14` |
| B43 | island (fresh `nros build`, s32z270-rtu0-zephyr) | the post-link stack-floor walk recursed once per call-graph level, so every fresh jobserver build of a deep image failed with `RecursionError` and a retry passed | 2979 | nros 133 | #4901 (`e9a55424fa`) | fixed: the island's fresh s32z270 build passed on the first try at `ebbb3a570d` (8 walk orders on 8 job tokens) |
| B44 | island (Zephyr FVP lane, multi-bringup configure) | an owned entry's census recorded its inputs relative to the entry package, but configure resolved them against the project root, so every taken owned census read stale and the host configure stopped unseeded; a regression from #4825 (`94a28565c5`), in `ebbb3a570d` and not in `d92c369a14` | the regression filed with PR 4917 | nros 241 | #4917 | OPEN: found 2026-11-26 by the island's FVP lane on the `ebbb3a570d` candidate; the user ruled it a blocker, fixed forward (no revert) |

Owners are the sessions the manager routed each blocker to (2026-11-23). B5 is in the same family as
issue 2772. The 118 session owns the `host-tests` / `live-peer` lane wiring itself.

B18 to B42 were opened while W1 and W2 ran, 2026-11-23 to 2026-11-26, and B43 and B44 during W3's
candidate migrations (B20 was reserved for a NuttX
riscv link red that #4720 fixed before it reproduced, and was never used). Each fix was gated at
its exact head before it merged. Tier 2 first reached its cells on 2026-11-25, which is why late
rows keep appearing: each run that got further reached reds that had been latent behind earlier ones.

Known issues the user ruled NOT blockers (2026-11-26). Each is on the release page and is fixed
after the release:

- [2978](../issues/2978-xrce-sample-over-64k-delivered-truncated.md): an XRCE sample over 65,535
  bytes is lost or truncated with no error. Not new in nros12; the refusal at the publish is the
  first change after the release.
- [2976](../issues/2976-local-delivery-publish-cost-grew.md): a zenoh publish on mps2/an385 costs
  about 51 µs (7 %) more at a fixed instruction count. Bisected after the release.

The release workflow path itself is not a blocker. W3 of phase-567 rehearses it end to end
(the island-shaped workspace job, both starter images, `verify-starter-image.sh` and the
installed-track probe against the candidate asset), and the 2026-11-17 dry run built
`0.6.0-nros12` and passed `PROBE PASS`. The red installed-track probe above is the same probe
on `main`, so W1 has to fix it before the rehearsal can pass again.

## Release steps

### W1 -- every lane green on main

- **Status:** DONE (2026-11-26): every blocker above is fixed on `main`.

Fix each red above on `main`, as its own pull request: file an issue, fix the class, and
re-measure on the next scheduled run, or on a dispatch on `main`. Done when each lane has one
green run on a commit at or after its fix. They do not yet have to be the same commit.

### W2 -- one commit all five lanes passed

- **Status:** DONE (2026-11-26, at `d92c369a14`): `release-lanes-green.py` passes every slow lane on
  dispatches on `main` at that commit (nightly 45742783398, run-matrix tier 1 and tier 2 45742794599,
  host-tests 45742787244, live-peer 45742791109). Refused before it: `0d9fa3e470`, `730dfbebeb`,
  `939ac840b2`, `cffdca5c9b` and `01388e07a3`, each by the rows above that were opened from it.
  The user confirmed the cut at `d92c369a14` on 2026-11-26, after two known issues were ruled not
  to block (the release notes name both): an XRCE sample over 65,535 B is lost or truncated without
  an error, and a zenoh publish on mps2/an385 costs about 7 % more guest time than on 2026-11-09
  (+51 us a dispatch at a fixed `-icount`; the larger drift issue 2971 measured is the emulator).

Two ways to get there. Since the 2026-11-24 ruling, the direct one is to dispatch all four
workflows on `main` at one commit (`gh workflow run <file> --ref main`, with no inputs that narrow
it). Each must finish with every job green, and `main` must not move between the dispatches,
because a dispatch runs whatever `main` is when it starts. The self-hosted runners serve the
03:00 to 06:00 UTC crons as well, so dispatch outside that window and not while a cron's run is
still queued.

The other way is a quiet night. The four crons start at 03:00, 04:00, 05:00 and 06:00 UTC, each on whatever `main` is then.
So a commit passes all five only if no code lands between about 02:50 and 06:20 UTC. Docs-only
merges are fine. On the chosen night, the merge queue lands docs only in that window, or is
paused. Then `release-lanes-green.py --shas-from <that sha>` (run read-only, as this plan did)
must print no `REFUSED` line once the 06:00 `run-matrix` finishes, about 10:30 UTC.

### W3 -- cut and rehearse

- **Status:** OPEN (re-cut pending): `release/nros12` was cut at `d92c369a14` on 2026-11-26 and its
  `publish=false` run (release-nros.yml 45768486500) passed, as did B21's upgrade probes in all three
  languages. The safety-island migration on that candidate found B43, so W2 was measured again at
  `ebbb3a570d` (lanes dispatched 12:31Z), which the island then found carried B44. Both were ruled
  blockers by the user. Once #4917 merges, W2 is measured again at one `main` commit containing it,
  the island's full CI runs at that commit, and `release/nros12` fast-forwards there (no tag exists,
  so the re-cut is allowed). The cross-bringup census staleness, ruled not a blocker (user, 2026-11-26), is
  fixed in the release anyway: PR 4902 merged before the re-cut, so it is off the release page.

1. Create `release/nros12` at the W2 commit. The name may be re-cut from `main` while no
   `nros-0.6.0-nros12` tag exists.
2. Update `book/src/migrating/releases/0.6.0-nros12.md` with the breaking changes above and
   the codegen answer on `main` BEFORE the W2 commit, so the cut carries it. A cherry-pick onto
   the branch does not work for this: W1 measures each pick's SOURCE on `main`, and a docs-only
   source that sits on code landed after the cut is refused on every lane (measured 2026-11-26,
   `8a1ae9ee46`).
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

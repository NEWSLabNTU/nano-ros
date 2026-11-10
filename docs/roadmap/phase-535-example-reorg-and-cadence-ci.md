# Phase 535 — Example pairs into the language workspaces, single-node witnesses, and cadence-backed CI

**Status (2026-11-06). W3 done; W4 next.** W0 answered (issue 2253 fixed on
the way, #3106). W1 merged (#3107). W0.b done: C (#3112), C++ (#3120), Rust
(#3124). W2 merged (#3125). W3: the pilot (#3129) moves the FreeRTOS pairs.
After the builder fan-out (#3142) and the shared jobserver pool (#3159), the
final re-measure is -7.7 % wall clock and -20 % CPU. That is under the 10 %
bar, and the maintainer had ruled (2026-11-05) that the phase proceeds after
one re-measure whatever it shows.
Implements [RFC-0122](../design/0122-behaviour-bringups-road-witnesses-cadence-backed-ci.md)
D1 to D9, with its amendments to RFC-0026, RFC-0051, RFC-0061, RFC-0066,
RFC-0114 and RFC-0121 D8.

Depends on:

- [phase-531](phase-531-workspace-first-project-layout.md) W2 to W5: the
  bringup rule, launch generation and the one build road. This phase replaces
  phase-531 W6 for the pairs under `examples/`, and W6 still migrates the
  witnesses, `packages/testing/` and the templates in place;
- phase-531 W7 step 3 (the selection crate retires) for Rust workspaces with
  more than one bringup. W0 below checks this rather than assuming it;
- issue 2251 (merged): the west lane reads isolation values from
  `nros_tests::alloc`.

## Why

275 example fixture rows build 275 separate projects whose node code the
language workspaces already hold. On the CMake RTOS roads, each one builds its
own kernel, network stack, RMW and `nros-c`. Separately, two support promises
have no CI job: `cadence = "merge"` for linux, zephyr native_sim and
threadx-linux, and tier 3. RFC-0122 has the measurements and the workflow
survey.

## Work items

### W0 — Can a workspace hold several bringups?

- Build a Rust workspace with two bringups on the phase-531 tree and check
  that each bringup's images build with their own selection. Today
  `generate_facade_crates` (`packages/cli/nros-cli-core/src/cmd/ws.rs`)
  declines to generate facades for it.
- Reproduce RFC-0066's C/C++ open question: with three bringups, does one
  bringup's capability set still overwrite another's for the whole configure?

**Acceptance:** both answered with a build. If either fails and is not fixed
here, D1 uses the fallback (image rows in `demo_bringup`) for that language,
and RFC-0122 records it.

**Result (2026-11-04), both answered with a build, neither needs the
fallback:**
- A Rust workspace with a second bringup builds. Each image gets its own
  selection crate (`generated/nros-selection/<entry>`) with the right RMW
  (posix-xrce for an xrce image), and `demo_bringup` still builds beside it.
  `nros configure`'s "selection facades not generated for multi-system
  workspaces" warning is harmless for generated entries.
- The C/C++ capability last-write-wins is unreachable here: no bringup
  declares capabilities.
- Grouping already exists for CMake: images of one (board, rmw, config hash)
  share a configure (`build/posix-zenoh-linux/cmake` holds seven C images).
  Each Rust entry has its own cargo target; nothing is shared beyond sccache.
- The builder qualifies an image as `<bringup>:<image>`
  (`scripts/build/workspace-fixtures-build.sh`); tests resolve it with
  `build_workspace_cmake_entry(row_id, lang, "<image>_entry")` and its cargo
  twin.
- On the way, issue 2253: a generated entry kept the board's panic crate
  under `panic = "platform"` (E0152). Fixed by `[board.entry]
  own_panic_root_*` and `BoardFacts::for_image_panic`.

### W0.b — One program per role (option B)

Maintainer ruling 2026-11-04, recorded in RFC-0122 D1: the example pairs and
the workspace node packages were different programs (tutorial `String`
versus an Int32 demo). The workspace packages of `c`, `rust` and `cpp` become
the tutorial programs, and every test, contract, sizing measurement and
marker expectation that read the Int32 demo moves with them, through
`nros_tests::output` constants. Order: C (one gate run), then Rust and C++,
then the pairs move (W3).

Decisions taken here, with the reason:
- **One-shot client.** The tutorial client sends one request (2 + 3) and
  prints `Result of add_two_ints: 5`. `roundtrip_xprocess_e2e` used to want
  three sums from a client that called every tick; it now asserts the one
  tutorial result, plus, for actions, the whole order-10 sequence and at
  least one feedback line. Repeated calls were a property of the demo
  client, not of the cross-process path the cell exists to prove.
- **A component never blocks.** The client sends from a timer and polls for
  the reply, with a resend guard for a request lost before discovery, and
  prints what the blocking tutorial client prints.
- **Observers follow the type.** While the C++ and mixed talkers still
  publish Int32, `entry_e2e` observes them with the `int32-sink`, and that
  row `serves` their coordinates (a second serves-equality test keeps it
  honest); the C robot2 listener observes the C cells.
- **`mixed` keeps its packages.** It shares none with `c`, `rust` or `cpp`,
  and its purpose is cross-language composition.
- **One bound per stock string type (C++).** The C++ workspace stated
  `std_msgs/String` at 16384 for phase-441 W6's fragmented leg
  (`bulk_listener_pkg`). With the tutorial pair on that type, the listener's
  receive hint (16400 B) exceeded the largest zenoh payload class and the
  subscription declare failed (`rmw_ret error`), and every image would have
  carried a 16 KiB message. The bulk listener moved to
  `example_interfaces/String` (16384) and `std_msgs/String` is 256, as in the
  C workspace and the standalone examples. Measured cost of the tutorial pair
  over the Int32 demo: `linux_entry` .bss +256 B, `threadx_entry` .bss +512 B.
- **The `int32-sink` observer stays.** With C, C++ and Rust on String, its
  `/chatter` role serves only the mixed cells, whose talker publishes Int32.
  It is also the side-topic sink of the param, safety, QoS, realtime, bridge
  and sizing tests (`/ctrl`, `/telem`, `/safe_ok`, …), none of which
  phase-535 touches, so retiring it is not on the table.
- **Rust nodes are the examples' nodes.** The Rust examples' node logic is
  already one file per role, byte-identical across platforms, so the
  workspace packages take that logic verbatim and keep their own class and
  node names (which the bringup, launch files and tests address). The
  in-process service test now reads the client's own result line; it used to
  need an `int32-sink` on `/sum`.

**C++ (2026-11-05):** done. `entry_e2e`'s C++ RTOS cells observe through
the C robot2 listener again and `serves` moved back; roundtrip, multihost,
freertos-posix, the an536 heap and both an536 Cyclone ROS 2 interop tests
read the tutorial markers. Run green on rebuilt fixtures: linux,
threadx-linux, freertos (mps2 zenoh, an536 Cyclone + slirp), freertos-posix
and the two zephyr native_sim west leaves.

**Rust (2026-11-05):** in review. Run green on rebuilt fixtures: linux
(roundtrip, in-process service, deployed system, multihost, per-node graph on
three RMWs, features, metadata) and zephyr native_sim (workspace entry,
multihost robot1). The FreeRTOS bridge pair (`bridge_network_e2e`) needs the
`qemu-br` host bridge and was not run here.

**Acceptance:** each language's workspace tests are green on the lanes that
build its images; no Int32 demo marker is left in a language workspace.

### W1 — The coverage ratchet and the image-row check

- The D5 gate: derive (platform family, language, build driver, project
  shape, launch origin) per runtime cell from the manifest. Fail when a tuple
  in the floor has no cell. The floor is a committed snapshot of the tree at
  adoption, which may only grow.
- The image-row check: every `[image.*]` in a bringup resolves to a known
  board, an existing or generated launch file, and a matrix cell.
- Both are static, on the fast line.

- The manual-entry inventory (RFC-0122 D10): every example `main` that is not
  `nros::main!`, classified as a **manual-entry demo** (kept, with its reason
  written down) or a **generated-entry candidate** (moves to a component).
  Candidates so far: the port templates (`cpp-port-minimal-publisher`,
  `rclcpp-compat-smoke`), `custom-platform`, and `custom-transport-*`. The
  ratchet carries entry form as an axis.

**Acceptance:**
- A mutation that deletes the last witness of a tuple fails the ratchet,
  naming the tuple.
- A mutation that adds an image with an unknown board fails the row check.
- Both mutations are recorded in the commit.

**Done (#3107).** `scripts/check-example-coverage-ratchet.py` (floor
`.config/example-coverage-floor.txt`, 75 tuples at adoption) and
`scripts/check-image-launch-files.py` (262 `system.toml`), both on the fast
line. Board resolution was already `check-deploy-board-resolves`, so the row
check covers the launch file.

### W2 — Grouping, the resolver, and the allocator lookup

- Find out whether the fixture builder groups `[[workspace_fixture]]` rows
  that share (dir, bringup, board, rmw) into one configure. If not, make it.
- `nros_tests::fixtures::workspace_image`: (cell, role) → (workspace,
  bringup, image id). This is the only place that knows the mapping (D4).
- `zephyr-fixture-leaves.sh`: workspace west rows map their image id to a
  workload for the `cell-isolation` lookup (D7).

**Acceptance:**
- One grouped configure builds two images of one bringup, and `ninja -t
  query` shows one runtime library feeding both.
- The resolver is unit-tested on every pair cell in `matrix::CELLS`.

**Result (2026-11-05):**
- Grouping already exists and needs no change. Measured on
  `examples/workspaces/c/build/posix-zenoh-linux/cmake` (one configure, seven
  images): `linux_service_server_entry` and `linux_service_client_entry` both
  link the one `nano_ros/packages/api/nros-cpp/libnros_cpp.a` and
  `libnros_platform_posix.a` of that configure (their `build.make` link
  dependencies; the host lane uses the Makefiles generator, so this is the
  `ninja -t query` question asked of make).
- `nros_tests::fixtures::workspace_image`: `workspace_image(cell, role)` →
  (workspace, bringup, image), `parse_image` its inverse, `row_id` the one
  manifest row, `resolve` the binary through the existing cargo/cmake
  resolvers (a cross Rust row adds its `target` triple). Zephyr images are left
  to the zephyr build-dir table (W4). Unit tests over every pair cell in
  `matrix::CELLS`: total, injective, the inverse round-trips with exactly one
  parse, every pair image row has a cell, and a row in a pair bringup that does
  not parse is refused.
- The image id is `<platform>_<role>[_<rmw>][_<config>…]`. The platform part
  reuses the spellings the workspaces already have (`threadx` for
  threadx-linux, `riscv_threadx`, `riscv_nuttx`, `freertos_posix`) and adds
  `zephyr_cortex_m`, `zephyr_nucleo_f767zi`, `baremetal` and `fvp`. A
  configured cell (RFC-0051 §7a) appends its configs (`freertos_talker_lending`).
  Chosen over deriving the prefix from the fixture token because that would
  rename `threadx`/`riscv_*` against `demo_bringup` in the same workspace.
- `zephyr-fixture-leaves.sh`: `workload_for_image` maps a workspace west row of
  a behaviour bringup to its workload from the bringup, and requires the image
  id to carry one of that bringup's role tokens (a misspelt id exits). The
  emitted records for today's manifest are unchanged (all 87 compared).
- The locator check: `pair_image_rows_dial_their_allocator_port` holds every
  baked zenoh pair image row to `alloc::port_of` of the cell its id names, with
  a negative control (`a_pair_image_dialling_another_port_is_refused`), and
  `resolve` asserts the same per call. `check-image-locator-bake`'s docstring
  points at it.

### W3 — Pilot: `mps2-an385-freertos`

- Move its 18 pair rows into `{rust,c,cpp}/src/{pubsub,service,action}_bringup`
  as `freertos_*` images. Keep `examples/mps2-an385-freertos/<lang>/talker/` as
  the witnesses.
- Measure with `scripts/dev/measure-fixture-build.sh`, cold, before and after,
  as RFC-0066 did. Record it in `docs/roadmap/data/phase-535-w3-pilot.md`.

**Acceptance:**
- `just test freertos` is green, with the same rstest case names as before.
- The ratchet is green.
- The measured saving is recorded, whatever it is.
- A saving under 10 % stops the phase for a maintainer decision before W4.

**Result (2026-11-05): under the bar. Stopped for a maintainer decision.**
Measured with the new `measure-fixture-build.sh --platform freertos
--no-sccache`, cold, two runs before and three after. Full table and
analysis: [phase-535-w3-pilot.md](data/phase-535-w3-pilot.md).

| | before (mean) | after (mean) | change |
| --- | --- | --- | --- |
| wall | 771 s | 1258 s | **+63 %** |
| CPU (user+sys) | 4306 s | 3719 s | **-14 %** (-20 % excluding the run that met heavy host load) |

- **The CPU saving is the C/C++ side.** One configure per language builds the
  kernel, lwIP and the C glue once. The second image of a configure is a link
  of about 8 s.
- **Rust gains nothing.** The leaves already shared a cargo group, and the
  images do not (issue 0616).
- **The wall clock is worse because `workspace-fixtures-build.sh` runs a
  workspace's rows serially.** The leaves ran in parallel.
- **Options for the maintainer:**
  1. Make the builder group by configure rather than by workspace directory,
     then re-measure. This is the likely way past the bar.
  2. Accept the CPU saving and the wall clock cost.
  3. Keep the pairs as leaves and drop D1 for the pairs.
- **Found on the way:** the CMake road needs a per-entry locator and network
  for any multi-image RTOS configure. Two consequences:
  - #3129 adds `LOCATOR` and `IMAGE_NET` per entry.
  - Moved leaves must be deleted with their rows, which
    `check-example-build-coverage` enforces.
- **The pilot works.** `rtos_e2e`'s 10 FreeRTOS cells passed on the pilot
  fixtures with unchanged case names.

**Re-measure (2026-11-05), option 1 taken: still under the bar.** The
builder now fans out by build unit
(`scripts/build/workspace-fixture-units.py`). Rows of one cmake configure, one
cargo target dir or one generated entry stay serial, and everything else runs
at once. Both trees use the new builder. Before is 2 runs and after is 3; one
before run is discarded for host load. Table, loads and method:
[phase-535-w3-pilot.md](data/phase-535-w3-pilot.md#re-measure-with-the-fan-out-builder).

| | before (mean) | after (mean) | change |
| --- | --- | --- | --- |
| wall | 816 s | 846 s | **+3.8 %** |
| CPU (user+sys) | 4501 s | 3738 s | **-17 %** |

- The fan-out took the pilot's wall clock from 1258 s to 846 s. It did not take
  it below the leaves'. The critical path is the serial rows of one configure:
  about 205 s for the first C/C++ image, then six images one after another for
  about 130 s more.
- Further levers, each a decision: one `cmake --build` for all the images of a
  configure; the workspace stage beside the leaf stage in `build-examples`; a
  Rust target shared per (board, rmw) behind D6's gate.
- By the rule above, W4 did not start then. See the final re-measure below.

**Levers pulled (2026-11-05, maintainer: one more lever, re-measure once, then
proceed).** The freertos leaf stage and workspace stage now run side by side
under one jobserver pool, and fan-out drivers under an inherited pool join it
instead of opening a second one. The first lever, one `cmake --build` over
every image of a configure, is not a builder change. The seven rows of one
C/C++ configure belong to four bringups, and the generated root carries one
bringup's entries at a time. Within a bringup, the untargeted `cmake --build`
already builds every entry in one ninja graph. Each bringup switch rebuilds
the Rust runtime, 22 to 32 s measured: [issue 2287](../issues/archived/2287-cmake-coordinate-bringup-switch-rebuilds-runtime.md), resolved by giving each bringup its own build dir.

**Final re-measure (2026-11-06): W3 done.** Two interleaved runs per tree,
cold, no run discarded for load:

| | before (mean) | after (mean) | change |
| --- | --- | --- | --- |
| wall | 678 s | 626 s | **-7.7 %** |
| CPU (user+sys) | 2161 s | 1728 s | **-20 %** |

Wall clock is still under the 10 % bar. By the maintainer's ruling, W4
starts. Table and caveats:
[phase-535-w3-pilot.md](data/phase-535-w3-pilot.md#final-re-measure-2026-11-06-after-the-shared-pool).

### W4 — The merge boards: linux, zephyr native_sim, threadx-linux

Same move as W3, all three RMWs. `demo_bringup`'s `linux_service_*` and
`linux_action_*` images move into the behaviour bringups, so each image exists
once.

**Acceptance:** `just ci tier1` is green, and
`zephyr::example_e2e` keeps its 27 case names. The cross-Agent XRCE repro of
issue 2221 (two Agents, two cells) hears 0 foreign samples.

One PR per board.

- **linux (merged, #3211).** Each language workspace gets 18
  `linux_<role>[_xrce|_cyclonedds]` images in its three behaviour bringups.
  `demo_bringup`'s service and action images move there, and their row ids
  stay. 39 leaf rows and 8 leaves retire: the C listener, service and
  action leaves, and the C++ listener, service server and action server. A few
  leaves stay until W6, because each carries something the workspace program
  does not have, and each keeps one plain row:
  - Rust `listener`: its `link-tls` and peer-mode rows.
  - Rust service and action leaves: the `linux[_<rmw>]_<edition>` images that
    the opt-in ROS-edition lane builds.
  - C++ `service-client`: a hand-written `main`, which the census test needs.
  - C++ `action-client`: it reads `NROS_TEST_GOAL_ORDER`, for the
    goal-rejection test.

  Every
  resolver that named a moved leaf now resolves its workspace image
  (`fixtures::linux_pair_image`). The 32 pair cells
  (`linux_example_{pubsub,reqresp}_e2e`) pass with unchanged case names.
  Found on the way: issue 2297. The fixture builder wipes a leaf's
  `generated/` on codegen-stamp drift, and `nros configure` then reports up
  to date without regenerating it. The wipe now retires the configure stamps
  too.
- **threadx-linux (merged, #3225).** Each behaviour bringup gets `threadx_<role>`
  images for all three languages (zenoh, loopback locator = the cell's
  `alloc::port_of`), and `threadx_<role>_cyclonedds` for C (every workload)
  and C++ (pubsub), at the leaves' domains 107 and 108. 25 leaf rows and 14
  leaves retire. The talker leaves stay as witnesses, and the Rust listener
  leaf keeps only its connect-forever row. The C++ Cyclone service and action
  leaf rows had no cell and no consumer, so they were not carried over. The
  SIGTERM and exit-status cases in `linux_api` measure a hand-written `main`,
  so they now run the C talker witness. `matrix_fixture_coverage` models a
  pair image row as its Example cell. The first nightly after it failed the
  quiet-link cell: the image's `connect` was interrupted by the ThreadX
  Linux port's preemption signal and reported `ConnectionFailed` at the
  first tick (issue 2351, fixed in `nsos-netx`). The cell now prints its
  router's state when the image does not come up, and the nightly uploads
  `test-logs/fixtures/` per platform.
- **zephyr native_sim (in review).** Each behaviour bringup gets
  `zephyr_<role>[_xrce|_cyclonedds]` images: 48 in all, every
  (language, role, rmw) the leaves had except Rust on Cyclone. Their router
  port, Agent port and domain are the cell's allocator values (the
  `cell-isolation` table); the west lane passes the same values as defines.
  The Kconfig fragments moved from the leaves to each bringup's
  `boards/native_sim_native_64/`, and the per-role XRCE pool and heap knobs
  moved onto the images. Each image is a `[[workspace_fixture]]` row with its
  own `west_build_name` (`build-ws-<lang>-<role>-<rmw>`), which
  `workspace_image::resolve` reads back, so `example_e2e` keeps its 27 case
  names and resolves both roles through `fixtures::workspace_image`. 40 leaf
  rows and 10 leaves (C and C++) retire. The talker leaves stay as the board's
  witnesses, with all nine of their rows. The Rust listener, service and
  action leaves keep only their Cyclone rows: a generated Rust Zephyr entry
  cannot run Cyclone yet (issue 2313), so those cells still resolve the
  leaves. The Rust action-server leaf also keeps its zenoh row:
  `zephyr_rust_zenoh_action_goal_completion_rate` reads its
  `reply-slot: refusals=` heartbeat, which the workspace server does not
  print. The listener readiness marker is now `LISTENER_READY_MARKER`,
  which every workspace listener prints; the Rust action cells wait for
  `ACTION_SERVER_READY_MARKER`. Found on the way, all from the Zephyr C++
  configure-time census check, which no other road runs:
  - the C++ service and action contracts declared timer rates the code does
    not use (the programs tick every 500 ms);
  - the census join never matched an action the C++ recorder reports, since
    it records an action as its five RMW constituents; the census side folds
    them into the action now;
  - the C++ action server needs `CONFIG_MAX_PTHREAD_COND_COUNT=24`.

### W5 — The other platforms

nuttx (arm, riscv), threadx-riscv64 (with its cyclonedds rows) and
mps2-an385 bare metal (its `-rtic` and `-rtic-mixed` variants moved in W6).
One PR per platform directory, each rebuilding its own lane.

**Acceptance:** `just test <platform>` is green for each; `just ci matrix`
(tier 2) is green at the end.

- **threadx-riscv64 (in review).** Each behaviour bringup gets
  `riscv_threadx_<role>` images for all three languages (zenoh, locator =
  the cell's `alloc::port_of` on QEMU's SLIRP host), except C++ pubsub. 14
  zenoh leaf rows and 8 leaves (the C and C++ service and action leaves)
  retire. The talker
  leaves stay as the board's witnesses; `typed-carrier` and
  `errno-isolation` stay until W6. Cyclone stays on the leaves, so every
  listener, service and action leaf that has a Cyclone row keeps it (the C
  and C++ listeners, the five Rust roles): a generated Rust entry boots with
  no Cyclone backend registered here too (issue 2313's class), and a C/C++
  two-QEMU pair needs two network identities, which images of one configure
  cannot have yet (issue 2336). Rust on zenoh builds through the cargo road,
  where the leaves used a Corrosion/cmake seam. Found on the way:
  - issue 2316: parallel image builds of one workspace raced. The riscv64 lld
    wrapper rewrites the configure's shared archives in place (SIGBUS), and
    parallel `nros build`s tore the workspace's `nros-sdk.lock`.
  - issue 2335: the board's newlib `__retarget_lock_init` read `*lock`
    instead of allocating one, so every generated C++ entry faulted on its
    first unbuffered `fprintf`.
  - issue 2336 (open): an image's `os_config` network file is per CONFIGURE,
    so two images of one configure link one address.
  - issue 2340 (resolved by phase-548 W4): the generated C++ talker
    published once and hung, because the riscv64 port's `TX_DISABLE` asm
    was not `volatile` and `_tx_mutex_delete` returned with interrupts
    masked. The C++ pubsub cell still resolves its leaves; adding the
    workspace C++ pubsub images is now unblocked.

- **nuttx arm (in review).** Each behaviour bringup gets `nuttx_<role>`
  images for all three languages (zenoh, locator = the cell's `alloc::port_of`
  on QEMU's SLIRP host), at the per-bringup build dir
  `build/nuttx-zenoh-qemu-armv7a-nuttx-<bringup>/cmake`. 15 leaf rows and 15
  leaves (the listener, service and action leaves of each language) retire;
  every one was zenoh, so no RMW loses its last row. The talker leaves stay as
  the board's witnesses, and `c/typed-carrier` stays until W6. `rtos_e2e`'s
  NuttX arms and `entry_build`'s NuttX roles resolve through
  `nuttx::pair_image` over `workspace_image::resolve_coord_cached`. The ratchet
  tuple `nuttx cpp cmake single generated generated` (a one-image C++ leaf
  with a generated entry) is no longer produced. It was never in the floor,
  and C keeps the same shape on the board. Found on the way:
  - a NuttX configure could hold only one image: every image wrote
    `nros-nuttx-ffi` to one per-configure path, so the second image's custom
    command named the first one's output and cmake refused the configure. The
    artifact dir is now per image (`nros-nuttx.cmake`).
  - a workspace Rust cross image resolved at the ambient cargo profile, while
    NuttX builds at its carve-out (`nros-minsizerel`).
    `build_workspace_rust_cross_entry` now asks the row's platform profile,
    as leaf rows do.
- **nuttx riscv.** Nothing to move: `examples/rv-virt-nuttx` holds only the C
  talker witness.
- **mps2-an385 bare metal (in review).** The Rust workspace's pubsub bringup
  gets `baremetal_talker` and `baremetal_listener`, with the locator at the
  cell's `alloc::port_of` (10200). `test_qemu_bsp_pubsub_e2e` is the
  `(QemuBaremetal, Rust, Pubsub)` pair cell's test, and it resolves both
  roles through `fixtures::workspace_image`. The RTIC pubsub pair used that
  port, so it moves to a new aux slot, `alloc::BAREMETAL_RTIC_PUBSUB_PORT`
  (10540). There are no plain Rust service or action leaves on this board,
  only the RTIC variants, and those are W6. No leaf retires:
  - The Rust `talker` is the witness.
  - The Rust `listener` keeps its row, because `tests/qemu-baremetal`
    builds its `docker` image in the leaf. That image is a configuration,
    so it moves in W6.
  - The twelve C/C++ leaves stay as they are. A C/C++ workspace image goes
    to the cmake driver, which knows no bare-metal board, and the link root
    on this board has to be Rust (issue 2377). These leaves are still the
    only images for the six C/C++ Runtime cells.

The coverage ratchet (`check-example-coverage-ratchet`) does not count the
RMW: a tuple is (platform, lang, driver, shape, launch, entry), so a move
that drops the last Cyclone or XRCE row of a platform passes it. Each W4/W5
PR checks that by hand (a moved row with no cell gets a cell); the ratchet
gaining an `rmw` field is a follow-up.

### W6 — Variants, configurations and feature demos

- The 22 variants become node packages.
- The 7 configurations become image rows with knobs.
- The 13 feature demos become node packages in `features` or the language
  workspace.
- `talker-xrce` becomes an `rmw` row.
- Zephyr Rust Cyclone leaves: retired by issue 2313 (nros 133), outside this
  item and W7. `examples/zephyr/rust/{listener,service-server,service-client,
  action-client}` are deleted; each cyclonedds row is now a
  `zephyr_<role>_cyclonedds` image of the Rust behaviour bringups. W6 skips
  them. `examples/zephyr/rust/action-server` stays a leaf for its zenoh row
  (the `reply-slot: refusals=` heartbeat the workspace action server does not
  print); only its cyclonedds row moved.

All per RFC-0122 D2.

**Acceptance:** each row's consumer test is green; the ratchet floor is unchanged.

A variant's image id carries its token after the role:
`<platform>_<role>[_<variant>][_<rmw>][_<config>…]`. The token is a
`workspace_image::Variant`, read by the same `parse_image` as every pair
image, so there is one parser and one spelling. A variant is not a cell axis:
its row is modelled by the same coordinate's Example cell as the plain role,
and a test asks for it by name (`resolve_coord_variant`).

- **linux callback clients (merged, #3394).** The C and C++ `service-client-callback`
  leaves and the C++ `action-client-callback` leaf become node packages
  (`service_client_callback_pkg` in `c` and `cpp`, `action_client_callback_pkg`
  in `cpp`) and images `linux_service_client_callback[_cyclonedds]` and
  `linux_action_client_callback`. 5 leaf rows and 3 leaves retire; each row
  keeps its RMW. `linux_api`'s callback cases resolve the images. The Rust
  `service-client-callback` leaf stays, with its reason in its row: the Rust
  declarative `Component` API has no callback-receive service client
  (`create_client_with_callback` is reachable only from an `Executor` that a
  hand-written `main` opens).
- **linux `*-rtic` (stay).** The six Rust leaves are hand-written `main`s, and
  that is what they demonstrate: the caller owns the loop (a zero-arena
  executor driven by `spin_once(0)`, publishing outside it), which a
  generated entry cannot express on the host. RFC-0122 D10 keeps such a
  `main`. On mps2-an385 the RTIC pattern is a board (`rtic-mps2-an385`), so
  those leaves become images.
- **linux serial (merged, #3404).** On linux a serial link is not a build
  configuration: the locator is runtime env, and the XRCE session routes a
  `serial://<path>` locator to its POSIX serial transport. The
  `rust/serial-{talker,listener}` leaves retire, with their 2 rows, and
  `xrce.rs`'s serial cases run the plain XRCE talker witness and the
  `linux_listener_xrce` image with `NROS_LOCATOR=serial://<pty>`.
- **bare-metal serial and XRCE (in review).** The mps2-an385
  `rust/serial-{talker,listener}` and `rust/talker-xrce` leaves retire, with
  their 3 rows. They become images of the Rust workspace's `pubsub_bringup`:
  `baremetal_{talker,listener}_serial` (`serial/UART_0`) and
  `baremetal_talker_xrce_serial` (`custom://uart`, XRCE knobs on the image).
  A new `matrix::Config::Serial` (registry row `[knob.NROS_LINK_IP]`) names
  the configuration, and two configured Runtime cells, Rust zenoh and Rust
  xrce, bind `emulator.rs`'s serial and xrce cases. A serial image dials no
  router port, so the port check skips it.
- **bare-metal RTIC (in review).** The eight mps2-an385
  `rust/{talker,listener,service-server,service-client,action-server,action-client}-rtic`
  and `rust/{talker,listener}-rtic-mixed` leaves retire, with their 8 rows.
  They become the `baremetal_<role>_rtic` images (board `rtic-mps2-an385`) of
  the Rust workspace's pubsub, service and action bringups, and three
  `matrix::Config::Rtic` Runtime cells bind `emulator.rs`'s rtic cases. Three
  enablers: `nros::main!`'s RTIC arm registers a launch-addressed entry's
  launch node set; a generated RTIC entry depends on `rtic` and the device
  crate; the workspace road resolves board facts per image (two board words
  of one descriptor differ on `NROS_BOARD`). The `-rtic-mixed` pair is the
  plain pair on another port: its two-priority split went in phase-244 D1,
  and its `nros/ffi-sync` reaches no code (issue 2417), so it retires into
  the RTIC pubsub images with no coverage lost.
- **bare-metal `rust/listener`'s `docker` image (stays).** The compose test
  (`tests/qemu-baremetal`, `just docker` recipes) builds BOTH roles' `docker`
  images in their leaves by one rule; the talker leaf is the board's witness
  and keeps its own. Moving the listener's would split `entrypoint.sh` by
  role, and no CI lane runs the compose test to prove the move.

### W7 — Retire the old leaves, add `road_witness_e2e`

- Delete every pair, variant, configuration and feature leaf moved by W3 to
  W6. Keep the 22 witnesses and `custom-platform`.
- `road_witness_e2e`: each witness talker against the workspace listener of the
  same (platform, language, rmw).
- Sweep every hard-coded leaf path from the 2026-11-04 survey:
  - `nightly.yml`: path filters 197-203, the 10 Zephyr leaves 819-829, `c/talker`
    at 1036 and 1142;
  - `just/ci.just` 229-231, and `justfile` `rust-rtos-link-check`;
  - `board-support.toml` `link_check_example`;
  - `just/zephyr-dev.just`, `just/zephyr-ci.just` 238;
  - the per-platform `just/*.just` recipes;
  - `justfile:360`.

  Most of these name `rust/talker` and keep working. Each one is checked, not
  assumed.

**Acceptance:**
- `git ls-files 'examples/*/*/*/'` lists only witnesses, `custom-platform`
  and templates.
- `just ci full` is green with a lower fixture-configure count.
- The book's example pointers resolve.

### W8 — CI follows the cadences

1. `merge-runtime` on `merge_group`, **advisory**: one hosted shard per merge
   board, running the behaviour bringups (zenoh, three languages) plus that
   board's witnesses, then their pair cells.
2. After a week, if p95 is at most 20 min per shard, it joins `CI`. Otherwise
   the three boards move to `cadence = "daily"` and `support-status.md`
   stops saying "every change" (RFC-0122 D9).
3. Tier 1 full moves to hosted shards, daily.
4. The Zephyr nightly goes from 20 hard-coded build jobs to 6 (2 lines × 3
   languages), with native_sim runtime.
5. `release-gate`: `ci full` on dispatch; `release-nros.yml` requires a green
   run on the tagged commit.
6. Stale headers, fixed against the workflows they describe:
   - `nightly.yml:25` (no PR trigger)
   - `post-submit.yml:38-40`
   - `queue.yml:95-99`
   - `docs/development/multi-agent-ci-workflow.md` 36-44
   - `support-status.md` tier-2 trigger wording

**Acceptance:**
- Every `cadence` in `board-support.toml` names a job that ran in the last
  window. A gate reads both files and refuses a cadence with no job.
- A release cannot be cut without a green tier 3 on its commit.

### W9 — Docs

- RFC-0122 flips to Stable.
- The book teaches the generated entry (RFC-0122 D10). Coordinated with
  phase-531 W8, which rewrites the same chapters for the workspace layout; a
  hand-written `main` appears only in the manual-entry chapter.
- `examples/README.md`, `examples/workspaces/README-layout.md` (the new
  bringups and the image-id vocabulary), the book's first chapters (witness
  paths), and `book/src/reference/support-status.md`.
- One pointer line in CLAUDE.md.

## Acceptance (phase)

- The ratchet floor from W1 holds at every merge of this phase.
- Rstest case names in the pair consumers are unchanged.
- The measured fixture-build time of `lane=all` is recorded before W3 and
  after W7.
- Every RFC-0114 cadence has a job (W8), or has been lowered.

## Issues homed here (2026-11-07)

| issue | why it belongs here |
| --- | --- |
| [#2336](../issues/archived/2336-board-net-is-per-configure-not-per-image.md) | An image's `os_config` network file is applied per cmake configure, so images sharing one configure clash: found in this phase's workspace images. Resolved 2026-11-10: each ThreadX entry gets its own config TU. |
| [#2383](../issues/archived/2383-live-peer-host-fixtures-miss-c-workspace-rows.md) | W4 made the linux pairs C workspace images; live-peer's derived host fixture set still builds only Rust rows, so its bridge and interop cells are UNMET (found closing phase-466). Resolved: the derivation now reaches pair images by coordinate and by `linux_pair_image` call. |
| [#2377](../issues/2377-workspace-road-no-c-image-on-bare-metal.md) | The workspace road has no C/C++ image for a board with a Rust link root, so the bare-metal C/C++ pairs stay leaves (W5). |

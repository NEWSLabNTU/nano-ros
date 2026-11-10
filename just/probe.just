set working-directory := '..'

default:
    @just --list --justfile {{source_file()}}

# Clean-system bootstrap probe (issue #204): run the book's documented setup
# steps verbatim in pristine containers and assert the documented outcome —
# BOTH front doors, because they are two different machines:
#   1. the CHECKOUT track (ros:jazzy: ubuntu:24.04 + ROS 2): clone + bootstrap, then the first-node
#      chapter. What everyone does until the first release is cut.
#   2. the INSTALLED track (phase-447 A3): install a release, then
#      run-a-talker.md (the serial QEMU chapter, timed) with NO checkout on the host.
#      The released `nros` could not build anything and nothing noticed, because
#      every host that ran it had a checkout the SDK-root ladder reached first.
# ~30-60 min cold each. Knobs: PROBE_BRANCH, PROBE_CLONE_URL, PROBE_IMAGE,
# PROBE_KEEP=1, and for the installed track PROBE_ASSET / PROBE_RELEASE_CACHE
# (see scripts/probe/run-bootstrap-probe.sh).
[group("main")]
bootstrap:
    ./scripts/probe/run-bootstrap-probe.sh
    PROBE_TRACK=installed ./scripts/probe/run-bootstrap-probe.sh

# The CHECKOUT track alone: clone + bootstrap in a pristine container, then the
# first-node chapter. Named so a lane can run the two tracks as separate jobs —
# they are two different machines (see `bootstrap`), so a failure should say
# WHICH front door broke, and a serial `bootstrap` hides the second track
# behind the first.
[group("main")]
checkout:
    ./scripts/probe/run-bootstrap-probe.sh

# The installed track alone: build the asset `release-nros.yml` would publish
# (its own steps, extracted, on its own `runs-on` image), install it into a
# second pristine container with nothing else mounted, and run run-a-talker.md.
# PROBE_ASSET=<dir> installs a prebuilt asset instead — e.g. a `release nros`
# run's artifact, `gh run download <id> -n nros-linux-x86_64 -D <dir>`.
[group("main")]
installed:
    PROBE_TRACK=installed ./scripts/probe/run-bootstrap-probe.sh

# phase-517 W10 — the installed track, one QEMU chapter per recipe: install the
# release asset into a clean `ros:humble` container (the chapters' router is
# ROS's `rmw_zenohd`), then replay the chapter's `probe=NN` blocks — setup,
# scaffold, build, and each Run step, which must print the output block the
# chapter documents beside it (`probe-expect`). Opt-in: never part of a tier or
# a merge-gating lane; the weekly `probe-platforms` workflow runs every one.
# PROBE_ASSET=<dir> skips the asset build (`platform-asset` below makes one).
# `platform` is the one recipe; the named ones below spell each chapter.
# Replay one platform chapter on the installed track (see the recipes below).
[group("main")]
platform name:
    PROBE_TRACK=installed PROBE_PLATFORM={{name}} ./scripts/probe/run-bootstrap-probe.sh

# Build the release asset once, for several `PROBE_ASSET=<out> just probe <platform>`.
[group("main")]
platform-asset out="tmp/probe-asset":
    PROBE_TRACK=installed PROBE_ASSET_OUT={{out}} ./scripts/probe/run-bootstrap-probe.sh

# book/src/tutorial/what-you-just-built.md — Tutorial 2, replayed after
# chapter 1 on the same project (phase-537 W5).
[group("main")]
tutorial-2: (platform "tutorial-2")

# book/src/tutorial/add-a-second-node.md — Tutorial 3 (phase-537 W5).
[group("main")]
tutorial-3: (platform "tutorial-3")

# book/src/tutorial/pick-the-rmw.md — Tutorial 4 (phase-537 W5).
[group("main")]
tutorial-4: (platform "tutorial-4")

# book/src/tutorial/your-own-message.md — Tutorial 5 (phase-537 W5).
[group("main")]
tutorial-5: (platform "tutorial-5")

# book/src/tutorial/services-and-actions.md — Tutorial 6 (phase-537 W5).
[group("main")]
tutorial-6: (platform "tutorial-6")

# book/src/tutorial/parameters-and-launch-files.md — Tutorial 7 (phase-537 W5).
[group("main")]
tutorial-7: (platform "tutorial-7")

# book/src/tutorial/tell-the-build.md — Tutorial 8 (phase-537 W5).
[group("main")]
tutorial-8: (platform "tutorial-8")

# book/src/tutorial/another-target.md — Tutorial 9 (phase-537 W5).
[group("main")]
tutorial-9: (platform "tutorial-9")

# book/src/tutorial/talk-to-ros2.md — Tutorial 10 (phase-537 W5).
[group("main")]
tutorial-10: (platform "tutorial-10")

# book/src/tutorial/talk-to-ros2.md — Tutorial 1-10 (C) (phase-537 W5).
[group("main")]
tutorial-c: (platform "tutorial-c")

# book/src/tutorial/talk-to-ros2.md — Tutorial 1-10 (C++) (phase-537 W5).
[group("main")]
tutorial-cpp: (platform "tutorial-cpp")

# book/src/tutorial/qos.md — Lab: QoS is checked twice (Rust), after Tutorial 1-3
# (phase-537 W5, issue 2269).
[group("main")]
lab-qos: (platform "lab-qos")

# book/src/tutorial/qos.md — Lab: QoS is checked twice (C), after Tutorial 1-3
# (phase-537 W5, issue 2269).
[group("main")]
lab-qos-c: (platform "lab-qos-c")

# book/src/tutorial/qos.md — Lab: QoS is checked twice (C++), after Tutorial 1-3
# (phase-537 W5, issue 2269).
[group("main")]
lab-qos-cpp: (platform "lab-qos-cpp")

# book/src/tutorial/two-tiers.md — Lab: two sched tasks keep a promise (Rust),
# after Tutorial 1 (phase-537 W5, issue 2269).
[group("main")]
lab-tiers: (platform "lab-tiers")

# book/src/tutorial/two-tiers.md — Lab: two sched tasks keep a promise (C),
# after Tutorial 1 (phase-537 W5, issue 2269).
[group("main")]
lab-tiers-c: (platform "lab-tiers-c")

# book/src/tutorial/two-tiers.md — Lab: two sched tasks keep a promise (C++),
# after Tutorial 1 (phase-537 W5, issue 2269).
[group("main")]
lab-tiers-cpp: (platform "lab-tiers-cpp")

# book/src/starters/linux-host.md — the Linux host starter: a C++ talker over
# Cyclone DDS, no router (weekly).
[group("main")]
linux: (platform "linux")

# book/src/starters/mps2-an385-bare-metal.md — Rust talker on QEMU MPS2-AN385.
[group("main")]
baremetal: (platform "baremetal")

# phase-523 W4 — the bare-metal starter OFFLINE: once online to stage the
# release asset and the SDK store, then again with `--network none` and
# NROS_OFFLINE=1 from that store (vendored-tree.md "Air-gapped / mirrored CI").
[group("main")]
offline: (platform "offline")

# book/src/starters/mps2-an385-freertos.md — the Rust tab, on QEMU MPS2-AN385.
[group("main")]
freertos: (platform "freertos")

# book/src/starters/mps2-an385-freertos.md — the C tab, on the same board
# (phase-517 W17: the chapter's other language road).
[group("main")]
freertos-c: (platform "freertos-c")

# book/src/starters/threadx-linux.md — the Rust tab, on the Linux simulator.
[group("main")]
threadx-linux: (platform "threadx-linux")

# book/src/starters/threadx-riscv64.md — the RISC-V 64 QEMU flavour.
[group("main")]
threadx-riscv64: (platform "threadx-riscv64")

# book/src/starters/threadx-linux.md — the C tab, on the Linux simulator
# (phase-517 W17).
[group("main")]
threadx-linux-c: (platform "threadx-linux-c")

# book/src/starters/nuttx-arm.md — the C tab, on QEMU Arm `virt`.
[group("main")]
nuttx-arm: (platform "nuttx-arm")

# book/src/starters/nuttx-arm.md — the Rust tab, on Arm virt (issue 2149).
[group("main")]
nuttx-arm-rust: (platform "nuttx-arm-rust")

# book/src/starters/nuttx-riscv.md — the C tab, on QEMU RISC-V `rv-virt`.
[group("main")]
nuttx-riscv: (platform "nuttx-riscv")

# book/src/starters/nuttx-riscv.md — the Rust tab, on rv-virt (issue 2178).
[group("main")]
nuttx-riscv-rust: (platform "nuttx-riscv-rust")

# book/src/starters/zephyr-native-sim.md — the C tab, on native_sim.
[group("main")]
zephyr: (platform "zephyr")

# book/src/starters/zephyr-native-sim.md — the Rust tab, on native_sim
# (phase-517 W17; issues 2151, 2152).
[group("main")]
zephyr-rust: (platform "zephyr-rust")

# book/src/starters/mps2-an385-zephyr.md — a Rust talker on Zephyr's
# mps2/an385 in QEMU (phase-537 W4).
[group("main")]
zephyr-mps2: (platform "zephyr-mps2")

# book/src/starters/freertos-posix.md — a C talker on the FreeRTOS POSIX
# simulator (phase-537 W4).
[group("main")]
freertos-posix: (platform "freertos-posix")

# book/src/starters/linux-host.md — the Rust tab (phase-548 W8).
[group("main")]
linux-rust: (platform "linux-rust")

# book/src/starters/linux-host.md — the C tab (phase-548 W8).
[group("main")]
linux-c: (platform "linux-c")

# book/src/starters/mps2-an385-freertos.md — the C++ tab (phase-548 W8).
[group("main")]
freertos-cpp: (platform "freertos-cpp")

# book/src/starters/threadx-linux.md — the C++ tab (phase-548 W8).
[group("main")]
threadx-linux-cpp: (platform "threadx-linux-cpp")

# book/src/starters/threadx-riscv64.md — the C tab (phase-548 W8).
[group("main")]
threadx-riscv64-c: (platform "threadx-riscv64-c")

# book/src/starters/threadx-riscv64.md — the C++ tab (phase-548 W8).
[group("main")]
threadx-riscv64-cpp: (platform "threadx-riscv64-cpp")

# book/src/starters/nuttx-arm.md — the C++ tab (phase-548 W8).
[group("main")]
nuttx-arm-cpp: (platform "nuttx-arm-cpp")

# book/src/starters/nuttx-riscv.md — the C++ tab (phase-548 W8).
[group("main")]
nuttx-riscv-cpp: (platform "nuttx-riscv-cpp")

# book/src/starters/zephyr-native-sim.md — the C++ tab (phase-548 W8).
[group("main")]
zephyr-cpp: (platform "zephyr-cpp")

# book/src/starters/mps2-an385-zephyr.md — the C tab (phase-548 W8).
[group("main")]
zephyr-mps2-c: (platform "zephyr-mps2-c")

# book/src/starters/mps2-an385-zephyr.md — the C++ tab (phase-548 W8).
[group("main")]
zephyr-mps2-cpp: (platform "zephyr-mps2-cpp")

# book/src/starters/freertos-posix.md — the Rust tab (phase-548 W8).
[group("main")]
freertos-posix-rust: (platform "freertos-posix-rust")

# book/src/starters/freertos-posix.md — the C++ tab (phase-548 W8).
[group("main")]
freertos-posix-cpp: (platform "freertos-posix-cpp")

# book/src/starters/freertos-posix.md — the zenoh section, Rust tab (phase-548 W8).
[group("main")]
freertos-posix-zenoh-rust: (platform "freertos-posix-zenoh-rust")

# book/src/starters/freertos-posix.md — the zenoh section, C tab (phase-548 W8).
[group("main")]
freertos-posix-zenoh: (platform "freertos-posix-zenoh")

# book/src/starters/freertos-posix.md — the zenoh section, C++ tab (phase-548 W8).
[group("main")]
freertos-posix-zenoh-cpp: (platform "freertos-posix-zenoh-cpp")

# book/src/starters/nucleo-f767zi.md — the Rust tab, in the Renode twin (phase-548 W8).
[group("main")]
renode-nucleo-rust: (platform "renode-nucleo-rust")

# book/src/starters/nucleo-f767zi.md — the C++ tab, in the Renode twin (phase-548 W8).
[group("main")]
renode-nucleo-cpp: (platform "renode-nucleo-cpp")

# book/src/guide/platforms/linux-host.md — the the Linux host platform page, after its
# starter: the run plan and the native build line (phase-537 W9).
[group("main")]
platform-linux: (platform "platform-linux")

# book/src/extending/custom-rmw.md — an out-of-tree RMW backend in a Linux host
# workspace, selected by name and run against a router (phase-545 W4).
[group("main")]
custom-rmw: (platform "custom-rmw")

# book/src/guide/platforms/zephyr-native-sim.md — the Zephyr native_sim platform page, after its
# starter: the run plan and the native build line (phase-537 W9).
[group("main")]
platform-zephyr-native-sim: (platform "platform-zephyr-native-sim")

# book/src/guide/platforms/threadx-linux.md — the the ThreadX Linux simulator platform page, after its
# starter: the run plan and the native build line (phase-537 W9).
[group("main")]
platform-threadx-linux: (platform "platform-threadx-linux")

# book/src/guide/platforms/freertos-posix.md — the FreeRTOS POSIX platform page, after its
# starter: the run plan and the native build line (phase-537 W9).
[group("main")]
platform-freertos-posix: (platform "platform-freertos-posix")

# book/src/guide/platforms/bare-metal.md — the bare-metal Cortex-M3 platform page, after its
# starter: the run plan and the native build line (phase-537 W9).
[group("main")]
platform-baremetal: (platform "platform-baremetal")

# book/src/guide/platforms/freertos.md — the FreeRTOS on mps2-an385 platform page, after its
# starter: the run plan and the native build line (phase-537 W9).
[group("main")]
platform-freertos: (platform "platform-freertos")

# book/src/guide/platforms/zephyr.md — the Zephyr on mps2-an385 platform page, after its
# starter: the run plan and the native build line (phase-537 W9).
[group("main")]
platform-zephyr-mps2: (platform "platform-zephyr-mps2")

# book/src/guide/platforms/nuttx.md — the NuttX Arm and RISC-V platform page, after its
# starter: the run plan and the native build line (phase-537 W9).
[group("main")]
platform-nuttx: (platform "platform-nuttx")

# book/src/guide/platforms/threadx.md — the ThreadX RISC-V 64 platform page, after its
# starter: the run plan and the native build line (phase-537 W9).
[group("main")]
platform-threadx-riscv64: (platform "platform-threadx-riscv64")

# book/src/guide/platforms/flash-a-real-board.md — Flash a real board, after
# the Nucleo-F767ZI starter: the bench file and the flash plan, no board
# needed (phase-537 W9).
[group("main")]
flash-a-real-board: (platform "flash-a-real-board")

# book/src/guide/build/install-options.md — Working offline; it fails up front
# naming the container runtime it awaits (phase-537 W13 follow-up).
[group("main")]
install-offline: (platform "install-offline")

# book/src/start/install.md — the ROS 2 prerequisite on a bare host; it fails
# up front naming the bare-host runner it awaits (phase-537 W13 follow-up).
[group("main")]
install-ros: (platform "install-ros")

# The same page with `nros flash` onto an attached Nucleo-F767ZI; it fails up
# front naming the board it awaits (phase-537 W9).
[group("main")]
flash-a-real-board-bench: (platform "flash-a-real-board-bench")

# book/src/guide/platforms/zephyr-west-app.md — a Zephyr application built with
# plain west (no workspace, no system.toml), on native_sim.
[group("main")]
zephyr-west-app: (platform "zephyr-west-app")

# The same chapter's C++ node, the same build and run.
[group("main")]
zephyr-west-app-cpp: (platform "zephyr-west-app-cpp")

# The same chapter's flash section: the Nucleo-F767ZI image it builds, booted
# in the board's Renode twin in place of `west flash`.
[group("main")]
zephyr-west-app-flash: (platform "zephyr-west-app-flash")

# The same chapter's Rust application (awaits `nros configure`, phase-530).
[group("main")]
zephyr-west-app-rust: (platform "zephyr-west-app-rust")

# book/src/guide/platforms/px4.md — the reader's own PX4-Autopilot, the module template
# copied out of the SDK root, `make px4_sitl_default`, then a headless SITL boot
# and `nano_ros_app start` (RFC-0115 D4: PX4 SITL has no twin). Awaits a
# user-tier build of the module's nano-ros archives (its registry `awaits`).
[group("main")]
px4: (platform "px4")

# book/src/starters/nucleo-f767zi.md — the Nucleo-F767ZI's Renode twin: the
# starter's setup, scaffold and `nros run` in Renode, heard by `ros2 topic
# echo` over a serial-locator `rmw_zenohd`.
[group("main")]
renode-nucleo: (platform "renode-nucleo")

# The same starter's `nros flash --monitor` onto an attached Nucleo-F767ZI; it
# fails up front naming the board it awaits (phase-537 W13).
[group("main")]
nucleo-f767zi-bench: (platform "nucleo-f767zi-bench")

# phase-517 W4: not a platform chapter, but it rides the same harness. One bare
# `nros setup` for a Linux and a FreeRTOS image, then `nros build --all`.
# book/src/guide/build/install-options.md — "Provision a whole workspace".
[group("main")]
workspace: (platform "workspace")

# phase-517 W11: installation-details.md "Pinning a version" — a project pinned to an
# older release; the installed nros must fetch and run the pinned one.
[group("main")]
pinned: (platform "pinned")

# Issue 2168: the same project with the PUBLISHED release `[tool.nros]` pins
# installed, so a newer front hands off to the project's older pin.
[group("main")]
pinned-newer: (platform "pinned-newer")

# Same probe on a NON-Debian, NON-bash host (issue 0373). The book's install
# path was only ever exercised on ubuntu+bash, which is how an apt-only prereq
# block, a `just` contradiction, an unactionable ROS warning and a zsh-fatal
# glob in activate.sh all survived. Opt-in, not part of any tier — these are
# the shapes to re-run before a release, or after touching activate*/bootstrap.
# The zenoh/ROS-interop track (phase-368 follow-up): the same probe on
# ros:humble with `--rmw zenoh`, running first-node-rust's zenoh-default build
# and replaying the interop page's three terminals — router, nano-ros talker,
# and `ros2 topic echo` proving cross-stack delivery. This is the flow that
# left the default track when its rmw moved to cyclonedds.
[group("main")]
zenoh:
    PROBE_TRACK=zenoh ./scripts/probe/run-bootstrap-probe.sh

[group("main")]
bootstrap-arch:
    PROBE_DISTRO=arch PROBE_IMAGE=archlinux:base-devel \
        ./scripts/probe/run-bootstrap-probe.sh

[group("main")]
bootstrap-fedora:
    PROBE_DISTRO=fedora PROBE_IMAGE=fedora:41 \
        ./scripts/probe/run-bootstrap-probe.sh

# The book claims bash/zsh support for activate.sh; this is what checks it end
# to end (`just check activate-shells` covers the file in isolation).
[group("main")]
bootstrap-zsh:
    PROBE_SHELL=zsh ./scripts/probe/run-bootstrap-probe.sh

# Extract the probe script from the book without running it (drift check —
# fails if the book's tagged blocks or substitution anchors changed shape).
# PROBE_DISTRO selects which `distro=`-tagged blocks are extracted; a distro
# with no block for a tagged step is an error, not a silently shorter probe.
# `track=installed` also extracts the release workflow's steps, so a renamed or
# newly conditional step in release-nros.yml fails here too.
[group("main")]
extract out="tmp/probe-steps.sh" distro="debian" track="quickstart":
    mkdir -p tmp
    PROBE_TRACK={{track}} PROBE_DISTRO={{distro}} PROBE_EXTRACT_ONLY={{out}} ./scripts/probe/run-bootstrap-probe.sh

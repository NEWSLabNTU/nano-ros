set working-directory := '..'

default:
    @just --list --justfile {{source_file()}}

# Clean-system bootstrap probe (issue #204): run the book's documented setup
# steps verbatim in pristine containers and assert the documented outcome —
# BOTH front doors, because they are two different machines:
#   1. the CHECKOUT track (ubuntu:24.04): clone + bootstrap, then the first-node
#      chapter. What everyone does until the first release is cut.
#   2. the INSTALLED track (phase-447 A3): install a release, then
#      first-project.md's scaffold -> build -> run with NO checkout on the host.
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
# second pristine container with nothing else mounted, and run first-project.md.
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

# book/src/getting-started/bare-metal.md — Rust talker on QEMU MPS2-AN385.
[group("main")]
baremetal: (platform "baremetal")

# book/src/getting-started/freertos.md — Rust workspace image on QEMU MPS2-AN385.
[group("main")]
freertos: (platform "freertos")

# book/src/getting-started/threadx.md — the Linux simulator flavour.
[group("main")]
threadx-linux: (platform "threadx-linux")

# book/src/getting-started/threadx.md — the RISC-V 64 QEMU flavour.
[group("main")]
threadx-riscv64: (platform "threadx-riscv64")

# book/src/getting-started/integration-nuttx.md — QEMU Arm `virt`.
[group("main")]
nuttx-arm: (platform "nuttx-arm")

# book/src/getting-started/integration-nuttx.md — QEMU RISC-V `rv-virt`.
[group("main")]
nuttx-riscv: (platform "nuttx-riscv")

# book/src/getting-started/integration-zephyr.md — native_sim.
[group("main")]
zephyr: (platform "zephyr")

# phase-517 W4: not a platform chapter, but it rides the same harness. One bare
# `nros setup` for a Linux and a FreeRTOS image, then `nros build --all`.
# book/src/getting-started/installation.md — "Provision a whole workspace".
[group("main")]
workspace: (platform "workspace")

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

set working-directory := '..'

default:
    @just --list --justfile {{source_file()}}

# Clean-system bootstrap probe (issue #204): run the book's documented setup
# steps verbatim in a pristine ubuntu:24.04 container and assert the
# first-node chapter's documented outcome. ~30-60 min cold (builds the nros
# CLI from source inside the container). Knobs: PROBE_BRANCH, PROBE_CLONE_URL,
# PROBE_IMAGE, PROBE_KEEP=1.
[group("main")]
bootstrap:
    ./scripts/probe/run-bootstrap-probe.sh

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
[group("main")]
extract out="tmp/probe-steps.sh" distro="debian":
    mkdir -p tmp
    PROBE_DISTRO={{distro}} PROBE_EXTRACT_ONLY={{out}} ./scripts/probe/run-bootstrap-probe.sh

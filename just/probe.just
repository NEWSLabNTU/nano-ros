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

# The installed track alone: build the asset `release-nros.yml` would publish
# (its own steps, extracted, on its own `runs-on` image), install it into a
# second pristine container with nothing else mounted, and run first-project.md.
# PROBE_ASSET=<dir> installs a prebuilt asset instead — e.g. a `release nros`
# run's artifact, `gh run download <id> -n nros-linux-x86_64 -D <dir>`.
[group("main")]
installed:
    PROBE_TRACK=installed ./scripts/probe/run-bootstrap-probe.sh

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

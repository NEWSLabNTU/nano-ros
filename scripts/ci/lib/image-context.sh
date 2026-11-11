#!/usr/bin/env bash
# scripts/ci/lib/image-context.sh -- the ONE generator of nano-ros' container
# build contexts (RFC-0127 D1, option B, ruled 2026-11-11; phase-551 W1).
#
# Sourced, never run. It used to be the body of `runner-container.sh`, moved
# here VERBATIM so a second image (the starter, phase-551 W2) can be generated
# from the same resolved index instead of from a copy of it:
#
#   nros_image_resolve              resolve the index into the layer inputs
#                                   (PREREQ_*, PYTHON_*, ROS_*) -- once;
#   nros_runner_context <dir>       write the CI runner's build context;
#   nros_image_host_stage           print the `host` layers (everything up to
#                                   the runner's own user and binary), the
#                                   part a starter image shares with the runner.
#
# The runner's context is held BYTE-IDENTICAL to what this code wrote before
# the move by `check-runner-context-golden`, against
# `ci/docker/runner-context.golden/` (the runner/CI owner's review contract):
# its Dockerfile, entrypoint and every file it COPYs. A starter image gets its
# OWN context directory, so the runner's never gains a file.
#
# Requires REPO_ROOT. Error prefixes still say `runner-container`, the only
# caller that existed when they were written.

# shellcheck disable=SC2034  # the resolved values are this library's output

nros_image_resolve() {
# --- the system-package layer ------------------------------------------------
#
# THIS is what the image is for, and it is the one thing a volume cannot hold.
# Everything else the runner needs — rustup, the SDKs, the nano-ros checkout —
# installs as the `runner` user and therefore persists in a volume. System
# packages need root, and the container runs `--cap-drop ALL
# --security-opt no-new-privileges` with a non-root user, so `sudo apt` inside a
# running container cannot work even where the package list is right.
#
# So the split is a property of WHO CAN INSTALL IT, not a preference: root-only
# things are baked, user-writable things persist. That also gives the image the
# stability the persist design wants — it changes when the prereq list changes,
# not when nano-ros code or an SDK version does.
#
# The LIST is resolved from `nros-sdk-index.toml` at generation time, through
# the same `prereq-packages.py` the workflows use (`check-workflow-indexed-apt`
# requires it there). A hand-written second list beside the index is the drift
# this repo has already paid for three times — issues 0833, 0500, 0610 — and
# `runner-provision.sh` refuses to be one for exactly that reason.
# `make` and `ninja` are deliberately NOT here, and the check-one-producer gate
# is what says so: the index declares both as TOOLS (`nros setup --tool make`),
# so apt-installing them as well is two producers for one prefix — issue 0500,
# where the store accumulates, prefixes resolve newest-first, both paths print
# success, and the stale one shadows the pin that was just installed. They come
# from the store with everything else the provisioning step fetches.
# The list is what `nros setup` REPORTED MISSING on a real bootstrap of this
# image, not a guess about what a build needs. Its `[MISSING]` lines are
# non-fatal and scroll past, which is precisely how a runner ends up one apt
# package short of a label it claims.
PREREQ_KEYS=(cmake unzip curl zstd python3-dev python3-venv
             python3-pip clang libclang-dev libglib2-dev libpixman-dev
             libgcrypt-dev socat genromfs kconfig-frontends libmbedtls
             aria2 doxygen graphviz libz3 gnu-parallel wget qemu-system-misc file
             qemu-system-aarch64)
if ! PREREQ_PACKAGES="$(python3 "$REPO_ROOT/scripts/sdk/prereq-packages.py" \
        --manager apt "${PREREQ_KEYS[@]}")"; then
    echo "runner-container: could not resolve the prereq packages from the index." >&2
    echo "  The Dockerfile is GENERATED from nros-sdk-index.toml — writing it" >&2
    echo "  with a guessed list would create the second source of truth this" >&2
    echo "  script exists to avoid. Fix the index, or the key names above." >&2
    exit 1
fi

# --- the `[python.*]` layer --------------------------------------------------
#
# `[prereq.*]` was only half of what the index declares, and the other half
# never reached the image: tier-2 nightly died on `ModuleNotFoundError: No
# module named 'catkin_pkg'` eleven minutes into a Zephyr build, because
# `msg2idl.py` — upstream's, from the `rosidl` clone `[rmw.cyclonedds]`
# provisions — imports `rosidl_adapter.cli`, and `cli` imports catkin_pkg, yaml
# and em (issues 1457, 1482).
#
# THE CYCLONE MSG->IDL ROAD DOES NOT NEED A ROS INSTALL. Issue 0368 / phase-327
# created the `[python.*]` layer precisely so it works without one. The message
# PACKAGES are a different matter: they come from the ROS install (the ROS layer
# below), and AMENT_PREFIX_PATH points at it (issue 2344).
#
# WHY THE IMAGE AND NOT THE HOST. The container runs `--cap-drop ALL
# --security-opt no-new-privileges` as a non-root user, so nothing inside it can
# install a system package; and `runner-provision.sh` never sudoes and never
# installs one either, by its own rule. A python module that must be importable
# by the AMBIENT interpreter — the one cmake and ninja invoke — therefore has
# exactly one place it can come from, and this is it. Installing it on the host
# by hand produces a runner nobody can account for and that no fresh container
# reproduces.
#
# WHICH ENTRIES — DERIVED. `python-packages.py` takes the `[python.*]` entries
# with no `check = { cmd = … }`, i.e. the ones the index probes by IMPORTING
# their module in the host's python3. An entry with a command (`west`,
# `clang-format`, `colcon`) is an executable some provisioning verb installs
# into a place the container already persists, and baking a second copy is issue
# 0500's shape — for `west`, worse: `runner-doctor.sh` probes `command -v west`,
# so an image-provided copy would make `nros-sdk-zephyr` true by construction.
#
# WHY THE apt/pip SPLIT IS NOT DECIDED HERE. Issue 1481's resolver asks apt for
# a candidate before preferring apt, and the host generating this Dockerfile is
# the wrong host to ask: measured, a workstation with the ROS 2 apt repo reports
# `python3-catkin-pkg` candidate 1.1.0-101, `ubuntu:22.04` reports 0.4.24-2 from
# universe, and a host with neither reports nothing. Encoding any of those here
# would make the image's contents depend on who ran the generator. So the
# DECLARATIONS are read from the index here and the MEASUREMENT happens in the
# `RUN` below, against that image's own apt.
#
# The self-test runs on the normal path rather than behind a flag, because a
# negative control nobody runs decays into a comment.
if ! python3 "$REPO_ROOT/scripts/sdk/python-packages.py" --self-test; then
    echo "runner-container: python-packages.py fails its own self-test — refusing" >&2
    echo "  to generate a Dockerfile from a resolver that cannot resolve." >&2
    exit 1
fi
if ! PYTHON_LAYER="$(python3 "$REPO_ROOT/scripts/sdk/python-packages.py" --emit json)"; then
    echo "runner-container: could not read the [python.*] layer from the index." >&2
    echo "  Same rule as the prereq list above — the Dockerfile is GENERATED" >&2
    echo "  from nros-sdk-index.toml, and a hand-written package list here is" >&2
    echo "  the second source of truth this script exists to avoid." >&2
    exit 1
fi
if ! PYTHON_KEYS="$(python3 "$REPO_ROOT/scripts/sdk/python-packages.py" --emit keys)"; then
    echo "runner-container: could not derive the [python.*] key set." >&2
    exit 1
fi
# --- the ROS layer: the zenoh router + the four interface packages cli-tests reads
#
# phase-489 W2 added `ros-interface-packages` (std_msgs, geometry_msgs,
# sensor_msgs, example_interfaces) so `check-cli-tests` can run here: its
# rosidl parity tests run `--ignored` once `/opt/ros/$ROS_DISTRO/share` exists
# and FAIL on an absent package. The paragraphs below describe the router.
#
# Issue 2344: since issue 2278 a cmake configure takes a message package only
# from the ament install, never from nano-ros's bundled copy ("the msgs must
# respect those from ROS; bundled msgs only work around the internal dep
# issues", user ruling 2026-11-07). So the image installs EVERY interface
# package a build here names (the whole `[prereq.ros-interface-packages]`
# list, which since 2278 is every package the CLI bundles), and sets
# AMENT_PREFIX_PATH to the ROS prefix so they are discoverable. A job's
# `source ./activate.sh` sources the same prefix's setup script; the ENV makes
# a shell that does not (a `docker exec`, a step that skips it) agree with it.
#
#
# Issue 1695. Tier 2 holds zenoh cells, and since issue 1670 a lane that runs
# them refuses to start without a router (`ci::_require-lane-router`), because
# without one every zenoh cell SKIPS and the lane verifies nothing while
# reading green. The router is ROS's own `rmw_zenohd` (RFC-0075): it ships in
# `rmw_zenoh_cpp` and links the same `libzenohc.so` a ROS node does, so it
# cannot drift from the RMW the way a vendored router did (issue 0609). There
# is no other place it may come from -- never a vendored copy, never a host
# install (issues 1457/1482: nothing inside this container can install a
# system package, and nothing outside it is reproducible).
#
# THE PACKAGE comes from the index, `[prereq.ros-rmw-zenoh-cpp]`, through the
# same resolver as the prereq layer; its apt name is DERIVED from the distro
# (`ros-<distro>-rmw-zenoh-cpp`), so the distro is the one input and it is
# named once, below. THE REPOSITORY is `scripts/sdk/ros2-apt-source.sh`, the
# one spelling of packages.ros.org the distrobox setup also uses.
#
# ROS_DISTRO goes into the image ENV and is the whole of the environment this
# layer adds: it is the third step of `nros_zenohd_bin` (scripts/dev/zenohd.sh),
# the resolver the lane gate, the harness and `runner-doctor.sh nros-ros2`
# share. AMENT_PREFIX_PATH is the other half (issue 2344, above): the ROS
# prefix's message packages are what every build on this runner resolves.
#
# THE REPOSITORY GOES IN BEFORE THE PYTHON LAYER, the package after it -- and
# that order was MEASURED, not chosen. The python layer measures the apt
# candidates of the image it runs in (issue 1481); with only jammy universe
# behind it, it installs `python3-catkin-pkg` 0.4.24-2, and the router's own
# dependency chain then pulls packages.ros.org's `python3-catkin-pkg-modules`
# 1.1.1, which ships the same `catkin_pkg/__init__.py`: dpkg refuses the
# overwrite and the image does not build. With the repository already present,
# the layer resolves catkin_pkg from packages.ros.org -- which is the image's
# own apt answering, exactly as 1481 designed it -- and the two agree.
ROS_DISTRO_IMAGE="${NROS_RUNNER_ROS_DISTRO:-humble}"
ROS_PREREQ_KEYS=(ros-rmw-zenoh-cpp ros-interface-packages)
if ! ROS_PACKAGES="$(ROS_DISTRO="$ROS_DISTRO_IMAGE" python3 \
        "$REPO_ROOT/scripts/sdk/prereq-packages.py" --manager apt "${ROS_PREREQ_KEYS[@]}")" \
        || [ -z "$ROS_PACKAGES" ]; then
    echo "runner-container: could not resolve ${ROS_PREREQ_KEYS[*]} from the index for" >&2
    echo "  ROS_DISTRO=$ROS_DISTRO_IMAGE. Without it every tier-2 run stops at" >&2
    echo "  ci::_require-lane-router (issue 1695) -- fix the index, not this list." >&2
    exit 1
fi
}

# Copy the files the `host` layers COPY into <ctx>.
nros_image_copy_inputs() {
    local ctx="$1"
cp "$REPO_ROOT/scripts/sdk/ros2-apt-source.sh" "$ctx/ros2-apt-source.sh"
cp "$REPO_ROOT/scripts/dev/zenohd.sh" "$ctx/zenohd.sh"

printf '%s\n' "$PYTHON_LAYER" > "$ctx/nros-python-layer.json"
cp "$REPO_ROOT/scripts/sdk/python-packages.py" "$ctx/python-packages.py"
cp "$REPO_ROOT/scripts/lib/index_packages.py" "$ctx/index_packages.py"

# The image provisions THROUGH `runner-provision.sh`, not through a second list
# of apt packages. A runner and a contributor must provision the same way or the
# runner's toolchain becomes a thing nobody can account for — the reason that
# script exists at all.
}

# The `host` layers, verbatim: everything after `FROM ubuntu:22.04` up to the
# runner's own user. Quoted heredoc, so the Dockerfile's own $VARs survive;
# the @PLACEHOLDERS@ are filled by nros_image_substitute.
nros_image_host_stage() {
    cat <<'DOCKEREOF'

# The runner refuses to run as root, and running as root would also defeat the
# isolation this image exists for.
ARG RUNNER_UID=1001
ARG RUNNER_VERSION=2.337.0

ENV DEBIAN_FRONTEND=noninteractive
RUN apt-get update && apt-get install -y --no-install-recommends \
        ca-certificates curl git sudo jq python3 build-essential pkg-config \
    && rm -rf /var/lib/apt/lists/*

# Resolved from nros-sdk-index.toml by scripts/sdk/prereq-packages.py --manager apt
# Keys: @PREREQ_KEYS@
RUN apt-get update && apt-get install -y --no-install-recommends \
        @PREREQ_PACKAGES@ \
    && rm -rf /var/lib/apt/lists/*

# The ROS 2 apt repository, BEFORE the python layer so that layer measures the
# apt this image will actually have (issue 1695 -- see the ROS layer below).
COPY ros2-apt-source.sh /opt/nros-ros/ros2-apt-source.sh
RUN bash /opt/nros-ros/ros2-apt-source.sh && rm -rf /var/lib/apt/lists/*

# The `[python.*]` layer — the modules the ambient interpreter must be able to
# import, RESOLVED HERE rather than on the machine that generated this file.
#
# `--emit plan` writes the decision into the build log, so which packages came
# from apt and which from pip — and WHY each — is a record of this image rather
# than a claim about it. `--verify` imports them in the interpreter that will
# run them: a package that installs and cannot be imported fails now, not
# eleven minutes into a Zephyr build in a nightly lane.
#
# Keys: @PYTHON_KEYS@
COPY nros-python-layer.json python-packages.py index_packages.py /opt/nros-python/
RUN apt-get update \
    && python3 /opt/nros-python/python-packages.py \
         --resolve /opt/nros-python/nros-python-layer.json --emit plan \
    && nros_apt="$(python3 /opt/nros-python/python-packages.py \
         --resolve /opt/nros-python/nros-python-layer.json --emit apt)" \
    && nros_pip="$(python3 /opt/nros-python/python-packages.py \
         --resolve /opt/nros-python/nros-python-layer.json --emit pip)" \
    && if [ -n "$nros_apt" ]; then \
         apt-get install -y --no-install-recommends $nros_apt; fi \
    && if [ -n "$nros_pip" ]; then pip3 install --no-cache-dir $nros_pip; fi \
    && python3 /opt/nros-python/python-packages.py \
         --resolve /opt/nros-python/nros-python-layer.json --verify \
    && rm -rf /var/lib/apt/lists/*

# The ROS layer -- the zenoh router and nothing else of ROS (issue 1695).
# Resolved from nros-sdk-index.toml by scripts/sdk/prereq-packages.py --manager apt
# Keys: @ROS_KEYS@ (ROS_DISTRO=@ROS_DISTRO@)
#
# The second RUN is the acceptance this layer exists for, checked at build time
# with the repo's OWN resolver and launcher (scripts/dev/zenohd.sh, copied in):
# `nros_zenohd_bin` must resolve the router from ROS_DISTRO alone, and
# `nros_router_exec` must still be running it 3 s later (`timeout`'s 124) --
# resolving is not running (issue 0774). A package that put its router
# elsewhere, or a router that cannot start here, fails the BUILD, not a lane
# that stops at its first step on the runner.
RUN apt-get update && apt-get install -y --no-install-recommends \
        @ROS_PACKAGES@ \
    && rm -rf /var/lib/apt/lists/*
ENV ROS_DISTRO=@ROS_DISTRO@
ENV AMENT_PREFIX_PATH=/opt/ros/@ROS_DISTRO@
COPY zenohd.sh /opt/nros-ros/zenohd.sh
RUN bash -c '. /opt/nros-ros/zenohd.sh && nros_zenohd_bin \
      && rc=0 \
      && { timeout 3 bash -c ". /opt/nros-ros/zenohd.sh && nros_router_exec tcp/127.0.0.1:17447" \
           || rc=$?; } \
      && echo "router after 3 s: timeout rc=$rc (124 = still running)" && test "$rc" -eq 124'

DOCKEREOF
}

# The runner's own layers: its user, the Actions runner binary, the mount
# points, the entrypoint.
_nros_image_runner_tail() {
    cat <<'DOCKEREOF'
RUN useradd -m -u ${RUNNER_UID} -s /bin/bash runner
WORKDIR /home/runner

# v2.329.0 or later is REQUIRED to configure or re-register since GitHub's
# 2026-06-12 change, and job execution needs staying inside a moving 30-day
# window. Pinned so the image is reproducible; bump it deliberately.
#
# Bump it to what GitHub PUSHES, not merely to a version it still registers
# (issue 1365, measured 2026-10-15/16 in runner-loop.log): a 2.329.0
# container registers and reads `Listening for Jobs`, and claims a job only
# after GitHub sends it a self-update to 2.337.0. Every job this loop ran
# that day started within 3 s of the updated listener coming up. A container
# that got no update message (05:20Z, and from 02:44Z the next day)
# never claimed one, while matching jobs sat queued. An update in flight
# is also a window where stopping the container loses the registration.
RUN curl -fsSL -o actions-runner.tar.gz \
      "https://github.com/actions/runner/releases/download/v${RUNNER_VERSION}/actions-runner-linux-x64-${RUNNER_VERSION}.tar.gz" \
    && tar xzf actions-runner.tar.gz && rm actions-runner.tar.gz \
    && ./bin/installdependencies.sh \
    && mkdir -p /home/runner/.cache/sccache /home/runner/_work /home/runner/.cargo \
         /home/runner/.nros /home/runner/.rustup /home/runner/.local \
         /home/runner/.cmake /home/runner/src \
    && chown -R runner:runner /home/runner

# Issue 1755 — every `-v` mount point of `runner-container.sh --run` exists in the image,
# owned by `runner`. A mount point the image lacks is created by the engine as
# ROOT, and so is every missing parent: `nros-runner-sccache` at
# `.cache/sccache` left `/home/runner/.cache` root-owned in every container, so
# `nros::main!`'s model cache and Miri's sysroot could not create their
# directories beside it (measured: `mkdir ~/.cache/nano-ros` -> Permission
# denied as uid 1001). A NEW named volume also takes its root's ownership from
# the image's directory, so the same list covers a first `--run` on a fresh
# host. Keep it in step with the `-v` list; `runner-doctor.sh` checks the
# result from inside the running container.

COPY --chown=runner:runner entrypoint.sh /home/runner/entrypoint.sh
RUN chmod +x /home/runner/entrypoint.sh
USER runner
ENTRYPOINT ["/home/runner/entrypoint.sh"]
DOCKEREOF
}

# The Dockerfile heredoc is QUOTED so its own $VAR references survive verbatim;
# the two generated values are substituted here instead.
nros_image_substitute() {
    local ctx
    ctx="$(dirname "$1")"
    [ "$1" = "$ctx/Dockerfile" ] || { echo "nros_image_substitute: expects <ctx>/Dockerfile" >&2; return 2; }
python3 - "$ctx/Dockerfile" "$PREREQ_PACKAGES" "${PREREQ_KEYS[*]}" "$PYTHON_KEYS" \
    "$ROS_PACKAGES" "${ROS_PREREQ_KEYS[*]}" "$ROS_DISTRO_IMAGE" <<'SUBEOF'
import sys
path, packages, keys, python_keys, ros_packages, ros_keys, ros_distro = sys.argv[1:8]
text = open(path).read()
text = text.replace("@PREREQ_PACKAGES@", " \\\n        ".join(packages.split()))
text = text.replace("@PREREQ_KEYS@", keys)
# The python layer substitutes only its KEY NAMES — the packages are resolved
# inside the image, which is the whole point of that layer.
text = text.replace("@PYTHON_KEYS@", python_keys)
# Issue 1695 -- the ROS layer: its packages, its keys, and the ONE distro both
# the derived apt name and the image's ROS_DISTRO come from.
text = text.replace("@ROS_PACKAGES@", " \\\n        ".join(ros_packages.split()))
text = text.replace("@ROS_KEYS@", ros_keys)
text = text.replace("@ROS_DISTRO@", ros_distro)
open(path, "w").write(text)
SUBEOF
}

_nros_runner_entrypoint() {
    local ctx="$1"
cat > "$ctx/entrypoint.sh" <<'ENTRYEOF'
#!/usr/bin/env bash
# Generated by scripts/ci/runner-container.sh.
set -euo pipefail
: "${NROS_RUNNER_LABELS:?labels required}"
: "${GH_REPO:?GH_REPO required, e.g. NEWSLabNTU/nano-ros}"
: "${RUNNER_TOKEN:?RUNNER_TOKEN required — a SHORT-LIVED registration token}"
# The checkout the label gate reads. A VOLUME, not a layer: the image carries no
# nano-ros source, so code changes never rebuild it, and `runner-bootstrap`
# refreshes this with a fetch rather than a 2.8 GB clone per start.
# A SUBDIRECTORY of the volume, not the volume's own mountpoint. The store is
# bind-backed by a host directory owned by the human who created it, and an ACL
# grants this UID access — but git's `safe.directory` check is about OWNERSHIP,
# not access, so a repo AT the mountpoint is refused with "detected dubious
# ownership" and no amount of ACL fixes it. A directory the container creates
# inside the mount is owned by the container's UID, so the clone below it is
# ordinary. The alternatives are worse: chown needs root on the host, and
# `safe.directory` in ~/.gitconfig dies with the writable layer every job.
NROS_SRC="${NROS_SRC:-/home/runner/src/nano-ros}"

# The store's own tools go on PATH, here, BEFORE anything reads them.
#
# `just`, `west` and the cargo-installed tools live in the persistent volumes —
# `~/.local/bin` and `~/.cargo/bin` — because they are installable as the runner
# user (the rule the `.local`, `.rustup` and `.cmake` stores all follow). The
# image does not put those on PATH, so without this line the label gate asks a
# correctly provisioned store whether it has `west` and is told no.
#
# It matters twice over: `run.sh` inherits this environment and hands it to
# every job, so a workflow's `run: just …` resolves the same tools the gate
# just verified. A runner whose PATH differs from the one its labels were
# checked against is a runner whose labels were checked against nothing.
export PATH="$HOME/.cargo/bin:$HOME/.local/bin:$PATH"

# --ephemeral: one job, then the registration is spent. A job cannot leave state
# for the next one, which is also what stops the orphan/disk rot a long-lived
# runner accumulates (the design doc records 71 orphaned processes, oldest 10
# days, on a persistent runner).
#
# THE LABEL GATE, and it runs BEFORE config.sh on purpose. A runner registers
# its labels and then wins jobs that match them; if the toolchain behind a label
# is absent, it wins a job it cannot do and the failure surfaces as that job's
# error, not as this container's. Measured: a first container with an empty SDK
# store connected and took `L3 (cross build + link)` three seconds later.
#
# So the order is: prove the labels, THEN become visible. An unprovisioned
# runner is better absent than lying.
#
# Exit 78 (EX_CONFIG) rather than 1, because the supervision loop must tell
# "this container finished its job" from "this container can never work". The
# first is a restart; the second is a stop. Without the distinction one bad SDK
# fetch becomes a restart storm against the download it is failing on.
if [ "${NROS_RUNNER_SKIP_DOCTOR:-0}" != "1" ]; then
    if [ ! -x "$NROS_SRC/scripts/ci/runner-doctor.sh" ]; then
        echo "entrypoint: no checkout at $NROS_SRC — run the one-shot bootstrap first:" >&2
        echo "    just runner-bootstrap ${NROS_RUNNER_LABELS}" >&2
        exit 78
    fi
    echo "entrypoint: verifying labels before registering"
    if ! ( cd "$NROS_SRC" && ./scripts/ci/runner-doctor.sh "${NROS_RUNNER_LABELS}" ); then
        echo "entrypoint: labels are NOT true — refusing to register." >&2
        echo "  Provision the store, then start again:" >&2
        echo "    just runner-bootstrap ${NROS_RUNNER_LABELS}" >&2
        exit 78
    fi
fi

./config.sh \
    --url "https://github.com/${GH_REPO}" \
    --token "${RUNNER_TOKEN}" \
    --labels "${NROS_RUNNER_LABELS}" \
    --name "${RUNNER_NAME:-$(hostname)}" \
    --work /home/runner/_work \
    --ephemeral --unattended --replace
exec ./run.sh
ENTRYEOF
chmod +x "$ctx/entrypoint.sh"
}

# The CI runner's build context in <ctx>. The bytes it writes are the ones
# `runner-container.sh` wrote before phase-551 W1.
nros_runner_context() {
    local ctx="$1"
    mkdir -p "$ctx"
    nros_image_copy_inputs "$ctx"
    {
        printf '%s\n' '# Generated by scripts/ci/runner-container.sh — edit that, not this.' \
            'FROM ubuntu:22.04'
        nros_image_host_stage
        _nros_image_runner_tail
    } > "$ctx/Dockerfile"
    nros_image_substitute "$ctx/Dockerfile"
    _nros_runner_entrypoint "$ctx"
}

# --- the starter image (phase-551 W2, RFC-0127 D1/D2) --------------------------
#
# ONE multi-stage Dockerfile with three targets (option B, ruled 2026-11-11):
#
#   host            the runner's `host` layers, emitted by the same
#                   nros_image_host_stage, so the two images share them
#                   (identical instructions over identical COPY inputs hit
#                   the same layer cache);
#   starter         FROM host: the released `nros` through `install.sh` (the
#                   user's own road), a baked `nros setup`, a minimal Rust,
#                   the first project's crates pre-fetched;
#   starter-zephyr  FROM starter: the Zephyr SDK and workspace.
#
# Its context is its OWN directory, never the runner's (the runner's context
# gains no file; `check-runner-context-golden`).
#
# The ruled contents (user ruling 2026-11-12, RFC-0127 Rulings), each an ARG
# a derived build can still override:
#   Q2 ROS edition   -- Humble on 22.04, which `host` already is (the release
#                       asset's libpython3.10 floor);
#   Q3 contents      -- NROS_STARTER_BOARDS: linux, mps2-an385-baremetal,
#                       mps2-an385-freertos; NROS_STARTER_RMW: zenoh;
#                       Rust in (minimal profile), no C/C++-only variant;
#   Q8 lab sources   -- the crates of the book's first projects only
#                       (NROS_STARTER_PROJECTS), no course repository.
# Nothing here publishes or tags an image: the release job does (RFC-0127
# D6), never a local build.

_nros_image_starter_stages() {
    cat <<'DOCKEREOF'

# ---------------------------------------------------------------------------
# starter -- RFC-0127 D2. The ARGs below are the ruled contents (Q2/Q3/Q8,
# 2026-11-12), changeable with --build-arg.
FROM host AS starter
ARG NROS_VERSION=latest
ARG NROS_STARTER_BOARDS="linux mps2-an385-baremetal mps2-an385-freertos"
ARG NROS_STARTER_RMW=zenoh
ARG NROS_STARTER_UID=1000
# What this image carries, as data a derived image's probe reads back
# (scripts/probe/verify-starter-recipe.sh) rather than assuming this
# checkout's defaults: a starter built with other --build-args is checked
# for what it actually holds.
LABEL org.nano-ros.starter.boards="${NROS_STARTER_BOARDS}" \
      org.nano-ros.starter.rmw="${NROS_STARTER_RMW}"

# RFC-0127 D5 guarantee 3 -- fixed paths a derived image may rely on.
ENV NROS_HOME=/opt/nros \
    RUSTUP_HOME=/opt/rustup \
    CARGO_HOME=/opt/cargo
ENV PATH=/opt/nros/bin:/opt/cargo/bin:$PATH
RUN useradd -m -u ${NROS_STARTER_UID} -s /bin/bash nros \
    && mkdir -p /opt/nros /opt/rustup /opt/cargo /work \
    && chown nros:nros /opt/nros /opt/rustup /opt/cargo /work
USER nros
WORKDIR /work

# The released `nros`, through the script a user runs (`install.sh`), so the
# store this image bakes is the one that release's `nros setup` resolves
# (RFC-0127 D5 guarantee 2).
COPY --chown=nros:nros install.sh /opt/nros-install/install.sh
RUN NROS_INSTALL_VERSION="${NROS_VERSION}" sh /opt/nros-install/install.sh \
    && nros --version

# Rust stays in rustup's own homes (maintainer ruling, 2026-11-06); the
# minimal profile, and the first project pulls its own pinned toolchain below.
RUN curl -fsSL https://sh.rustup.rs | sh -s -- -y --no-modify-path --profile minimal \
        --default-toolchain stable \
    && rustup target add thumbv7m-none-eabi

RUN for b in ${NROS_STARTER_BOARDS}; do \
        nros setup "$b" --rmw "${NROS_STARTER_RMW}" || exit 1; \
    done

# The projects the book teaches first, each scaffolded and built once, then
# removed: their crates and pinned toolchain stay in CARGO_HOME / RUSTUP_HOME,
# so a seat behind a NAT builds them offline (RFC-0127 D2, Q8's
# recommendation). One `<board>:<lang>` per project: the C road reaches crates
# the Rust road does not (nros-c's own dependencies), so warming one road
# leaves the other online-only -- measured on the first local build.
ARG NROS_STARTER_PROJECTS="linux:rust mps2-an385-freertos:c"
RUN export NROS_BUILD_ROOT=/tmp/nros-build; for p in ${NROS_STARTER_PROJECTS}; do \
        b="${p%%:*}"; l="${p#*:}"; \
        cd /tmp && rm -rf starter_ws \
        && nros new starter_ws --board "$b" --template talker --lang "$l" \
            --rmw "${NROS_STARTER_RMW}" \
        && (cd starter_ws && nros build) || exit 1; \
    done; root="$(nros locate nano-ros)" \
    && rm -rf /tmp/starter_ws /tmp/nros-build "${root:?}/build"

# Neither step keeps its build output. The probes' caches (3-4 GB: the
# metadata, sizes and Corrosion cargo dirs) go to NROS_BUILD_ROOT, and the
# release tree's own `build/` is removed too, because the sizes probe writes
# there whatever NROS_BUILD_ROOT says. What makes the offline build work is
# CARGO_HOME's registry (~100 MB), not those caches; a seat pays the probe
# compile once, on its first build.
#
# The offline claim, proved at build time rather than asserted: the same
# projects again with the network removed from this step. NROS_OFFLINE is the
# spelling that reaches every cargo (`nros build --offline` misses the
# metadata probe, issue 2444).
RUN --network=none export NROS_BUILD_ROOT=/tmp/nros-build; for p in ${NROS_STARTER_PROJECTS}; do \
        b="${p%%:*}"; l="${p#*:}"; \
        cd /tmp && rm -rf starter_ws \
        && nros new starter_ws --board "$b" --template talker --lang "$l" \
            --rmw "${NROS_STARTER_RMW}" \
        && (cd starter_ws && NROS_OFFLINE=1 nros build) || exit 1; \
    done; root="$(nros locate nano-ros)" \
    && rm -rf /tmp/starter_ws /tmp/nros-build "${root:?}/build"

# ---------------------------------------------------------------------------
# starter-zephyr -- RFC-0127 D3. Built only when asked (`--target
# starter-zephyr`): on dispatch, never every release (Q4, ruled 2026-11-12).
FROM starter AS starter-zephyr
ARG NROS_STARTER_ZEPHYR_BOARD=zephyr
RUN nros setup "${NROS_STARTER_ZEPHYR_BOARD}" --rmw "${NROS_STARTER_RMW:-zenoh}"
DOCKEREOF
}

# The starter's build context in <ctx>: its own directory, holding the host
# layers' COPY inputs plus `install.sh`.
nros_starter_context() {
    local ctx="$1"
    mkdir -p "$ctx"
    nros_image_copy_inputs "$ctx"
    cp "$REPO_ROOT/scripts/install.sh" "$ctx/install.sh"
    {
        printf '%s\n' '# Generated by scripts/ci/image-context.sh starter — edit scripts/ci/lib/image-context.sh, not this.' \
            '# RFC-0127 option B: one generator; targets host, starter, starter-zephyr.' \
            'FROM ubuntu:22.04 AS host'
        nros_image_host_stage
        _nros_image_starter_stages
    } > "$ctx/Dockerfile"
    nros_image_substitute "$ctx/Dockerfile"
}

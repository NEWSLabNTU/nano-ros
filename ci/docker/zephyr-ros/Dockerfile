# nano-ros Zephyr CI toolchain image.
#
# Bakes the *stable, heavy* provisioning the zephyr-dual-line lane re-installs
# every run — ROS 2 Humble (base image), the ~1 GB Zephyr SDK (with its CMake
# package registered), and the host tools (cmake/ninja/dtc/gperf, uv, just,
# rustup + cross targets). NOT baked: the west workspace (per-line, manifest-
# pinned — stays a dynamic `west update`) and the `nros` CLI (released binary,
# version-pinned, fetched per-run via install.sh). See
# docs/development/ci-conventions.md + the zephyr image design (Phase 196.3).
#
# Build/publish via the `zephyr` job in .github/workflows/images.yml →
#   ghcr.io/newslabntu/nano-ros-zephyr-ci:humble-sdk<ver>-r<N>
# Tag encodes the inputs (ROS distro + SDK version) + a `-rN` content revision;
# bump `-rN` when the Dockerfile content changes without an SDK/distro bump.
FROM ros:humble-ros-base

# ---- ROS 2 environment, baked (issue 0866) --------------------------------
#
# The SAME gap ci-base has, and it does NOT inherit the fix: this image is
# `FROM ros:humble-ros-base` directly, not `FROM` ci-base — despite ci-base's
# own header saying "the Zephyr CI image is FROM this + the Zephyr SDK layer".
# That comment is stale, so a reader who fixes one image and trusts it has
# fixed neither.
#
# GitHub Actions runs a job's steps directly and does NOT use the image's
# ENTRYPOINT, so `/ros_entrypoint.sh` never sources `setup.bash` and every
# process runs with `AMENT_PREFIX_PATH` unset and ROS's site-packages off
# `PYTHONPATH`. `ROS_DISTRO=humble` IS set by the base image, so probes that
# read only that answered YES while nothing else was wired.
#
# Full reasoning, and the measurement the values come from, in
# ci/docker/ci-base/Dockerfile — kept in ONE place rather than copied, but the
# ENV must be repeated here because Docker has no include.
#
# The `-rN` tag revision is deliberately NOT bumped for this change. The
# convention says to bump when content moves, but the image is only BUILT by a
# push to `main`, so a new tag does not exist until after the merge — and the
# lanes referencing it fail to pull in the meantime, which is exactly what
# happened when this was first written as `-r3` ("Initialize containers"
# failure on every zephyr job of the very PR carrying the fix). Overwriting the
# existing tag in place is what ci-base already does with `humble`, and it is
# the only option that keeps the lanes runnable across the merge.
ENV AMENT_PREFIX_PATH=/opt/ros/humble \
    LD_LIBRARY_PATH=/opt/ros/humble/lib/x86_64-linux-gnu:/opt/ros/humble/lib \
    PYTHONPATH=/opt/ros/humble/lib/python3.10/site-packages:/opt/ros/humble/local/lib/python3.10/dist-packages \
    ROS_VERSION=2 \
    ROS_PYTHON_VERSION=3 \
    ROS_LOCALHOST_ONLY=0
ENV PATH=/opt/ros/humble/bin:$PATH

# TWO SDKs, because the SDK is a PER-LINE FACT — `scripts/zephyr/setup.sh` says
# so in as many words, and `nros-sdk-index.toml` carries a second entry
# (`[tool.zephyr-sdk-1-0-1]`) for exactly this reason: 3.7's `zephyr/SDK_VERSION`
# demands 0.16.8, 4.4's demands 1.0.1, and `FindZephyr-sdk.cmake` refuses
# anything older than the tree asks for.
#
# One baked SDK could not serve both. 0.17.4 satisfies 3.7 (newer-satisfies-
# older, and 3.7 sets no upper bound) but 4.4 asks for `find_package(Zephyr-sdk
# 1.0)` and correctly rejects it:
#
#   Could not find a configuration file for package "Zephyr-sdk" that is
#   compatible with requested version "1.0".
#     considered: /opt/zephyr-sdk/cmake/Zephyr-sdkConfig.cmake, version: 0.17.4
#
# That killed every `zephyr 4.4 /*` nightly cell. The comment below used to say
# 4.4 "must fall through to its own west-managed SDK" — but every CI job runs
# `just zephyr setup --skip-sdk`, so there was no west-managed SDK to fall
# through to, and the fallback the design relied on did not exist.
ARG ZEPHYR_SDK_VERSION=0.17.4
ARG ZEPHYR_SDK_VERSION_44=1.0.1
# Zephyr SDK toolchains the dual-line + aemv8r reference need (setup.sh
# DEFAULT_TARGETS + the phase-117 aarch64 target). Keep in sync with
# scripts/zephyr/setup.sh.
ARG SDK_TARGETS="x86_64-zephyr-elf arm-zephyr-eabi aarch64-zephyr-elf"

ENV DEBIAN_FRONTEND=noninteractive

# Zephyr + nano-ros host build deps (the apt set scripts/zephyr/setup.sh expects
# + curl/zstd/git for the per-run install.sh + nros source provisioning).
RUN apt-get update && apt-get install -y --no-install-recommends \
        cmake ninja-build device-tree-compiler gperf aria2 \
        git curl ca-certificates zstd xz-utils file wget \
        python3 python3-pip python3-venv build-essential \
        ros-humble-example-interfaces \
    && rm -rf /var/lib/apt/lists/*
# `ros-humble-example-interfaces` (issue #59): NOT in `ros:humble-ros-base`
# (only common_interfaces/action_msgs/unique_identifier_msgs are), but the
# rust service/action examples (AddTwoInts srv, Fibonacci action) depend on it.
# `nros sync` reads its .srv/.action defs from /opt/ros/humble/share to
# codegen `generated/example_interfaces`; without the pkg it silently skips
# codegen and cargo then fails `no matching package named example_interfaces`.
# Mirrors ci/docker/ci-base/Dockerfile (which is also FROM ros:humble-ros-base).

# Zephyr SDK — pinned, with the CMake package registered (`-c`) IN the image so
# `find_package(Zephyr-sdk)` resolves without a per-run setup step. (A file-only
# cache that skips registration is exactly what broke Phase 196.3's first cut.)
RUN curl -fsSL -o /tmp/sdk.tar.xz \
        "https://github.com/zephyrproject-rtos/sdk-ng/releases/download/v${ZEPHYR_SDK_VERSION}/zephyr-sdk-${ZEPHYR_SDK_VERSION}_linux-x86_64.tar.xz" \
    && mkdir -p /opt/zephyr-sdk \
    && tar xf /tmp/sdk.tar.xz -C /opt/zephyr-sdk --strip-components=1 \
    # setup.sh wants one `-t` per toolchain (mirrors scripts/zephyr/setup.sh).
    && sdk_args="" && for t in ${SDK_TARGETS}; do sdk_args="$sdk_args -t $t"; done \
    && /opt/zephyr-sdk/setup.sh $sdk_args -h -c \
    && rm /tmp/sdk.tar.xz

# ...and the 4.4 line's own SDK, registered the same way. Separate prefix, so
# both Config packages sit in the CMake user registry and each line's
# `find_package` picks the one that satisfies it: 3.7 accepts either, 4.4
# accepts only this one.
#
# `_gnu`, and the suffix is NOT optional — sdk-ng REPACKAGED at 1.0.
#
# The 0.x line ships one bundle per host, `zephyr-sdk-<ver>_linux-x86_64.tar.xz`.
# The 1.x line splits it by toolchain flavour, so the same name without a
# suffix simply does not exist and the build 404'd on its first run:
#
#   curl: (22) The requested URL returned error: 404
#   ERROR: failed to build: process "/bin/sh -c curl -fsSL -o /tmp/sdk44.tar.xz …"
#
# Measured against the release API rather than guessed — v1.0.1 offers
# `_gnu` (2033 MiB), `_llvm` (319 MiB) and `_minimal` (67 MiB), and no
# unsuffixed bundle at all. `_gnu` is the one that corresponds to 0.17.4's
# 1907 MiB bundle and carries the GNU toolchains `SDK_TARGETS` registers below;
# `_minimal` is host tools only and would need a per-target download each.
#
# That 404 is why this image was never published with the second SDK: the
# `images.yml` run for the commit that added it failed here, so the registry
# kept serving the previous `-r3` content and every `zephyr 4.4 /*` nightly cell
# went on failing `find_package(Zephyr-sdk 1.0)` against 0.17.4 — the exact
# symptom the comment above says this block exists to fix.
RUN curl -fsSL -o /tmp/sdk44.tar.xz \
        "https://github.com/zephyrproject-rtos/sdk-ng/releases/download/v${ZEPHYR_SDK_VERSION_44}/zephyr-sdk-${ZEPHYR_SDK_VERSION_44}_linux-x86_64_gnu.tar.xz" \
    && mkdir -p /opt/zephyr-sdk-${ZEPHYR_SDK_VERSION_44} \
    && tar xf /tmp/sdk44.tar.xz -C /opt/zephyr-sdk-${ZEPHYR_SDK_VERSION_44} --strip-components=1 \
    && sdk_args="" && for t in ${SDK_TARGETS}; do sdk_args="$sdk_args -t $t"; done \
    && /opt/zephyr-sdk-${ZEPHYR_SDK_VERSION_44}/setup.sh $sdk_args -h -c \
    && rm /tmp/sdk44.tar.xz
# NB: deliberately NOT setting ZEPHYR_SDK_INSTALL_DIR. That env makes Zephyr's
# FindZephyr-sdk use ONE directory exclusively, which is precisely wrong when
# two lines need two SDKs. Both are exposed as CMake-package-registry
# *candidates* instead (each registered by its own `setup.sh -c`), and
# `FindZephyr-sdk` picks per line: 3.7 accepts either, 4.4 accepts only 1.0.1.
ENV ZEPHYR_TOOLCHAIN_VARIANT=zephyr

# Zephyr's Python build prerequisites — issue 0878.
#
# `scripts/build/zephyr-python.sh` resolves an interpreter in the order
# NROS_PYTHON -> scripts/zephyr/.venv -> `python3` on PATH, and says outright
# that nano-ros never CREATES any of them: choosing between a distro package,
# `pip --user` and a venv is a decision about the host's interpreter, and a PEP
# 668 host refuses `pip --user` outright.
#
# That reasoning is about a DEVELOPER's host. This image is ours, so the
# decision it defers has no user left to make it — and deferring it here is what
# put every Zephyr nightly cell at `failure` in the `Set up Zephyr <line>
# workspace` step, before a single build ran. 21 of 21 red, for weeks, which
# also means the lane could not report the regression it did contain (issue
# 0876).
#
# Installed into the image's SYSTEM python3, which is the third arm of that
# resolution order — no venv, so the script still creates nothing and simply
# finds a `python3` that can `import west`.
#
# PINNED, like `uv` and `just` below now are in ci-base: an unpinned
# `pip install` at image-build time is the floating half of the drift axis
# docs/development/ci-image-provisioning.md describes, and two builds of the
# "same" tag would ship different tooling with no signal.
#
# The list is the union of `scripts/check-python-deps.py`'s `west` and
# `zephyr-build` groups, which is the checker this image was failing. That
# script is the single source; `scripts/check-ci-image-python-deps.py` gates the
# two against each other, because a second hand-maintained copy of a dependency
# list is how they drift.
RUN pip3 install --no-cache-dir \
        west==1.2.0 \
        pyelftools==0.31 \
        PyYAML==6.0.2 \
        pykwalify==1.8.0 \
        packaging==24.2 \
        jsonschema==4.23.0 \
    && python3 -c 'import west, elftools, yaml, pykwalify, packaging, jsonschema' \
    && west --version

# uv (Python 3.12 for the Zephyr 4.4 line) + just (recipe runner). Both via
# their canonical installers (arch/version handled upstream).
RUN curl -LsSf https://astral.sh/uv/install.sh | env UV_INSTALL_DIR=/usr/local/bin sh
RUN curl --proto '=https' --tlsv1.2 -sSf https://just.systems/install.sh \
        | bash -s -- --to /usr/local/bin

# Rust toolchain + the cross targets the zephyr examples build for (native_sim =
# x86_64-unknown-none; cortex-a9 = armv7a; cortex-m = thumbv7m/em; the SDK-built
# C/C++ links against these). Pinned to a fixed root on PATH for all users.
ENV RUSTUP_HOME=/usr/local/rustup CARGO_HOME=/usr/local/cargo
ENV PATH=/usr/local/cargo/bin:$PATH
RUN curl -fsSL https://sh.rustup.rs | sh -s -- -y --profile minimal --no-modify-path \
    && rustup target add \
        x86_64-unknown-none armv7a-none-eabi \
        thumbv7m-none-eabi thumbv7em-none-eabi \
        riscv64imac-unknown-none-elf

# git-as-root in CI containers: avoid "dubious ownership" on the checkout +
# the source submodules `nros setup` provisions.
RUN git config --system --add safe.directory '*'

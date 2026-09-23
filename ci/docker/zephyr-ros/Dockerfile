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
#
# Its build CONTEXT is the repository root, not this directory — the Dockerfile
# COPYs `ci/docker/apt-packages.txt`, a path relative to the tree. `ci-base` has
# been built that way since issue 1201, which is also the record of what
# happens when a COPY is added and the context is not moved with it: every build
# fails in under a minute at "failed to calculate checksum of ref: not found",
# and the floating tag goes on serving months-old content while the tree says
# otherwise. `Dockerfile.dockerignore` beside this file keeps that root context
# to the files named here; add a line to it when this gains a COPY.
FROM ros:humble-ros-base

# ---- ROS 2 environment, baked (issue 0866) --------------------------------
#
# The SAME gap ci-base has, and it does NOT inherit the fix: this image is
# `FROM ros:humble-ros-base` directly, not `FROM` ci-base. ci-base's header
# used to claim the opposite ("the Zephyr CI image is FROM this + the Zephyr
# SDK layer"), so a reader who fixed one image and trusted it had fixed
# neither; that sentence is corrected there now. The inheritance still does not
# exist — what is now genuinely shared is the apt closure, as a file both
# images read, not as a layer one inherits.
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
# TAG POLICY, because the two halves of this file disagreed about it and the
# disagreement is load-bearing (phase-466).
#
# This paragraph used to say the `-rN` revision is deliberately NOT bumped: a
# new tag does not exist until `images.yml` has run on `main`, so a PR that
# bumps it and moves the consumers together makes every zephyr job fail at
# "Initialize containers" between the merge and the image build. True, and it
# is what happened at `-r3`. But `images.yml`'s own header settled the other
# way at `-r4`, and that is the rule now: bump, move the consumers in the same
# commit, and rely on the slack — the workflow triggers on the merge push and
# the scheduled lanes are daily.
#
# The reason the second rule wins is not tidiness. Overwriting a tag in place
# means a lane cannot tell WHICH content it ran, so a lane that is still red
# after a fix looks identical whether the fix did not work or the image was
# never republished. That is the same indistinguishability issue 1201 records
# for `humble`, and phase-466 exists because every scheduled lane had been red
# for eleven days with nobody able to say which. A failed pull says "the image
# has not been published" in as many words; a stale pull says nothing at all.
#
# So: a content change here is a THREE-file change — this Dockerfile,
# `images.yml`'s `TAG`, and every `container:` that names the image.
# `check-ci-image-apt-packages` asserts the third against the second, because
# four hand-copied tag literals is the same defect shape one level up.
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

# The apt closure EVERY image installs, from the one list (phase-466).
#
# This block used to be a hand-written mirror of ci-base's, and it had drifted:
# no `unzip` (so `_setup-common`'s `setup-clang-format` could not unpack its
# prebuilt, and all 22 nightly `zephyr *` jobs died in the step named
# `Set up Zephyr <line> workspace` — issue 1359) and no `python3-tomli` (so on
# this jammy / Python 3.10 base no repo script could read its own TOML manifests
# — issue 1364). Each symptom was one absent package; the defect was that two
# lists had to agree and nothing made them. `ci/docker/apt-packages.txt` is now
# that one list, and its header carries the reasoning.
#
# This is why `images.yml` builds this image with the REPOSITORY ROOT as its
# context (the way it already builds ci-base since issue 1201) and why
# `Dockerfile.dockerignore` sits beside this file.
COPY ci/docker/apt-packages.txt /opt/nros-ssot/ci/docker/apt-packages.txt
# NOT a pipe into `xargs -r`: Docker's default shell is `sh -c` with no
# `pipefail`, so a `sed` that failed would leave `xargs -r` a silent no-op and
# the image would build clean with NOTHING installed -- which is the exact
# shape of the defect this file exists to remove, one layer down. `set -e`
# aborts on the failed assignment instead, and the word count refuses a list
# that has gone empty or garbled: a step that cannot fail is not a step.
RUN set -eu; \
    pkgs="$(sed -e 's/#.*//' /opt/nros-ssot/ci/docker/apt-packages.txt)"; \
    n="$(echo $pkgs | wc -w)"; \
    if [ "$n" -lt 10 ]; then echo "apt-packages.txt yielded only $n package(s)" >&2; exit 1; fi; \
    echo "shared apt closure ($n): $(echo $pkgs)"; \
    apt-get update; \
    apt-get install -y --no-install-recommends $pkgs; \
    rm -rf /var/lib/apt/lists/*

# This image's OWN packages — the ones ci-base has no use for. A name here that
# is ALSO in the shared list is refused by `check-ci-image-apt-packages`.
#
# `aria2` is here because `[prereq.aria2]` declares it for the Zephyr SDK fetch
# (issue 0368 F7) and it has always been in this image and never in ci-base.
# STATED PRECISELY rather than inherited: `scripts/zephyr/setup.sh` no longer
# shells `aria2c` — that block was replaced by a download through `nros` — so
# the only caller left would be `west sdk install`, which these jobs do not run
# (`just setup zephyr --skip-sdk`, the SDKs being baked above). It is kept
# because dropping a package is a separate decision from un-drifting two lists,
# and phase-466 is the second; if nothing turns up a caller, the honest next
# move is to drop it here and from the index together.
RUN apt-get update && apt-get install -y --no-install-recommends \
        aria2 \
    && rm -rf /var/lib/apt/lists/*

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

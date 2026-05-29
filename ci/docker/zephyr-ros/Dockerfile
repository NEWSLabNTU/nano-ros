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
# Build/publish via .github/workflows/build-zephyr-ci-image.yml →
#   ghcr.io/newslabntu/nano-ros-zephyr-ci:humble-sdk<ver>
# Tag encodes the inputs (ROS distro + SDK version); a bump = a new tag.
FROM ros:humble-ros-base

ARG ZEPHYR_SDK_VERSION=0.16.8
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
ENV ZEPHYR_SDK_INSTALL_DIR=/opt/zephyr-sdk \
    ZEPHYR_TOOLCHAIN_VARIANT=zephyr

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

# nano-ros CI base image (Phase 196.9).
#
# The stable, heavy provisioning every host/per-platform CI lane re-installs each
# run — ROS 2 Humble (base image, for the interface codegen's AMENT lookup) + the
# host build tools (cmake/ninja/dtc/gperf, uv, just, rustup + cross targets) +
# the arm cross-compiler. NOT baked: the `nros` CLI (released binary, version-
# pinned, fetched per-run via install.sh) or any board SDK/source (provisioned
# per-run by `nros setup`, the way a user does).
#
# This is the sibling base the zephyr CI image factors out of: the Zephyr CI image
# is `FROM` this + the ~1 GB Zephyr SDK layer. `platform-ci.yml` + `host-unit-
# tests` run their cells in this image — no per-cell `setup-ros` / apt / tool
# installs, and the AMENT/ROS gap that fails `nros generate-rust` is closed.
#
# Build/publish via .github/workflows/build-ci-base-image.yml →
#   ghcr.io/newslabntu/nano-ros-ci:humble  (tag = ROS distro; bump = new tag)
FROM ros:humble-ros-base

ENV DEBIAN_FRONTEND=noninteractive

# Host build deps shared across platforms + the arm cross-compiler (the setup-
# time zenoh-pico build + freertos/nuttx/threadx/qemu cross builds need it on
# PATH before `nros setup` provisions the index toolchain). curl/zstd/git for the
# per-run install.sh + nros source provisioning.
RUN apt-get update && apt-get install -y --no-install-recommends \
        cmake ninja-build device-tree-compiler gperf \
        git curl ca-certificates zstd xz-utils file wget \
        python3 python3-pip python3-venv build-essential \
        gcc-arm-none-eabi kconfig-frontends socat \
        libslirp0 libpixman-1-0 \
    && rm -rf /var/lib/apt/lists/*

# uv (Python 3.12 venvs) + just (recipe runner) via their canonical installers.
RUN curl -LsSf https://astral.sh/uv/install.sh | env UV_INSTALL_DIR=/usr/local/bin sh
RUN curl --proto '=https' --tlsv1.2 -sSf https://just.systems/install.sh \
        | bash -s -- --to /usr/local/bin

# Rust toolchain + the cross targets the platforms build for. Pinned to a fixed
# root on PATH for all users (so in-container jobs need no per-cell rustup).
ENV RUSTUP_HOME=/usr/local/rustup CARGO_HOME=/usr/local/cargo
ENV PATH=/usr/local/cargo/bin:$PATH
RUN curl -fsSL https://sh.rustup.rs | sh -s -- -y --profile minimal --no-modify-path \
    && rustup target add \
        x86_64-unknown-none armv7a-none-eabi \
        thumbv7m-none-eabi thumbv7em-none-eabi thumbv7em-none-eabihf \
        riscv32imc-unknown-none-elf riscv64imac-unknown-none-elf \
    && cargo install cargo-nextest --locked

# git-as-root in CI containers: avoid "dubious ownership" on the checkout + the
# source submodules `nros setup` provisions.
RUN git config --system --add safe.directory '*'

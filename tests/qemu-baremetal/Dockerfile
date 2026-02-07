# nano-ros QEMU ARM Development Container
#
# Provides QEMU 7.2 for Cortex-M development and testing.
# Based on Debian bookworm for up-to-date QEMU with TAP networking support.
#
# Build:
#   docker build -t nano-ros-qemu -f docker/Dockerfile.qemu-arm .
#
# Run (interactive):
#   docker run -it --rm -v $PWD:/work nano-ros-qemu
#
# Run (with networking):
#   docker run -it --rm --cap-add=NET_ADMIN --device=/dev/net/tun \
#     -v $PWD:/work nano-ros-qemu
#
# Run QEMU example:
#   docker run -it --rm -v $PWD:/work nano-ros-qemu \
#     qemu-system-arm -M mps2-an385 -nographic -semihosting \
#     -kernel target/thumbv7m-none-eabi/release/qemu-rs-blinky

FROM debian:bookworm-slim

LABEL maintainer="nano-ros developers"
LABEL description="QEMU ARM development environment for nano-ros"

# Prevent interactive prompts during package installation
ENV DEBIAN_FRONTEND=noninteractive

# Install QEMU and ARM toolchain
RUN apt-get update && apt-get install -y --no-install-recommends \
    # QEMU for ARM emulation (v7.2 in bookworm)
    qemu-system-arm \
    # ARM bare-metal toolchain
    gcc-arm-none-eabi \
    libnewlib-arm-none-eabi \
    # Build essentials
    build-essential \
    cmake \
    ninja-build \
    # Utilities
    curl \
    ca-certificates \
    git \
    # Networking tools (for TAP setup and NAT)
    iproute2 \
    iptables \
    iputils-ping \
    net-tools \
    netcat-openbsd \
    tcpdump \
    # Debug tools
    gdb-multiarch \
    && rm -rf /var/lib/apt/lists/*

# Install gosu for privilege de-escalation (network setup needs root, builds don't)
RUN apt-get update && apt-get install -y --no-install-recommends gosu && rm -rf /var/lib/apt/lists/*

# Install Rust in shared paths (not /root, so any user can access the toolchain)
ENV CARGO_HOME="/cargo"
ENV RUSTUP_HOME="/rustup"
ENV PATH="/cargo/bin:${PATH}"
RUN curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --default-toolchain stable

# Add ARM bare-metal targets
RUN rustup target add \
    thumbv6m-none-eabi \
    thumbv7m-none-eabi \
    thumbv7em-none-eabi \
    thumbv7em-none-eabihf \
    thumbv8m.main-none-eabi \
    thumbv8m.main-none-eabihf

# Install cargo tools
RUN cargo install cargo-binutils just && \
    rustup component add llvm-tools-preview

# Make toolchain readable and writable by all users (must be after all cargo/rustup installs).
# Writable because rustup may auto-sync channels and install components at runtime.
RUN chmod -R a+rwX /cargo /rustup

# Install zenohd for networked testing
RUN curl -L "https://github.com/eclipse-zenoh/zenoh/releases/download/1.2.0/zenoh-1.2.0-x86_64-unknown-linux-gnu-standalone.zip" -o /tmp/zenoh.zip && \
    apt-get update && apt-get install -y --no-install-recommends unzip && \
    unzip /tmp/zenoh.zip -d /usr/local/bin && \
    rm /tmp/zenoh.zip && \
    chmod +x /usr/local/bin/zenohd && \
    apt-get remove -y unzip && apt-get autoremove -y && \
    rm -rf /var/lib/apt/lists/*

# Set working directory
WORKDIR /work

# Verify installations
RUN echo "=== QEMU version ===" && qemu-system-arm --version && \
    echo "=== GCC version ===" && arm-none-eabi-gcc --version && \
    echo "=== Rust version ===" && rustc --version && \
    echo "=== Cargo version ===" && cargo --version

# Default entrypoint drops privileges to HOST_UID:HOST_GID if set.
# Docker Compose services override this with their own entrypoint.
COPY tests/qemu-baremetal/docker-user-entrypoint.sh /usr/local/bin/docker-user-entrypoint.sh
RUN chmod +x /usr/local/bin/docker-user-entrypoint.sh
ENTRYPOINT ["/usr/local/bin/docker-user-entrypoint.sh"]
CMD ["/bin/bash"]

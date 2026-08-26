# syntax=docker/dockerfile:1.7
#
# ROS 2 over CAN, end to end, in one container (RFC-0082 / phase-387).
#
# Reproduces the stack ROS actually ships rather than an approximation of it.
# rmw_zenoh does not consume released zenoh-c: every live branch pins a commit
# on a fork, which resolves to zenoh 2687c5135 -- a commit that is on neither
# main nor release/1.8.0. Those two SHAs are pinned literally below.
#
# Build with run.sh, which supplies the two named contexts.

# The zenoh-c commit rmw_zenoh vendors, from zenoh_cpp_vendor/CMakeLists.txt on
# every live rmw_zenoh branch. Its Cargo.lock resolves zenoh to 2687c5135,
# which is the revision the `zenoh` build context must be based on.
ARG ZENOHC_COMMIT=05bd370343b5161ca9269649b9a914c9c2dc4170

# ---------------------------------------------------------------------------
# libzenohc.so, carrying the CAN link
# ---------------------------------------------------------------------------
FROM rust:slim-bookworm AS zenohc-builder
ARG ZENOHC_COMMIT
RUN apt-get update -qq && apt-get install -y -qq --no-install-recommends \
        git cmake clang pkg-config libssl-dev ca-certificates python3 \
    && rm -rf /var/lib/apt/lists/*

# The zenoh fork carrying io/zenoh-links/zenoh-link-can, on feat/can-link-ros.
COPY --from=zenoh . /src/zenoh
RUN test -d /src/zenoh/io/zenoh-links/zenoh-link-can \
    || (echo "the 'zenoh' build context has no zenoh-link-can crate" >&2; exit 1)

WORKDIR /src
RUN git clone --filter=blob:none --no-checkout https://github.com/eclipse-zenoh/zenoh-c.git zenoh-c \
    && git -C zenoh-c fetch --depth 1 origin "${ZENOHC_COMMIT}" \
    && git -C zenoh-c checkout --detach "${ZENOHC_COMMIT}"

# Point zenoh-c at the local fork and switch the CAN feature on.
#
# TWO TRAPS, both of which cost real time to find.
#
# 1. Both manifests, always. zenoh-c's build script builds a helper crate under
#    build-resources/opaque-types to compute type sizes, and hands it the
#    PARENT's Cargo.lock while that crate keeps its own manifest. Touch only the
#    parent and the two disagree about where zenoh comes from, the size probe
#    yields nothing, and the build fails much later as "no sigatures found for
#    building generic z_take_from_loaned" -- which names nothing relevant.
#
# 2. Rewrite the dependency, do not [patch] it. A [patch] section leaves the
#    original git source in the manifest for cargo to resolve, and that same
#    build script invokes the sub-build with --offline, where resolving
#    `branch = "main"` from a cached checkout fails outright. Rewriting the
#    source to a path leaves nothing to resolve.
COPY rewrite-deps.py /tmp/rewrite-deps.py
RUN set -eu; \
    python3 /tmp/rewrite-deps.py /src/zenoh \
        /src/zenoh-c/Cargo.toml \
        /src/zenoh-c/build-resources/opaque-types/Cargo.toml; \
    for m in /src/zenoh-c/Cargo.toml /src/zenoh-c/build-resources/opaque-types/Cargo.toml; do \
        grep -q 'transport_vsock = \["zenoh/transport_vsock"\]' "$m" \
            || { echo "no transport_vsock anchor in $m; zenoh-c layout changed" >&2; exit 1; }; \
        sed -i 's|transport_vsock = \["zenoh/transport_vsock"\]|&\ntransport_can = ["zenoh/transport_can"]|' "$m"; \
    done; \
    rm -f /src/zenoh-c/build-resources/opaque-types/Cargo.lock

# The feature set must match what zenoh_cpp_vendor passes, because `unstable`
# and `shared-memory` move struct layouts and the vendored library has no
# DT_SONAME to catch a mismatch -- it would be silent corruption, not a link
# error. zenoh_cpp_vendor passes -DZENOHC_BUILD_WITH_UNSTABLE_API=TRUE and
# --features=shared-memory.
WORKDIR /src/zenoh-c
RUN cargo build --release --features unstable,shared-memory,transport_can

# Verify the link is really in the artifact rather than trusting the flag.
# Deliberately not `strings | grep -q`: under pipefail grep -q closes the pipe
# on the first match, strings takes SIGPIPE, and the pipeline reports failure
# even though the string was found.
RUN set -eu; \
    n="$(strings target/release/libzenohc.so | grep -c 'CAN: no such interface' || true)"; \
    [ "$n" -gt 0 ] || (echo "built libzenohc.so does not contain the CAN link" >&2; exit 1); \
    echo "zenohc-builder: CAN link present"

# ---------------------------------------------------------------------------
# zenoh-pico, standing in for the safety island
# ---------------------------------------------------------------------------
FROM debian:bookworm-slim AS pico-builder
RUN apt-get update -qq && apt-get install -y -qq --no-install-recommends \
        build-essential cmake \
    && rm -rf /var/lib/apt/lists/*

COPY --from=zenohpico . /src/zenoh-pico

# BATCH_MULTICAST_SIZE is load-bearing, not a tuning knob. zenoh-pico advertises
# Z_BATCH_MULTICAST_SIZE in its Join regardless of what the link beneath can
# carry, and its receiver rejects any peer whose value differs. At the default
# 2048 against a 63-byte CAN FD MTU the two never associate, and the only
# symptom is one INFO line on the pico side and nothing at all on the other.
RUN cmake -S /src/zenoh-pico -B /build/pico \
        -DCMAKE_BUILD_TYPE=Release \
        -DZ_FEATURE_LINK_CAN=1 \
        -DBATCH_MULTICAST_SIZE=63 \
        -DBUILD_EXAMPLES=ON \
        -DBUILD_TESTING=OFF \
        -DZENOH_LOG=INFO \
    && cmake --build /build/pico -j"$(nproc)" --target z_pub z_sub

RUN set -eu; \
    n="$(nm -a /build/pico/lib/libzenohpico.so 2>/dev/null | grep -c '_z_open_can' || true)"; \
    [ "$n" -gt 0 ] || (echo "zenoh-pico was built without the CAN link" >&2; exit 1); \
    echo "pico-builder: CAN link present"

# ---------------------------------------------------------------------------
# Runtime
# ---------------------------------------------------------------------------
FROM ros:humble-ros-base AS runtime
RUN apt-get update -qq && apt-get install -y -qq --no-install-recommends \
        can-utils iproute2 ros-humble-demo-nodes-cpp ros-humble-rmw-zenoh-cpp \
    && rm -rf /var/lib/apt/lists/*

# Ahead of /opt/ros/humble/opt/zenoh_cpp_vendor/lib on LD_LIBRARY_PATH. This
# substitutes the vendored library wholesale, because librmw_zenoh_cpp.so and
# rmw_zenohd name libzenohc.so as a plain DT_NEEDED with no RPATH or RUNPATH,
# and a cargo feature adds no C API so the ABI does not move.
COPY --from=zenohc-builder /src/zenoh-c/target/release/libzenohc.so /opt/can-demo/lib/
COPY --from=pico-builder /build/pico/examples/z_pub /opt/can-demo/pico/
COPY --from=pico-builder /build/pico/examples/z_sub /opt/can-demo/pico/
COPY --from=pico-builder /build/pico/lib/libzenohpico.so /opt/can-demo/lib/

COPY entrypoint.sh /opt/can-demo/entrypoint.sh
RUN chmod +x /opt/can-demo/entrypoint.sh

ENV RMW_IMPLEMENTATION=rmw_zenoh_cpp
ENTRYPOINT ["/opt/can-demo/entrypoint.sh"]

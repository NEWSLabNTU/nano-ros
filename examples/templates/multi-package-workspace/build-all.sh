#!/usr/bin/env bash
# Phase 123.A.10 — build all three packages in this workspace.
#
# Configures each CMake package with CMAKE_PREFIX_PATH pointing at
# the nano-ros install + shares one NANO_ROS_GEN_CACHE_DIR so the
# std_msgs codegen runs once across the C + C++ packages.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
NANO_ROS_ROOT="$(cd "${SCRIPT_DIR}/../../.." && pwd)"
NANO_ROS_INSTALL="${NANO_ROS_INSTALL:-${NANO_ROS_ROOT}/build/install}"
GEN_CACHE="${SCRIPT_DIR}/build/nros-gen-cache"

if [[ ! -d "${NANO_ROS_INSTALL}/lib/cmake/NanoRos" ]]; then
    cat >&2 <<EOF
build-all.sh: nano-ros not installed at ${NANO_ROS_INSTALL}
  Run from the nano-ros checkout first:
    ./tools/setup.sh --target=posix-zenoh
    just install-local
EOF
    exit 1
fi

mkdir -p "${GEN_CACHE}"

build_cmake_pkg() {
    local pkg="$1"
    local src="${SCRIPT_DIR}/src/${pkg}"
    local bld="${src}/build"
    echo "=== build ${pkg} (cmake) ==="
    cmake -S "${src}" -B "${bld}" \
        -DCMAKE_BUILD_TYPE=Release \
        -DCMAKE_PREFIX_PATH="${NANO_ROS_INSTALL}" \
        -DNANO_ROS_GEN_CACHE_DIR="${GEN_CACHE}" \
        > /dev/null
    cmake --build "${bld}" --parallel
}

build_cargo_pkg() {
    local pkg="$1"
    local src="${SCRIPT_DIR}/src/${pkg}"
    echo "=== build ${pkg} (cargo) ==="
    # Regenerate Rust msg bindings into the package's `generated/`
    # dir so the path dependencies in Cargo.toml resolve. Mirrors
    # what `cargo nano-ros generate-rust` does in standalone Rust
    # examples; this is per-package because the Rust codegen
    # cache (A.7 follow-up) isn't shared yet.
    (cd "${src}" && cargo nano-ros generate-rust > /dev/null)
    (cd "${src}" && cargo build --release)
}

build_cmake_pkg pkg_c_talker
build_cmake_pkg pkg_cpp_listener
build_cargo_pkg pkg_rust_publisher

cat <<EOF

All three packages built. Outputs:
  src/pkg_c_talker/build/pkg_c_talker
  src/pkg_cpp_listener/build/pkg_cpp_listener
  src/pkg_rust_publisher/target/release/pkg_rust_publisher

Codegen cache (shared between C + C++ packages):
  ${GEN_CACHE}
EOF

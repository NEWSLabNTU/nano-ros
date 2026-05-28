#!/usr/bin/env bash
# Build the patched QEMU (NEWSLabNTU/qemu fork) and package it for one host.
# Mirrors [tool.qemu.source] in nano-ros's nros-sdk-index.toml so the prebuilt
# and source-built layouts are identical. Phase 187.5.
#
#   build-qemu.sh <version> <host-key>   ->   dist/qemu-<host-key>.tar.zst
set -euo pipefail

version="${1:?usage: build-qemu.sh <version> <host-key>}"
host="${2:?usage: build-qemu.sh <version> <host-key>}"

root="$(pwd)"
prefix="$root/out/qemu"
rm -rf "$root/qemu-src" "$prefix"
mkdir -p "$prefix" "$root/dist"

# Host build deps (linux runners). macOS runners use brew (pre-provisioned).
if [ "${host#linux-}" != "$host" ]; then
    sudo apt-get update -qq
    sudo apt-get install -y -qq ninja-build python3-venv libglib2.0-dev \
        libpixman-1-dev zstd flex bison
fi

# Patched fork branch (no tags upstream); keep in sync with [tool.qemu.source]
# ref in nano-ros's nros-sdk-index.toml.
git clone --depth 1 --branch nano-ros-v11.0.0-patches \
    https://github.com/NEWSLabNTU/qemu qemu-src
cd qemu-src
./configure --prefix="$prefix" \
    --target-list=arm-softmmu,riscv64-softmmu \
    --disable-docs
make -j"$(getconf _NPROCESSORS_ONLN)"
make install
cd "$root"

# Tarball = the prefix CONTENTS (bin/, share/, …) so it unpacks straight into
# $NROS_HOME/sdk/qemu/<version>/ — the install-layout contract.
tar --use-compress-program "zstd -19 -T0" \
    -cf "dist/qemu-${host}.tar.zst" -C "$prefix" .
echo "built dist/qemu-${host}.tar.zst"

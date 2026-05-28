#!/usr/bin/env bash
# Build the patched QEMU (NEWSLabNTU/qemu fork) and package it for one host.
# Mirrors [tool.qemu.source] in nano-ros's nros-sdk-index.toml so the prebuilt
# and source-built layouts are identical. Phase 187.5.
#
#   build-qemu.sh <version> <host-key>   ->   dist/qemu-<host-key>.tar.zst
set -euo pipefail

version="${1:?usage: build-qemu.sh <version> <host-key> <upstream>}"
host="${2:?usage: build-qemu.sh <version> <host-key> <upstream>}"
# Fork branch/ref (e.g. nano-ros-v11.0.0-patches) — SSOT is the index
# [tool.qemu].upstream / source.ref, passed by build-tool.yml. No longer
# hardcoded here.
upstream="${3:?usage: build-qemu.sh <version> <host-key> <upstream>}"

root="$(pwd)"
prefix="$root/out/qemu"
rm -rf "$root/qemu-src" "$prefix"
mkdir -p "$prefix" "$root/dist"

# Host build deps. libslirp is REQUIRED — nano-ros QEMU tests use `-netdev user`
# (slirp NAT, no TAP/sudo); without --enable-slirp the binary can't network.
if [ "${host#linux-}" != "$host" ]; then
    sudo apt-get update -qq
    sudo apt-get install -y -qq ninja-build python3-venv libglib2.0-dev \
        libpixman-1-dev libslirp-dev zstd flex bison
else
    brew install libslirp pkg-config ninja pixman glib zstd 2>/dev/null || true
fi

# Configure flags mirror just/qemu-baremetal.just's setup-qemu so the prebuilt
# == the source build.
git clone --depth 1 --branch "$upstream" \
    https://github.com/NEWSLabNTU/qemu qemu-src
cd qemu-src
./configure --prefix="$prefix" \
    --target-list=arm-softmmu,riscv64-softmmu \
    --enable-slirp \
    --disable-docs --disable-tools --disable-gtk --disable-vnc \
    --disable-sdl --disable-spice
make -j"$(getconf _NPROCESSORS_ONLN)"
make install
cd "$root"

# Tarball = the prefix CONTENTS (bin/, share/, …) so it unpacks straight into
# $NROS_HOME/sdk/qemu/<version>/ — the install-layout contract.
tar --use-compress-program "zstd -19 -T0" \
    -cf "dist/qemu-${host}.tar.zst" -C "$prefix" .
echo "built dist/qemu-${host}.tar.zst"

#!/usr/bin/env bash
# Repackage xPack's prebuilt riscv-none-elf-gcc for one host into the nano-ros
# install-layout contract (fetch+repackage, not a source build). Phase 187.5.
#
#   build-riscv-none-elf-gcc.sh <version> <host-key>
#       -> dist/riscv-none-elf-gcc-<host-key>.tar.zst
set -euo pipefail

version="${1:?usage: build-riscv-none-elf-gcc.sh <version> <host-key> <upstream>}"
host="${2:?usage: build-riscv-none-elf-gcc.sh <version> <host-key> <upstream>}"
# Exact xPack build (e.g. 14.2.0-3) — SSOT is the index [tool.*].upstream, passed
# by build-tool.yml. Was hardcoded in a case here (the index didn't record it).
upstream="${3:?usage: build-riscv-none-elf-gcc.sh <version> <host-key> <upstream>}"

case "$host" in
linux-x86_64) plat="linux-x64" ;;
linux-arm64) plat="linux-arm64" ;;
macos-arm64) plat="darwin-arm64" ;;
*)
    echo "build-riscv-none-elf-gcc: unsupported host $host" >&2
    exit 1
    ;;
esac

asset="xpack-riscv-none-elf-gcc-${upstream}-${plat}"
url="https://github.com/xpack-dev-tools/riscv-none-elf-gcc-xpack/releases/download/v${upstream}/${asset}.tar.gz"

root="$(pwd)"
rm -rf "$root/work"
mkdir -p "$root/dist" "$root/work"
cd "$root/work"

curl -fL --retry 3 -o tc.tar.gz "$url"
tar -xf tc.tar.gz # extracts to xpack-riscv-none-elf-gcc-${upstream}/ (bin/ lib/ …)

tar --use-compress-program "zstd -19 -T0" \
    -cf "$root/dist/riscv-none-elf-gcc-${host}.tar.zst" \
    -C "xpack-riscv-none-elf-gcc-${upstream}" .
echo "built dist/riscv-none-elf-gcc-${host}.tar.zst"

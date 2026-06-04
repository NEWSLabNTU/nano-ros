#!/usr/bin/env bash
set -euo pipefail

source scripts/build/cargo.sh

make_bin="third-party/make/make"
ninja_bin="third-party/ninja/ninja"
if [ ! -x "$make_bin" ] || ! "$make_bin" --version | head -1 | grep -q "4.4"; then
    echo "jobserver build needs make >=4.4 — run: just workspace install-make" >&2
    exit 1
fi
if [ ! -x "$ninja_bin" ]; then
    echo "jobserver build needs ninja >=1.13 — run: just workspace install-ninja" >&2
    exit 1
fi

n="${NROS_BUILD_JOBS:-$(nproc 2>/dev/null || echo 8)}"
export PATH="$(pwd)/third-party/make:$(pwd)/third-party/ninja:$PATH"
echo "build-all (jobserver): $make_bin -j$n --jobserver-style=fifo -f build-all.mk"
echo "  make=$(make --version | head -1), ninja=$(ninja --version)"
echo "  cargo-profile=$(nros_cargo_profile_name), cargo-frontends=${NROS_CARGO_FRONTENDS:-auto}"

log_dir="${NROS_BUILD_LOG_DIR:-$(pwd)/tmp/build-all-$(date +%Y%m%d-%H%M%S)-$$}"
mkdir -p "$log_dir" tmp
log_dir="$(cd "$log_dir" && pwd)"
ln -sfn "$log_dir" tmp/build-all-latest
echo "  log-dir=$log_dir"

echo "build-all: prefetching Cargo registries before broad fanout"
nros_cargo_fetch_root
echo "build-all: generating Rust example bindings before standalone prefetch"
just generate-bindings
echo "build-all: prefetching standalone Cargo manifests"
nros_cargo_fetch_standalone_manifests
echo "build-all: resolving host nros codegen tool"
nros_cargo_ensure_codegen_c

exec env -u MAKEFLAGS -u CARGO_MAKEFLAGS \
    NROS_JOBSERVER=1 NROS_BUILD_JOBS="$n" NROS_BUILD_LOG_DIR="$log_dir" \
    NROS_CODEGEN_C_PREBUILT=1 CARGO_NET_OFFLINE=true \
    "$make_bin" -j"$n" --jobserver-style=fifo -f build-all.mk

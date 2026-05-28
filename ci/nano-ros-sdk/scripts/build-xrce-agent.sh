#!/usr/bin/env bash
# Build the Micro-XRCE-DDS Agent (the rmw-xrce host daemon) and package it for
# one host. Mirrors [tool.xrce-agent.source] in nros-sdk-index.toml. Phase 191.6.
#
#   build-xrce-agent.sh <version> <host-key> <upstream>  ->  dist/xrce-agent-<host>.tar.zst
#
# The Agent's CMake is a superbuild: it fetches + builds Fast-DDS / Fast-CDR and
# installs MicroXRCEAgent + its shared libs into the prefix, so the tarball is
# self-contained (bin/ + lib/).
set -euo pipefail

version="${1:?usage: build-xrce-agent.sh <version> <host-key> <upstream>}"
host="${2:?usage: build-xrce-agent.sh <version> <host-key> <upstream>}"
# Upstream tag (e.g. v2.4.3) — SSOT is the index [tool.*].upstream, passed by
# build-tool.yml.
upstream="${3:?usage: build-xrce-agent.sh <version> <host-key> <upstream>}"

root="$(pwd)"
src="$root/xrce-agent-src"
build="$src/build"
prefix="$root/out/xrce-agent"
rm -rf "$src" "$prefix"
mkdir -p "$prefix" "$root/dist"

git clone --depth 1 --branch "$upstream" \
    https://github.com/eProsima/Micro-XRCE-DDS-Agent "$src"

cmake -S "$src" -B "$build" \
    -DCMAKE_BUILD_TYPE=Release \
    -DUAGENT_BUILD_EXECUTABLE=ON \
    -DUAGENT_P2P_PROFILE=OFF \
    -DUAGENT_LOGGER_PROFILE=OFF \
    -DUAGENT_SOCKETCAN_PROFILE=OFF  # Linux-CAN transport: unused by nano-ros + breaks the macOS build (v2.4.3 compiles CanAgentLinux unconditionally)
# The Agent CMake is a superbuild: `cmake --build` builds Fast-CDR/Fast-DDS (into
# `build/temp_install`) then the Agent (into the build tree). There is no
# top-level `install` target, and the built binary's RUNPATH is absolute — so we
# assemble a relocatable prefix by hand instead of installing.
cmake --build "$build" --parallel "$(nproc 2>/dev/null || echo 4)"

# Bundle the real binary + its non-system shared libs into lib/, fronted by a
# wrapper that points LD/DYLD_LIBRARY_PATH at its own ../lib — relocatable
# without patchelf, and cross-platform (Linux + macOS).
mkdir -p "$prefix/bin" "$prefix/lib"
cp -a "$build/MicroXRCEAgent" "$prefix/lib/MicroXRCEAgent.real"
# Bundle the agent's own lib + Fast-DDS/Fast-CDR — `.so*` on Linux, `.dylib` on
# macOS (find handles both + the symlink chain).
find "$build" -maxdepth 1 \
    \( -name 'libmicroxrcedds_agent.so*' -o -name 'libmicroxrcedds_agent*.dylib' \) \
    -exec cp -a {} "$prefix/lib/" \;
find "$build/temp_install" \
    \( -name 'libfastrtps*.so*' -o -name 'libfastrtps*.dylib' \
       -o -name 'libfastcdr*.so*' -o -name 'libfastcdr*.dylib' \) \
    -exec cp -a {} "$prefix/lib/" \;
cat > "$prefix/bin/MicroXRCEAgent" <<'WRAP'
#!/bin/sh
# nano-ros-sdk relocatable launcher — resolves bundled libs next to itself.
here="$(cd "$(dirname "$0")" && pwd)"
libdir="$here/../lib"
case "$(uname -s)" in
  Darwin) DYLD_LIBRARY_PATH="$libdir${DYLD_LIBRARY_PATH:+:$DYLD_LIBRARY_PATH}" exec "$libdir/MicroXRCEAgent.real" "$@" ;;
  *) LD_LIBRARY_PATH="$libdir${LD_LIBRARY_PATH:+:$LD_LIBRARY_PATH}" exec "$libdir/MicroXRCEAgent.real" "$@" ;;
esac
WRAP
chmod +x "$prefix/bin/MicroXRCEAgent"

test -x "$prefix/bin/MicroXRCEAgent" && test -f "$prefix/lib/MicroXRCEAgent.real" \
    || { echo "error: MicroXRCEAgent not assembled in $prefix"; exit 1; }

tar --use-compress-program "zstd -19 -T0" \
    -cf "dist/xrce-agent-${host}.tar.zst" -C "$prefix" .
echo "built dist/xrce-agent-${host}.tar.zst"

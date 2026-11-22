#!/usr/bin/env bash
# Issue 2702 (ruled option 2) -- two C images on one `linux` coordinate, one
# of which states `node_interfaces = "none"`. That image gets a cmake configure
# of its own, built and sized with its own capability axes; the other keeps
# the shared one. The test reads what each configure recorded.
#
#   build.sh <out-dir>
#
# Into <out-dir>/<image>/: `exit`, `stderr`, the image's sizing descriptor
# (`descriptor.toml`), its configure's capability list (`features`, the
# `NROS_FEATURES` cache line) and its build dir's name (`build_dir`).
set -euo pipefail
out="$1"
here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
rm -rf "$out/ws"
cp -r "$here/ws" "$out/ws"
rm -rf "$out/ws/build"
for image in plain slim; do
    rec="$out/$image"
    rm -rf "$rec"
    mkdir -p "$rec"
    rc=0
    (cd "$out/ws" && nros build "probe_bringup:$image") >"$rec/stdout" 2>"$rec/stderr" || rc=$?
    printf '%s\n' "$rc" >"$rec/exit"
    [ "$rc" = 0 ] || continue
    desc=""
    for f in "$out"/ws/build/*/cmake/nros/sizing/"${image}_entry.toml"; do
        [ -f "$f" ] && desc="$f"
    done
    [ -n "$desc" ] || continue
    cp "$desc" "$rec/descriptor.toml"
    cfg="$(dirname "$(dirname "$(dirname "$desc")")")"
    basename "$(dirname "$cfg")" >"$rec/build_dir"
    grep '^NROS_FEATURES:' "$cfg/CMakeCache.txt" >"$rec/features" || true
done

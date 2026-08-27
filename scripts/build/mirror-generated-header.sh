#!/usr/bin/env bash
# Mirror a build.rs-generated header into a leaf's include dir. Issue 0805.
#
# nros-c / nros-cpp's build script writes each generated header TWICE:
#
#   1. `$CORROSION_BUILD_DIR/<name>`  — this leaf's own cmake binary dir
#   2. `$CARGO_TARGET_DIR/nros-{c,cpp}-generated/nros/<name>` — leaf-independent
#
# (1) only exists if the build script RAN for this leaf. Once leaves share a
# cargo target dir, cargo skips the script for every leaf after the first, so a
# freshly-configured leaf has no (1) — measured: it fails with no header and no
# binary. Making the script re-run per leaf is what
# `rerun-if-env-changed=CORROSION_BUILD_DIR` used to do, and that is precisely
# the issue-0491 path-variable fingerprint that made every leaf recompile
# `nros-c` + `nros-cpp` (459 s -> 9 s of cargo time when removed).
#
# So prefer (1) when present and fall back to (2), which is the same bytes: both
# are written from one `build.rs` run, and leaves only share a target dir when
# their configuration key matches, which is what makes the header identical.
#
# Usage: mirror-generated-header.sh <corrosion-src> <build-dir> <gen-subdir> <name> <dest>
set -euo pipefail
src="$1"; build_dir="$2"; gen_subdir="$3"; name="$4"; dest="$5"

if [ ! -f "$src" ]; then
    # `<build>/cargo/<workspace>_<hash>/...` — one entry in practice; the glob
    # avoids hardcoding Corrosion's hash, and this path is identical whether
    # `cargo` is a real directory or the shared-store symlink.
    for cand in "$build_dir"/cargo/*/"$gen_subdir"/nros/"$name"; do
        [ -f "$cand" ] && { src="$cand"; break; }
    done
fi

if [ ! -f "$src" ]; then
    echo "nros: no generated $name to mirror (looked in the leaf's corrosion dir" >&2
    echo "      and $build_dir/cargo/*/$gen_subdir/nros/) — issue 0805" >&2
    exit 1
fi

mkdir -p "$(dirname "$dest")"
# copy_if_different semantics, so an unchanged header does not re-stamp mtime
# and re-trigger every consumer TU.
if [ ! -f "$dest" ] || ! cmp -s "$src" "$dest"; then
    cp -- "$src" "$dest"
fi

#!/bin/bash
# tests/leaf-row-build-class-tests.sh -- phase-489 W6
#
# A fixture group's single-package leaves share ONE cargo target dir, and each
# leaf's settings name its own NROS_SIZING_DESCRIPTOR file. Cargo compares that
# variable as text, so without a per-configuration build dir every row after a
# different leaf recompiled nros / nros-node / nros-params / nros-rmw-cffi /
# zpico-sys: ~12.7 s a row, ~50 rows, on a re-run with nothing changed (and the
# staleness probe did the same). `nros build` already keyed a class for a caller
# `--target-dir` (issue 1885); the fixture road did not.
#
# This holds the plumbing, with a stub CLI so it needs no build:
#   A. a row with a shared target dir hands the CLI that dir, and builds with
#      `--config build.build-dir=<what the CLI printed>`;
#   B. the `--target-dir=X` spelling is read the same way;
#   C. a row with no shared target dir passes nothing and gets no build-dir;
#   D. two leaves the CLI puts in different classes get different build dirs
#      (so a real configuration change is never folded into another's units);
#   F. (real CLI, when one is built) two leaves in ONE class carry the SAME
#      NROS_SIZING_DESCRIPTOR text in their settings -- a leaf-relative path
#      keeps the leaf's name in its text, cargo compares text, and the two then
#      recompile the core crates against each other (the second half of the bug;
#      the build dir alone does not fix it);
#   E. BOTH callers -- the build and the staleness probe -- go through the one
#      helper with the row's target-dir flag, so they cannot build a row two ways.
set -euo pipefail
cd "$(dirname "$0")/.."
root="$PWD"
tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT
fail=0
ok() { printf 'ok   %s\n' "$1"; }
bad() { printf 'FAIL %s\n' "$1"; fail=1; }

# Two leaves, each a system.toml + Cargo.toml.
for l in a b; do
    mkdir -p "$tmp/leaves/$l/build/linux"
    : >"$tmp/leaves/$l/system.toml"
    : >"$tmp/leaves/$l/Cargo.toml"
    : >"$tmp/leaves/$l/build/linux/nros-cargo.toml"
done

# Stub CLI: records its argv; prints a class build dir keyed on the leaf name
# when it was given --target-dir (what the real writer does), nothing otherwise.
cat >"$tmp/nros" <<STUB
#!/bin/bash
echo "\$*" >>"$tmp/argv"
leaf="\${@: -1}"
echo "NROS_LEAF_SETTINGS=\$leaf/build/linux/nros-cargo.toml"
prev=""
for t in "\$@"; do
    if [ "\$prev" = "--target-dir" ]; then
        echo "NROS_LEAF_BUILD_DIR=\$t/nros-class/key-\$(basename "\$leaf")/build"
    fi
    prev="\$t"
done
STUB
chmod +x "$tmp/nros"

source scripts/build/leaf-settings.sh
# shellcheck source=scripts/lib/grep-q.sh
source scripts/lib/grep-q.sh
export NROS_CLI="$tmp/nros" NROS_REPO_ROOT="$tmp/leaves"
cd "$tmp/leaves"

: >"$tmp/argv"
nros_leaf_settings_prepare a "" " --target-dir /t/group"
[ "$NROS_LEAF_BUILD_ARGS" = '--config build.build-dir="/t/group/nros-class/key-a/build"' ] \
    && ok "A: shared target dir -> class build-dir" || bad "A: got '$NROS_LEAF_BUILD_ARGS'"
nros_grep_q -F -- "--target-dir /t/group" "$tmp/argv" && ok "A: the CLI was told the dir" || bad "A: CLI not told"

nros_leaf_settings_prepare a "" "--target-dir=/t/eq"
[ "$NROS_LEAF_BUILD_ARGS" = '--config build.build-dir="/t/eq/nros-class/key-a/build"' ] \
    && ok "B: --target-dir=X read" || bad "B: got '$NROS_LEAF_BUILD_ARGS'"

: >"$tmp/argv"
nros_leaf_settings_prepare a "" ""
[ -z "$NROS_LEAF_BUILD_ARGS" ] && ! nros_grep_q -F -- "--target-dir" "$tmp/argv" \
    && ok "C: no shared dir -> nothing passed, no build-dir" || bad "C: got '$NROS_LEAF_BUILD_ARGS'"

nros_leaf_settings_prepare a "" "--target-dir /t/g"; ba="$NROS_LEAF_BUILD_ARGS"
nros_leaf_settings_prepare b "" "--target-dir /t/g"; bb="$NROS_LEAF_BUILD_ARGS"
[ -n "$ba" ] && [ "$ba" != "$bb" ] \
    && ok "D: different classes -> different build dirs" || bad "D: '$ba' vs '$bb'"

# E. one spelling, both callers, the row's own target-dir flag.
for f in scripts/build/fixtures-build.sh scripts/test/rust-fixture-stale.sh; do
    if nros_grep_q -E 'nros_leaf_settings_prepare "\$dir" "\$image" "\$tdir_flag"' "$root/$f" \
        && nros_grep_q -F 'NROS_LEAF_BUILD_ARGS' "$root/$f"; then
        ok "E: $f builds through the class"
    else
        bad "E: $f does not pass the row's target dir to nros_leaf_settings_prepare"
    fi
done

# F. needs the real CLI and a rendered tree; it reports instead of skipping
# silently. It rewrites the leaves' own gitignored build/<image>/nros-cargo.toml
# (pointed at a throwaway target dir), so each leaf is probed under the fixture
# build's own `nros_leaf_lock` and its settings files are restored byte for
# byte, mtimes included, before the lock is released: a concurrent fixture
# build never sees the throwaway paths.
cd "$root"
# issue 1850 — $NROS_CLI's caller-set value is the stub by now, so ask for
# this checkout's own CLI; never PATH, never a joined path.
# shellcheck source=scripts/build/cargo.sh
source scripts/build/cargo.sh
real_cli="$(NROS_CLI= nros_cli_bin --checkout-only 2>/dev/null || true)"
f_probe() {   # <leaf> -> prints "<build-args>|<descriptor row>"
    local l="$1" bak="$tmp/bak" s rc=0
    rm -rf "$bak"; mkdir -p "$bak"
    for s in "$l"/build/*/nros-cargo.toml; do
        [ -f "$s" ] || continue
        mkdir -p "$bak/$(dirname "$s")"; cp -p "$s" "$bak/$s"
    done
    if nros_leaf_settings_prepare "$l" "" " --target-dir $tmp/class-target" 2>/dev/null \
        && [ -n "$NROS_LEAF_BUILD_ARGS" ]; then
        printf '%s|%s\n' "$NROS_LEAF_BUILD_ARGS" \
            "$(grep -h '^NROS_SIZING_DESCRIPTOR' "$NROS_LEAF_SETTINGS_OUT" || true)"
    else
        rc=1
    fi
    for s in "$l"/build/*/nros-cargo.toml; do
        if [ -f "$bak/$s" ]; then cp -p "$bak/$s" "$s"; else rm -f "$s"; fi
    done
    return "$rc"
}
if [ -n "$real_cli" ] && [ -x "$real_cli" ] && [ -z "${NROS_LEAF_CLASS_TEST_NO_REAL:-}" ]; then
    export NROS_CLI="$real_cli" NROS_REPO_ROOT="$root"
    seen="$tmp/seen"; : >"$seen"
    pair=""
    for l in examples/linux/rust/*/; do
        l="${l%/}"
        [ -f "$l/system.toml" ] && [ -f "$l/Cargo.toml" ] || continue
        got="$(nros_leaf_lock "$l" f_probe "$l")" || continue
        key="${got%%|*}" row="${got#*|}"
        if prev="$(grep -F -- "$key|" "$seen" | head -1)" && [ -n "$prev" ]; then
            if [ "${prev#*|}" = "$row" ]; then
                ok "F: $l shares a class with an earlier leaf and carries the same descriptor text"
            else
                bad "F: $l is in the class of an earlier leaf but its NROS_SIZING_DESCRIPTOR text differs"
            fi
            pair=1
            break
        fi
        printf '%s\n' "$got" >>"$seen"
    done
    [ -n "$pair" ] || echo "note F: no two rendered leaves share a class here; not exercised"
else
    echo "note F: no built CLI; not exercised"
fi

exit "$fail"

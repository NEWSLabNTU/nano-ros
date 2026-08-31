#!/usr/bin/env bash
#
# Issue 0945 item 1 — did cargo actually write into the shared directory?
#
# WHAT THIS GUARDS
#
# `nros_share_corrosion_cargo_dir()` collapses a leaf's private cargo target dir
# onto a shared one by SYMLINKING over `${CMAKE_BINARY_DIR}/cargo`, because
# Corrosion derives its `--target-dir` as
#
#     ${CMAKE_BINARY_DIR}/${build_dir}/cargo/<workspace-folder>_<hash5>
#
# and exposes no knob for it — verified against the pinned v0.6.1 AND against
# upstream `master` (2026-08-31): same `cmake_path(APPEND ...)` line, still no
# cache variable, no `corrosion_import_crate()` argument, no target property.
# So the symlink is not a workaround for a version we are behind on; it is the
# only override point that exists.
#
# The exposure is that the redirect depends on a path Corrosion computes
# PRIVATELY. If a future Corrosion renames or moves that directory, the symlink
# points somewhere cargo no longer looks. Nothing fails: the build succeeds and
# silently stops sharing, so the only symptom is that six platforms get slower
# and no one notices. That is the failure mode this script converts into a loud
# one.
#
# WHAT IT CHECKS, AND WHY THESE TWO THINGS
#
# Deliberately NOT a re-implementation of Corrosion's formula — a second copy of
# the thing we do not control would drift from it silently, which is the defect
# rather than the fix. Instead it observes the RESULT:
#
#   1. `${CMAKE_BINARY_DIR}/cargo` is still a symlink to the shared directory
#      this configure chose. Catches something replacing the link with a real
#      directory (an out-of-band `mkdir`, a cleanup script, a stray tool).
#
#   2. an artifact with the built target's file NAME exists under the shared
#      directory, and its SIZE matches the copy Corrosion produced. If cargo has
#      started writing elsewhere, the shared directory is either empty (a fresh
#      key — the common case, caught immediately) or holds an artifact stale
#      against the one just built, whose size differs as soon as any code does.
#
# BE HONEST ABOUT THE HOLE IN (2): a long-lived build dir that shared
# successfully before a Corrosion upgrade keeps a same-named artifact around, so
# if the code has not changed the sizes can still match and this passes. It
# cannot pass for long — any edit moves the size — and it cannot pass at all for
# a new key, which is what a reconfigure after an upgrade produces. A check that
# fires on the first fresh configure is worth more than one that pretends to be
# exact.
#
# Byte-comparing instead of size-comparing would close that hole and cost a full
# read of a multi-hundred-MB archive on EVERY leaf build. Not worth it for a
# performance regression detector.
#
# WHY THIS IS NOT ITSELF AN INTERNALS DEPENDENCY
#
# Everything it consumes is documented or ours: the artifact path and name come
# from `$<TARGET_FILE:...>` / `$<TARGET_FILE_NAME:...>` (documented CMake
# generator expressions), the shared directory is ours, and the rest is stat(2).
# Nothing here parses Corrosion or cargo internals.

set -euo pipefail

usage() {
    cat >&2 <<'EOF'
usage: check-shared-cargo-dir-used.sh --shared-dir DIR --link PATH --artifact FILE [--label NAME]
       check-shared-cargo-dir-used.sh --self-test

  --shared-dir  the keyed directory nros_shared_cargo_dir() chose
  --link        the path symlinked at it (${CMAKE_BINARY_DIR}/cargo)
  --artifact    the built artifact Corrosion copied out ($<TARGET_FILE:...>)
  --label       name used in messages (defaults to the artifact basename)

Set NROS_ALLOW_UNSHARED_CARGO_DIR=1 to downgrade a failure to a warning — for
someone mid-upgrade who wants the build to finish while they fix the redirect.
EOF
}

# ---------------------------------------------------------------------------
# The check itself, as a function so `--self-test` can drive it on fixtures.
# Returns 0 on OK, 1 on a violation; prints its own diagnosis.
# ---------------------------------------------------------------------------
shared_cargo_dir_used() {
    local shared_dir="$1" link="$2" artifact="$3" label="$4"
    local rc=0

    if [ ! -d "$shared_dir" ]; then
        echo "shared-cargo-dir: FAIL — the shared directory does not exist:" >&2
        echo "    $shared_dir" >&2
        return 1
    fi

    # --- 1. the redirect is still a symlink at the directory we chose --------
    if [ ! -L "$link" ]; then
        echo "shared-cargo-dir: FAIL — $link is not a symlink." >&2
        if [ -d "$link" ]; then
            echo "  It is a real directory, so cargo built into this leaf's own" >&2
            echo "  tree and nothing was shared. Something created it after" >&2
            echo "  configure, or this build dir predates sharing and the" >&2
            echo "  configure-time degrade path fired (issue 0805)." >&2
        fi
        rc=1
    else
        local current
        current="$(readlink -f "$link" || true)"
        local want
        want="$(readlink -f "$shared_dir" || true)"
        if [ "$current" != "$want" ]; then
            echo "shared-cargo-dir: FAIL — $link points somewhere else." >&2
            echo "    points at: ${current:-<dangling>}" >&2
            echo "    expected:  $want" >&2
            rc=1
        fi
    fi

    # --- 2. the shared directory actually received this artifact ------------
    local name
    name="$(basename "$artifact")"
    if [ ! -f "$artifact" ]; then
        echo "shared-cargo-dir: FAIL — the built artifact is missing:" >&2
        echo "    $artifact" >&2
        return 1
    fi
    local want_size
    want_size="$(stat -c '%s' "$artifact")"

    # maxdepth 6: <shared>/<folder>_<hash>/<triple>/<profile>/<name> is 4, and
    # a no-triple (host) layout is 3. Six leaves room for a layout change
    # WITHIN the shared dir without this turning into a second formula.
    local found
    found="$(find "$shared_dir" -maxdepth 6 -type f -name "$name" 2>/dev/null || true)"

    if [ -z "$found" ]; then
        echo "shared-cargo-dir: FAIL — no \`$name\` anywhere under the shared dir." >&2
        echo "    shared dir: $shared_dir" >&2
        echo "    built:      $artifact" >&2
        echo "  cargo is not writing where the symlink points. The likely cause" >&2
        echo "  is a Corrosion upgrade that moved the \`--target-dir\` it derives" >&2
        echo "  privately (issue 0945 item 1) — re-read" >&2
        echo "  nros_share_corrosion_cargo_dir() against the installed" >&2
        echo "  Corrosion.cmake and move the redirect to the new path." >&2
        echo "  To build anyway, drop -DNROS_SHARED_CARGO_ROOT (correct, just" >&2
        echo "  slower) or set NROS_ALLOW_UNSHARED_CARGO_DIR=1." >&2
        return 1
    fi

    local matched=0 sizes=""
    local f sz
    while IFS= read -r f; do
        [ -n "$f" ] || continue
        sz="$(stat -c '%s' "$f")"
        sizes="${sizes:+$sizes
}    $sz  $f"
        if [ "$sz" = "$want_size" ]; then
            matched=1
        fi
    done <<< "$found"

    if [ "$matched" -eq 0 ]; then
        echo "shared-cargo-dir: FAIL — \`$name\` under the shared dir is STALE." >&2
        echo "    just built: $want_size bytes  $artifact" >&2
        echo "    shared dir holds:" >&2
        printf '%s\n' "$sizes" >&2
        echo "  Corrosion copied an artifact that did not come from the shared" >&2
        echo "  directory, so this leaf built into a private tree and the shared" >&2
        echo "  copy is left over from before (issue 0945 item 1)." >&2
        return 1
    fi

    if [ "$rc" -eq 0 ]; then
        echo "shared-cargo-dir OK ($label): cargo wrote $name into $shared_dir"
    fi
    return "$rc"
}

# ---------------------------------------------------------------------------
# self_test — the five states this can be in, on real files.
#
# phase-395 — this runs on the NORMAL path, not only behind the flag. A
# negative control nobody runs decays into a comment, and that is a sharper
# risk here than for most gates: this script's whole job is to notice that
# something upstream moved. A witness that had quietly stopped witnessing would
# be the exact defect it exists to catch, one level up. Five temp files against
# a cargo build is not a cost worth reasoning about.
#
# Pass `verbose` to print per-case lines (what `--self-test` does); silent
# otherwise, so a normal build says nothing unless something is wrong.
# ---------------------------------------------------------------------------
self_test() {
    local verbose="${1:-}"
    local st_fails=0 st_tmp
    st_tmp="$(mktemp -d)"

    st_case() { # name expected_rc cmd...
        local name="$1" want="$2"
        shift 2
        local got=0
        "$@" >/dev/null 2>&1 || got=$?
        if [ "$got" -ne "$want" ]; then
            echo "  [FAIL] $name: expected rc=$want, got rc=$got" >&2
            st_fails=$((st_fails + 1))
        elif [ "$verbose" = "verbose" ]; then
            echo "  [ok]   $name"
        fi
    }

    # (a) the healthy state: link points at the shared dir, cargo wrote there.
    mkdir -p "$st_tmp/a/shared/nano-ros_1147c/armv7a/release" "$st_tmp/a/build"
    printf 'ARCHIVE-BYTES' > "$st_tmp/a/shared/nano-ros_1147c/armv7a/release/libnros_c.a"
    printf 'ARCHIVE-BYTES' > "$st_tmp/a/build/libnros_c.a"   # Corrosion's copy
    ln -s "$st_tmp/a/shared" "$st_tmp/a/build/cargo"
    st_case "healthy: link + fresh artifact in the shared dir" 0 \
        shared_cargo_dir_used "$st_tmp/a/shared" "$st_tmp/a/build/cargo" \
                              "$st_tmp/a/build/libnros_c.a" nros_c

    # (b) Corrosion moved: it wrote into the leaf's own tree, shared dir empty.
    #     This is the case a fresh configure after an upgrade produces, and the
    #     one this script exists for.
    mkdir -p "$st_tmp/b/shared" "$st_tmp/b/build"
    printf 'ARCHIVE-BYTES' > "$st_tmp/b/build/libnros_c.a"
    ln -s "$st_tmp/b/shared" "$st_tmp/b/build/cargo"
    st_case "moved: shared dir never received the artifact" 1 \
        shared_cargo_dir_used "$st_tmp/b/shared" "$st_tmp/b/build/cargo" \
                              "$st_tmp/b/build/libnros_c.a" nros_c

    # (c) Corrosion moved on a build dir that HAD been sharing: the shared copy
    #     survives but is stale, so the sizes disagree.
    mkdir -p "$st_tmp/c/shared/nano-ros_1147c/armv7a/release" "$st_tmp/c/build"
    printf 'OLD' > "$st_tmp/c/shared/nano-ros_1147c/armv7a/release/libnros_c.a"
    printf 'NEW-AND-LONGER' > "$st_tmp/c/build/libnros_c.a"
    ln -s "$st_tmp/c/shared" "$st_tmp/c/build/cargo"
    st_case "moved: shared copy survives but is stale" 1 \
        shared_cargo_dir_used "$st_tmp/c/shared" "$st_tmp/c/build/cargo" \
                              "$st_tmp/c/build/libnros_c.a" nros_c

    # (d) the redirect is gone — a real directory where the symlink was.
    mkdir -p "$st_tmp/d/shared/nano-ros_1147c/armv7a/release" "$st_tmp/d/build/cargo"
    printf 'ARCHIVE-BYTES' > "$st_tmp/d/shared/nano-ros_1147c/armv7a/release/libnros_c.a"
    printf 'ARCHIVE-BYTES' > "$st_tmp/d/build/libnros_c.a"
    st_case "redirect replaced by a real directory" 1 \
        shared_cargo_dir_used "$st_tmp/d/shared" "$st_tmp/d/build/cargo" \
                              "$st_tmp/d/build/libnros_c.a" nros_c

    # (e) the link points at a DIFFERENT key's directory — the artifact is
    #     found (so check 2 passes) and only check 1 can catch it. Guards
    #     against the two checks being collapsed into one.
    mkdir -p "$st_tmp/e/shared/nano-ros_1147c/armv7a/release" "$st_tmp/e/other" "$st_tmp/e/build"
    printf 'ARCHIVE-BYTES' > "$st_tmp/e/shared/nano-ros_1147c/armv7a/release/libnros_c.a"
    printf 'ARCHIVE-BYTES' > "$st_tmp/e/build/libnros_c.a"
    ln -s "$st_tmp/e/other" "$st_tmp/e/build/cargo"
    st_case "link points at another key's directory" 1 \
        shared_cargo_dir_used "$st_tmp/e/shared" "$st_tmp/e/build/cargo" \
                              "$st_tmp/e/build/libnros_c.a" nros_c

    rm -rf "$st_tmp"
    if [ "$st_fails" -ne 0 ]; then
        echo "check-shared-cargo-dir-used: self-test FAILED ($st_fails case(s))" >&2
        echo "  The witness cannot be trusted to report on anything else." >&2
        return 1
    fi
    [ "$verbose" = "verbose" ] && echo "check-shared-cargo-dir-used --self-test: 5 case(s) OK"
    return 0
}

if [ "${1:-}" = "--self-test" ]; then
    self_test verbose
    exit $?
fi

SHARED_DIR=""
LINK=""
ARTIFACT=""
LABEL=""
while [ $# -gt 0 ]; do
    case "$1" in
        --shared-dir) SHARED_DIR="$2"; shift 2 ;;
        --link)       LINK="$2";       shift 2 ;;
        --artifact)   ARTIFACT="$2";   shift 2 ;;
        --label)      LABEL="$2";      shift 2 ;;
        -h|--help)    usage; exit 0 ;;
        *) echo "check-shared-cargo-dir-used: unknown argument '$1'" >&2; usage; exit 2 ;;
    esac
done

if [ -z "$SHARED_DIR" ] || [ -z "$LINK" ] || [ -z "$ARTIFACT" ]; then
    usage
    exit 2
fi
[ -n "$LABEL" ] || LABEL="$(basename "$ARTIFACT")"

# The negative control, on the normal path — see the note above `self_test`.
self_test || exit 1

if shared_cargo_dir_used "$SHARED_DIR" "$LINK" "$ARTIFACT" "$LABEL"; then
    exit 0
fi

if [ "${NROS_ALLOW_UNSHARED_CARGO_DIR:-}" = "1" ]; then
    echo "shared-cargo-dir: NROS_ALLOW_UNSHARED_CARGO_DIR=1 — continuing unshared." >&2
    exit 0
fi
exit 1

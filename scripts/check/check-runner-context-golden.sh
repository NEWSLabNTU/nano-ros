#!/usr/bin/env bash
# check-runner-context-golden — the CI runner's build context is what it was.
#
# phase-551 W1 (RFC-0127 D1, option B) moved the runner's Dockerfile generation
# out of `runner-container.sh` into the shared image generator
# (`scripts/ci/lib/image-context.sh`), so a starter image can be generated
# from the same resolved index. The runner/CI owner's review contract for that
# move: the runner's build context stays BYTE-IDENTICAL, and something holds
# it there afterwards. This is that something.
#
# The context is generated into a temp dir (`scripts/ci/image-context.sh
# runner`) with the ROS distro pinned (`NROS_RUNNER_ROS_DISTRO=humble`, the
# default, so the answer does not depend on the shell), and compared with
# `ci/docker/runner-context.golden/`:
#
#   * the GENERATED files (Dockerfile, entrypoint.sh, nros-python-layer.json)
#     byte for byte against `<name>.golden`;
#   * the file LIST and every file's MODE against `MANIFEST`, so the context
#     gains no file (a starter's files go to its own context) and loses none;
#   * every COPIED file byte for byte against the tracked source `MANIFEST`
#     names for it, rather than a second copy of each script in the golden.
#
# A deliberate change -- a moved index pin that changes the resolved apt list,
# a new layer, a runner version bump -- regenerates the golden in the same
# change, so the review sees the diff:
#
#   bash scripts/check/check-runner-context-golden.sh --update
#
# Usage: check-runner-context-golden.sh [--update] [--self-test]
set -euo pipefail
REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
GOLDEN="$REPO_ROOT/ci/docker/runner-context.golden"
NAME=check-runner-context-golden
GENERATED=(Dockerfile entrypoint.sh nros-python-layer.json)
# The files the generator COPIES, and the tracked file each one is.
COPIES=(
    "ros2-apt-source.sh scripts/sdk/ros2-apt-source.sh"
    "zenohd.sh scripts/dev/zenohd.sh"
    "python-packages.py scripts/sdk/python-packages.py"
    "index_packages.py scripts/lib/index_packages.py"
)

generate() {
    NROS_RUNNER_ROS_DISTRO=humble bash "$REPO_ROOT/scripts/ci/image-context.sh" runner "$1" \
        >/dev/null
}

manifest_of() {
    local dir="$1" f name src
    for f in "$dir"/*; do
        name="$(basename "$f")"
        src="generated"
        for c in "${COPIES[@]}"; do
            [ "${c%% *}" = "$name" ] && src="copy ${c#* }"
        done
        printf '%s %s %s\n' "$(stat -c '%a' "$f")" "$name" "$src"
    done
}

# compare <generated dir> <golden dir> -> prints each difference, returns 1 on any
compare() {
    local gen="$1" gold="$2" bad=0 f name src
    for f in "${GENERATED[@]}"; do
        if ! cmp -s "$gen/$f" "$gold/$f.golden"; then
            echo "  $f differs from its golden:" >&2
            diff -u "$gold/$f.golden" "$gen/$f" | sed 's/^/    /' | head -40 >&2 || true
            bad=1
        fi
    done
    if ! diff -q <(manifest_of "$gen") "$gold/MANIFEST" >/dev/null; then
        echo "  the context's file list or modes moved (MANIFEST):" >&2
        diff -u "$gold/MANIFEST" <(manifest_of "$gen") | sed 's/^/    /' >&2 || true
        bad=1
    fi
    while read -r _mode name src; do
        case "$src" in
            copy\ *)
                src="${src#copy }"
                if ! cmp -s "$gen/$name" "$REPO_ROOT/$src"; then
                    echo "  $name is not a byte copy of $src" >&2
                    bad=1
                fi ;;
        esac
    done < "$gold/MANIFEST"
    return "$bad"
}

self_test() {
    local t
    t="$(mktemp -d)"
    trap 'rm -rf "$t"' RETURN
    generate "$t/gen"
    mkdir -p "$t/gold"
    for f in "${GENERATED[@]}"; do cp "$t/gen/$f" "$t/gold/$f.golden"; done
    manifest_of "$t/gen" > "$t/gold/MANIFEST"
    compare "$t/gen" "$t/gold" 2>/dev/null || { echo "$NAME: self-test: identical reads as different" >&2; return 1; }
    # A changed generated line, an extra context file, a changed mode: each is named.
    cp -r "$t/gen" "$t/a"; sed -i 's/^ARG RUNNER_VERSION=.*/ARG RUNNER_VERSION=9.9.9/' "$t/a/Dockerfile"
    ! compare "$t/a" "$t/gold" 2>/dev/null || { echo "$NAME: self-test: an edited Dockerfile passed" >&2; return 1; }
    cp -r "$t/gen" "$t/b"; touch "$t/b/starter.txt"
    ! compare "$t/b" "$t/gold" 2>/dev/null || { echo "$NAME: self-test: an extra context file passed" >&2; return 1; }
    cp -r "$t/gen" "$t/c"; chmod -x "$t/c/entrypoint.sh"
    ! compare "$t/c" "$t/gold" 2>/dev/null || { echo "$NAME: self-test: a lost +x passed" >&2; return 1; }
    echo "$NAME: self-test OK"
}

case "${1:-}" in
    --self-test) self_test; exit $? ;;
    --update)
        t="$(mktemp -d)"; trap 'rm -rf "$t"' EXIT
        generate "$t/gen"
        mkdir -p "$GOLDEN"
        for f in "${GENERATED[@]}"; do cp "$t/gen/$f" "$GOLDEN/$f.golden"; done
        manifest_of "$t/gen" > "$GOLDEN/MANIFEST"
        echo "$NAME: golden written to ${GOLDEN#"$REPO_ROOT"/}"
        exit 0 ;;
    "") ;;
    *) echo "usage: $NAME [--update] [--self-test]" >&2; exit 2 ;;
esac

self_test >/dev/null
t="$(mktemp -d)"; trap 'rm -rf "$t"' EXIT
generate "$t/gen"
if ! compare "$t/gen" "$GOLDEN"; then
    echo "$NAME: the CI runner's build context moved." >&2
    echo "  The runner/CI owner reviews it byte for byte (phase-551 W1). If the change" >&2
    echo "  is meant, regenerate the golden in this change so the diff is reviewed:" >&2
    echo "    bash scripts/check/check-runner-context-golden.sh --update" >&2
    exit 1
fi
echo "$NAME: OK (${#GENERATED[@]} generated files, ${#COPIES[@]} copies, file list and modes)"

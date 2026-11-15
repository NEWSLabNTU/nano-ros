#!/usr/bin/env bash
# evict-build-stage-only.sh evict|verify|selftest
#
# phase-559 W12 (the user's ruling, 2026-11-15: option A). The hosted tier1
# zephyr shard's BUILD fits its 146 G runner. The RUN stage then compiles the
# test binaries, writes about 7.5 G more, and crosses the disk balloon's floor
# (run 43684256518), so the shard is about 5–8 G short. Two directories are
# outputs of the metadata probe alone, and the probe runs only inside
# `nros configure` during the build stage. Its sidecars live in
# `src/*/metadata/`, which stays:
#
#   build/corrosion-cargo/metadata-probe/   the probe's per-workspace cargo dirs
#   build/metadata-probe/                   the probe's harness build dirs
#
# `build/sizes-probe/` is NOT evicted: the run stage's own cargo builds may
# re-enter it.
#
# evict   Proves each dir is gitignored (`git check-ignore`, never a name
#         match), records its real size (`du`, allocated blocks), removes it,
#         and puts a READ-ONLY SENTINEL FILE of the same name in its place.
#         That file is the guard the ruling asked for. Anything in the run
#         stage that reads or creates under the evicted path gets `ENOTDIR`
#         at the point of use, through the workspaces' `cargo` symlinks into
#         it too, and fails loudly instead of quietly rebuilding 20 G.
# verify  After the run, fails if a sentinel was touched, replaced or removed.
# selftest  The negative controls, on a scratch git repo: a reader that
#         descends into the evicted path must fail, and a sentinel that was
#         replaced must fail `verify`.
set -euo pipefail

DIRS=(build/corrosion-cargo/metadata-probe build/metadata-probe)
SENTINEL_TEXT="evicted by scripts/ci/evict-build-stage-only.sh between the build and run stages (phase-559 W12); the run stage must not read this path"

note() {   # to the log, the job summary, and the disk transcript
    echo "$*"
    [ -z "${GITHUB_STEP_SUMMARY:-}" ] || echo "$*" >> "$GITHUB_STEP_SUMMARY"
    [ -z "${NROS_DISK_TRANSCRIPT:-}" ] || echo "$*" >> "$NROS_DISK_TRANSCRIPT"
}

evict() {
    local root="$1" d bytes total=0 free_before free_after
    free_before=$(df -B1 --output=avail "$root" | tail -1 | tr -d ' ')
    for d in "${DIRS[@]}"; do
        if [ ! -e "$root/$d" ] && [ ! -L "$root/$d" ]; then
            note "evict: $d absent (nothing built there), sentinel only"
        else
            git -C "$root" check-ignore -q "$d" \
                || { echo "evict: REFUSED: $d is not gitignored" >&2; exit 1; }
            bytes=$(du -s --block-size=1 "$root/$d" | cut -f1)
            total=$((total + bytes))
            rm -rf "${root:?}/$d"
            note "evict: $d $(numfmt --to=iec "$bytes") (du, allocated blocks)"
        fi
        mkdir -p "$(dirname "$root/$d")"
        printf '%s\n' "$SENTINEL_TEXT" > "$root/$d"
        chmod a-w "$root/$d"
    done
    free_after=$(df -B1 --output=avail "$root" | tail -1 | tr -d ' ')
    note "evict: freed $(numfmt --to=iec "$total") by du; free space $(numfmt --to=iec "$free_before") -> $(numfmt --to=iec "$free_after")"
}

verify() {
    local root="$1" d bad=0
    for d in "${DIRS[@]}"; do
        if [ -L "$root/$d" ] || [ ! -f "$root/$d" ] || [ "$(cat "$root/$d")" != "$SENTINEL_TEXT" ]; then
            echo "::error::evict: the run stage TOUCHED $d (its sentinel was replaced or removed). The run reads a build-stage-only dir, so the W12 eviction is unsafe as it stands." >&2
            bad=1
        fi
    done
    [ "$bad" -eq 0 ] && note "evict: verified, the run stage left all ${#DIRS[@]} sentinels untouched"
    return "$bad"
}

selftest() {
    local t rc=0
    t="$(mktemp -d)"
    trap 'chmod -R u+w "$t" 2>/dev/null; rm -rf "$t"' RETURN
    git -C "$t" init -q
    printf 'build/\n' > "$t/.gitignore"
    mkdir -p "$t/build/corrosion-cargo/metadata-probe/key/x" "$t/build/metadata-probe/h"
    head -c 4096 /dev/zero > "$t/build/metadata-probe/h/f"
    ln -s ../build/corrosion-cargo/metadata-probe/key "$t/link"
    GITHUB_STEP_SUMMARY='' NROS_DISK_TRANSCRIPT='' evict "$t" > /dev/null
    verify "$t" > /dev/null || { echo "selftest: verify failed on a clean eviction" >&2; return 1; }
    # A reader through a workspace's symlink, and a writer, must both fail.
    if ls "$t/link/" > /dev/null 2>&1; then echo "selftest: a read through the evicted path SUCCEEDED" >&2; return 1; fi
    if mkdir -p "$t/build/metadata-probe/again" 2> /dev/null; then echo "selftest: a write under the evicted path SUCCEEDED" >&2; return 1; fi
    # A run stage that replaced the sentinel must fail verify.
    chmod u+w "$t/build/metadata-probe"; rm -f "$t/build/metadata-probe"; mkdir "$t/build/metadata-probe"
    verify "$t" > /dev/null 2>&1 && rc=1
    [ "$rc" -eq 0 ] || { echo "selftest: verify PASSED a replaced sentinel" >&2; return 1; }
    echo "evict-build-stage-only selftest: OK (reads and writes under an evicted path fail; a replaced sentinel fails verify)"
}

root="$(git rev-parse --show-toplevel)"
case "${1:-}" in
    evict) selftest > /dev/null; evict "$root" ;;
    verify) verify "$root" ;;
    selftest) selftest ;;
    *) echo "usage: $0 evict|verify|selftest" >&2; exit 2 ;;
esac

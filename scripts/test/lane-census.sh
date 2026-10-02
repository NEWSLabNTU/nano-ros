#!/usr/bin/env bash
# phase-475 W1 — run every `nros-tests` target in a LANE'S OWN ENVIRONMENT and
# record, per target, whether it reaches a verdict there.
#
# Why a census and not an inference: a target's preconditions are written four
# ways (a library probe, a helper module that probes, a test-local probe, an
# inline `if … { skip! }`), and only the environment can say which hold.
# Measured on ci-base: ~878 cases over 167 targets run in under 25 s with no
# fixtures staged, because an unmet precondition ends a test in milliseconds.
#
# Usage:
#   scripts/test/lane-census.sh <image>  [out-dir] [--runs N] [--admit FILE]
#   scripts/test/lane-census.sh --here   [out-dir] [--runs N] [--admit FILE]
#
# `<image>`: the checkout is mounted READ-ONLY and copied inside the container,
# so nothing root-owned reaches the host. A linked worktree's `.git` is a FILE
# naming the main checkout's gitdir; that gitdir is mounted at its own absolute
# path, or every `git ls-files`-based test fails for a reason that is about the
# census and not the code (measured: 6 of the first run's 11 FAILs).
# `--here`: run in this environment, for a CI job that already IS the image.
#
# `--admit FILE` writes the targets that PASSED IN EVERY RUN, the lane's
# admission list (W3). Provisions only the gate job's CLI build; the real lane
# provisions more, so the admitted set is a LOWER BOUND.
set -uo pipefail
root="$(git rev-parse --show-toplevel)"
mode="${1:?usage: lane-census.sh <image>|--here [out-dir] [--runs N] [--admit FILE]}"; shift
out="$PWD/tmp/lane-census"; runs=1; admit=""
while [ $# -gt 0 ]; do
    case "$1" in
        --runs) runs="$2"; shift 2 ;;
        --admit) admit="$2"; shift 2 ;;
        *) out="$1"; shift ;;
    esac
done
mkdir -p "$out"
rm -f "$out"/junit-*.xml

if [ "$mode" = "--here" ] && [ -n "$admit" ] && [ -z "${GITHUB_ACTIONS:-}" ]; then
    # Measured: the same tree admits 53 targets on a provisioned dev host and 47
    # in the gate image — the host has a launch resolver, QEMU and more. An
    # admission list is a statement about the LANE's environment, so one made
    # here would admit targets that skip in the lane (and W4 would then fail
    # it). Allowed, because this may BE the lane's image; said, because usually
    # it is not.
    echo "lane-census: --here --admit outside CI: this list describes THIS" >&2
    echo "  environment. The committed gate list must come from the gate image:" >&2
    echo "      scripts/test/lane-census.sh <gate-image> --runs 2 --admit .config/lane-admission/gate.txt" >&2
fi
if [ "$mode" = "--here" ]; then
    (cd "$root" && NROS_CENSUS_RUNS="$runs" bash scripts/test/lane-census-run.sh "$out")
    rc=$?
else
    common="$(git rev-parse --path-format=absolute --git-common-dir)"
    name="nros-lane-census-$$"
    trap 'docker rm -f "$name" >/dev/null 2>&1 || true' EXIT
    docker run --name "$name" -e NROS_CENSUS_RUNS="$runs" \
        -v "$root":/src:ro -v "$common":"$common":ro \
        --entrypoint bash "$mode" -lc '
            cp -a /src /work && cd /work && rm -rf target packages/cli/target build
            git config --global --add safe.directory "*"
            bash scripts/test/lane-census-run.sh /out'
    rc=$?
    [ "$rc" -eq 0 ] && docker cp "$name":/out/. "$out/" >/dev/null
fi
[ "$rc" -eq 2 ] && exit 2
set -- "$out"/junit-*.xml
[ -e "$1" ] || { echo "census: no junit produced" >&2; exit 2; }
python3 "$root/scripts/test/lane-census-classify.py" --out "$out/per-target.json" \
    ${admit:+--admit "$admit"} "$@"

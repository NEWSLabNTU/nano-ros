#!/usr/bin/env bash
# phase-591 W3 -- build a lane's fixture rows on N runners, hand them to one
# cells job. ONE mechanism for every lane that builds many independent rows
# (live-peer's host job first; nightly's platform jobs next, phase-591 W5).
#
#   fixture-shards.sh select <units-file> <weights-file> <k>/<n>
#   fixture-shards.sh pack   <out.tar.zst> <k>/<n> <paths-file> [<units-file>]
#   fixture-shards.sh unpack <n> <in.tar.zst>...
#   fixture-shards.sh pin-mtimes
#   fixture-shards.sh self-test
#
# WHY. A lane's fixture rows mostly build configurations no other row builds
# (issue 0616 forbids sharing those), so neither a compiler cache nor keeping
# build dirs between rows saves much: measured on live-peer's host job,
# sccache made it slower (#4750) and keeping class dirs could skip at most 11
# of ~110 core-crate builds. The rows ARE independent, so they shard.
#
# select     The unit ids (one per line, `<kind>:<id>`, no spaces) this shard
#            builds. `scripts/build/gate-shard.py` does the split: greedy by
#            measured weight, every unit in exactly one shard, a unit missing
#            from the weights gets the median. Weights only decide balance,
#            never coverage. Building a unit is the LANE's own command, under
#            the lane's own env: this script never builds anything.
#
# pack       Tars the paths a shard's cells will read (the lane derives the
#            list; this script globs nothing) with their modes and mtimes,
#            which actions/upload-artifact drops. It adds a manifest naming
#            the commit, the shard and the units.
#
# unpack     Extracts every shard's tar over the checkout, and REFUSES a tar
#            whose manifest names a different commit than HEAD, naming the
#            shard and both commits: an artifact built at another commit is
#            the museum-binary case (the freshness probes are mtime-based, so
#            nothing downstream would catch it). It also fails when fewer
#            than <n> shards arrived, naming the missing ones -- AFTER
#            extracting the rest, so their cells can still run (W1's rule:
#            a missing fixture is its own cells' hard failure, never a skip).
#
# pin-mtimes Sets every tracked file (superproject and initialised
#            submodules) to the commit's time. A shard and the cells job both
#            run it right after their submodule init, so on BOTH sides every
#            source is older than anything built from it. Without it the
#            cells job's fresh checkout is newer than every unpacked artifact
#            and every fixture reads STALE (CLAUDE.md's mtime treadmill).
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
MANIFEST=".nros-fixture-shard.manifest"

die() { echo "fixture-shards: $*" >&2; exit 1; }

shard_ok() { [[ "$1" =~ ^([1-9][0-9]*)/([1-9][0-9]*)$ ]] && [ "${BASH_REMATCH[1]}" -le "${BASH_REMATCH[2]}" ]; }

cmd_select() {
    local units="${1:?units file}" weights="${2:?weights file}" spec="${3:?k/n}"
    shard_ok "$spec" || die "select: '$spec' is not <k>/<n>"
    [ -f "$units" ] || die "select: no units file $units"
    python3 "$repo_root/scripts/build/gate-shard.py" "$units" "$weights" "$spec"
}

cmd_pack() {
    local out="${1:?out}" spec="${2:?k/n}" paths="${3:?paths file}" units="${4:-}"
    shard_ok "$spec" || die "pack: '$spec' is not <k>/<n>"
    [ -f "$paths" ] || die "pack: no paths file $paths"
    local man
    man="$(mktemp)"
    {
        echo "commit=$(git -C "$repo_root" rev-parse HEAD)"
        echo "shard=$spec"
        echo "run=${GITHUB_RUN_ID:-local}"
        if [ -n "$units" ] && [ -f "$units" ]; then
            sed 's/^/unit=/' "$units"
        fi
    } > "$man"
    cp "$man" "$repo_root/$MANIFEST.${spec%/*}"
    rm -f "$man"
    # Paths are repo-relative. A path that no longer exists is dropped rather
    # than failing the pack: the lane's own build already said whether it
    # built, and the cells say whether they found it.
    local list
    list="$(mktemp)"
    (cd "$repo_root" && while IFS= read -r p; do
        [ -n "$p" ] && [ -e "$p" ] && printf '%s\0' "$p"
    done < "$paths"; printf '%s\0' "$MANIFEST.${spec%/*}") > "$list"
    (cd "$repo_root" && tar --null -T "$list" -cf - | zstd -q -T0 -3 -o "$out" -f)
    rm -f "$list"
    echo "fixture-shards: packed shard $spec: $(du -h "$out" | cut -f1) ($(grep -c . "$paths") listed paths)"
}

cmd_unpack() {
    local n="${1:?shard count}"; shift
    [[ "$n" =~ ^[1-9][0-9]*$ ]] || die "unpack: '$n' is not a shard count"
    local head bad=0 got=() f k commit
    head="$(git -C "$repo_root" rev-parse HEAD)"
    for f in "$@"; do
        [ -f "$f" ] || continue
        # Read the manifest BEFORE extracting anything from this tar.
        local man
        man="$(zstd -dc "$f" | tar -xOf - --wildcards "$MANIFEST.*" 2>/dev/null)" \
            || { echo "fixture-shards: $f has no manifest -- refused" >&2; bad=1; continue; }
        commit="$(sed -n 's/^commit=//p' <<<"$man" | head -1)"
        k="$(sed -n 's/^shard=\([0-9]*\)\/.*/\1/p' <<<"$man" | head -1)"
        if [ "$commit" != "$head" ]; then
            echo "fixture-shards: shard ${k:-?} ($f) was built at $commit, this checkout is $head -- refused (its fixtures would read fresh against the wrong sources)" >&2
            bad=1
            continue
        fi
        (cd "$repo_root" && zstd -dc "$f" | tar -xpf -)
        got+=("$k")
        echo "fixture-shards: unpacked shard $k/$n ($(du -h "$f" | cut -f1))"
    done
    local missing=()
    for k in $(seq 1 "$n"); do
        case " ${got[*]-} " in *" $k "*) ;; *) missing+=("$k") ;; esac
    done
    if [ "${#missing[@]}" -gt 0 ]; then
        echo "fixture-shards: shard(s) ${missing[*]} of $n did not arrive: their rows' cells will fail as missing fixtures" >&2
        bad=1
    fi
    return "$bad"
}

cmd_pin_mtimes() {
    local t
    t="$(git -C "$repo_root" log -1 --format=%ct)"
    (cd "$repo_root" && git ls-files -z --recurse-submodules | xargs -0 -r touch -h -m -d "@$t" 2>/dev/null) || true
    echo "fixture-shards: tracked files pinned to the commit time $(date -u -d "@$t" '+%Y-%m-%dT%H:%M:%SZ')"
}

cmd_self_test() {
    local work fails=0
    # The self-test builds a throwaway repository: issue 0986's rule, so an
    # inherited GIT_DIR (a hook, a linked worktree) cannot aim it at the caller.
    # shellcheck source=scripts/lib/git-hook-env.sh
    source "$repo_root/scripts/lib/git-hook-env.sh"
    nros_clear_inherited_git_env
    work="$(mktemp -d)"
    trap 'rm -rf "$work"' RETURN
    (
        set -e
        cd "$work"
        git init -q repo && cd repo
        git config user.email t@t && git config user.name t
        mkdir -p scripts/ci scripts/build out
        cp "$repo_root/scripts/ci/fixture-shards.sh" scripts/ci/
        cp "$repo_root/scripts/build/gate-shard.py" scripts/build/
        echo src > a.c && git add a.c scripts && git commit -qm one
        printf 'ws:a\nws:b\nfb:linux,c,zenoh\ncc:x\n' > units
        printf 'ws:a 100\nws:b 90\nfb:linux,c,zenoh 10\ncc:x 10\n' > weights
        s1="$(bash scripts/ci/fixture-shards.sh select units weights 1/2)"
        s2="$(bash scripts/ci/fixture-shards.sh select units weights 2/2)"
        [ "$(printf '%s\n%s\n' "$s1" "$s2" | sort)" = "$(sort units)" ] || { echo "FAIL select: not a partition"; exit 1; }
        echo bin1 > out/one && chmod +x out/one && echo bin2 > out/two
        echo out/one > p1 && echo out/two > p2
        bash scripts/ci/fixture-shards.sh pack s1.tar.zst 1/2 p1 >/dev/null
        bash scripts/ci/fixture-shards.sh pack s2.tar.zst 2/2 p2 >/dev/null
        rm -f out/one out/two .nros-fixture-shard.manifest.*
        bash scripts/ci/fixture-shards.sh unpack 2 s1.tar.zst s2.tar.zst >/dev/null 2>&1 || { echo "FAIL unpack: current commit refused"; exit 1; }
        [ -x out/one ] || { echo "FAIL unpack: exec bit lost"; exit 1; }
        # Negative control: a shard built at another commit is refused.
        echo more > a.c && git commit -qam two
        rm -f out/one
        if bash scripts/ci/fixture-shards.sh unpack 1 s1.tar.zst >/dev/null 2>&1; then
            echo "FAIL unpack: a shard built at the previous commit was accepted"; exit 1
        fi
        [ ! -e out/one ] || { echo "FAIL unpack: refused shard was still extracted"; exit 1; }
        # A missing shard fails, after extracting the ones that came.
        echo bin1 > out/one && chmod +x out/one
        bash scripts/ci/fixture-shards.sh pack s1b.tar.zst 1/2 p1 >/dev/null
        rm -f out/one
        if bash scripts/ci/fixture-shards.sh unpack 2 s1b.tar.zst >/dev/null 2>&1; then
            echo "FAIL unpack: a missing shard passed"; exit 1
        fi
        [ -e out/one ] || { echo "FAIL unpack: the shard that came was not extracted"; exit 1; }
        # pin-mtimes: tracked sources end older than an artifact built after.
        sleep 1; echo built > out/new
        bash scripts/ci/fixture-shards.sh pin-mtimes >/dev/null
        [ out/new -nt a.c ] || { echo "FAIL pin-mtimes: the source is not older than the artifact"; exit 1; }
    ) || fails=1
    if [ "$fails" = 0 ]; then
        echo "fixture-shards self-test: OK (partition, exec bits, old-commit refusal, missing shard, pinned mtimes)"
    else
        echo "fixture-shards self-test: FAILED" >&2
        return 1
    fi
}

case "${1:-}" in
    select) shift; cmd_select "$@" ;;
    pack) shift; cmd_pack "$@" ;;
    unpack) shift; cmd_unpack "$@" ;;
    pin-mtimes) cmd_pin_mtimes ;;
    self-test) cmd_self_test ;;
    *) sed -n '2,12p' "$0" >&2; exit 2 ;;
esac

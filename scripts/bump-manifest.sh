#!/usr/bin/env bash
# Phase 378 W1 — move every `ros-launch-manifest` pin to one tag, atomically.
#
# Four manifests across TWO cargo workspaces pin this crate, and bumping a
# subset does not fail in a way that names the cause. It fails like this:
#
#     expected `TierDef`, found `ros_launch_manifest_model::TierDef`
#       .../checkouts/ros-launch-manifest-<hash>/1a53088/sched/src/types.rs:23
#       .../checkouts/ros-launch-manifest-<hash>/ce0b918/sched/src/types.rs:23
#
# Two revisions of one crate resolve as two same-named, incompatible types, and
# the compiler points at a type mismatch rather than at the pin that caused it.
# So the unit of work is ALL pins or NONE: this script backs up every file it
# will touch and restores them on any failure. A half-bump is the state it
# exists to make unrepresentable.
#
# The manifest list is DISCOVERED, never hardcoded — a fifth pin added later is
# picked up automatically. It keys on a dependency KEY at line start
# (`^ros-launch-manifest... =`), not a bare mention: two manifests discuss this
# crate in comments and must not be rewritten.
#
# Usage:  just bump-manifest v0.1.12
#         just bump-manifest v0.1.12 --dry-run
set -euo pipefail

# Issue 0726 — `grep -q` cannot distinguish "no match" from "grep failed to
# run", and the two want opposite handling. Here a failed grep would report the
# tag as ABSENT and refuse a bump that was perfectly valid.
# shellcheck source=scripts/lib/grep-q.sh
. "$(dirname "${BASH_SOURCE[0]}")/lib/grep-q.sh"

TAG="${1:-}"
DRY_RUN="${2:-}"
if [ -z "$TAG" ]; then
    echo "usage: just bump-manifest <tag> [--dry-run]" >&2
    echo "  e.g. just bump-manifest v0.1.12" >&2
    exit 2
fi

ROOT="$(git rev-parse --show-toplevel)"
cd "$ROOT"

# --- discover the pinned manifests -----------------------------------------
mapfile -t MANIFESTS < <(git ls-files '*Cargo.toml' \
    | xargs grep -lE '^ros-launch-manifest[a-z-]* *=' 2>/dev/null | sort)
if [ "${#MANIFESTS[@]}" -eq 0 ]; then
    echo "ERROR: no manifest pins ros-launch-manifest — has the dep been removed?" >&2
    exit 1
fi

# The remote is read from the manifests too, so this script has no URL of its own.
URL="$(grep -hoE 'https://[^"]*ros-launch-manifest[^"]*\.git' "${MANIFESTS[@]}" | sort -u | head -1)"
if [ -z "$URL" ]; then
    echo "ERROR: could not read the git URL out of the manifests" >&2
    exit 1
fi

CURRENT="$(grep -hoE 'tag = "[^"]*"' "${MANIFESTS[@]}" | sort -u | sed 's/tag = //;s/"//g' | tr '\n' ' ')"
echo "ros-launch-manifest: $URL"
echo "  manifests : ${#MANIFESTS[@]}"
for m in "${MANIFESTS[@]}"; do echo "      $m"; done
echo "  current   : $CURRENT"
echo "  requested : $TAG"

# --- validate the tag on the REMOTE before touching anything ---------------
# The whole point of refusing early: a typo'd tag must change nothing at all,
# rather than rewriting four manifests and failing at `cargo update`.
remote_tags="$(git ls-remote --tags "$URL" "refs/tags/$TAG" 2>/dev/null || true)"
if ! nros_grep_q "refs/tags/$TAG" <<<"$remote_tags"; then
    echo "ERROR: tag '$TAG' does not exist on $URL — nothing changed." >&2
    echo "  available (most recent):" >&2
    git ls-remote --tags "$URL" 2>/dev/null \
        | sed 's#.*refs/tags/##' | grep -v '\^{}' | sort -V | tail -8 | sed 's/^/      /' >&2
    exit 1
fi
echo "  tag exists on remote: OK"

if [ "$DRY_RUN" = "--dry-run" ]; then
    echo "[dry-run] would rewrite ${#MANIFESTS[@]} manifest(s) to $TAG and refresh their lockfiles"
    exit 0
fi

# --- work out which workspaces own those manifests -------------------------
# Derived with `cargo locate-project --workspace`, never a path-prefix test:
# `packages/cli` is a separate workspace INSIDE this repo (issue 0616's rule).
declare -A WORKSPACES=()
for m in "${MANIFESTS[@]}"; do
    ws="$(cd "$(dirname "$m")" && cargo locate-project --workspace --message-format plain 2>/dev/null || true)"
    [ -n "$ws" ] && WORKSPACES["$(dirname "$ws")"]=1
done
if [ "${#WORKSPACES[@]}" -eq 0 ]; then
    echo "ERROR: could not resolve a workspace for any pinned manifest" >&2
    exit 1
fi

# --- back up everything we may modify --------------------------------------
BACKUP="$(mktemp -d)"
cleanup_ok=0
restore() {
    if [ "$cleanup_ok" -eq 0 ]; then
        echo "" >&2
        echo "RESTORING — no file is left half-bumped." >&2
        (cd "$BACKUP" && find . -type f -print0) | while IFS= read -r -d '' f; do
            cp "$BACKUP/${f#./}" "$ROOT/${f#./}"
        done
    fi
    rm -rf "$BACKUP"
}
trap restore EXIT

backup_file() { mkdir -p "$BACKUP/$(dirname "$1")"; cp "$1" "$BACKUP/$1"; }
for m in "${MANIFESTS[@]}"; do backup_file "$m"; done
for ws in "${!WORKSPACES[@]}"; do
    rel="${ws#"$ROOT"/}"; [ "$rel" = "$ws" ] && rel=""
    lock="${rel:+$rel/}Cargo.lock"
    [ -f "$lock" ] && backup_file "$lock"
done

# --- rewrite the pins ------------------------------------------------------
# Only on lines that are a ros-launch-manifest dependency, so a `tag = ` for any
# other git dep in the same file is untouched.
for m in "${MANIFESTS[@]}"; do
    sed -i -E '/^ros-launch-manifest[a-z-]* *=/ s/tag = "[^"]*"/tag = "'"$TAG"'"/' "$m"
done
echo "  rewrote ${#MANIFESTS[@]} manifest(s)"

# --- refresh each workspace's lock -----------------------------------------
# `NROS_CARGO_FLAGS=` drops the project-wide `--locked` that the `scripts/bin/cargo`
# PATH shim injects; without that, updating a lock is exactly what cargo refuses.
# `-p <crate>` keeps this to the crate named, never a whole-graph re-resolve
# (issue 0359: a bare `generate-lockfile` once moved 5388 lines).
for ws in "${!WORKSPACES[@]}"; do
    echo "  refreshing lock in ${ws#"$ROOT"/}"
    for crate in ros-launch-manifest-types ros-launch-manifest-model ros-launch-manifest-sched; do
        (cd "$ws" && NROS_CARGO_FLAGS= cargo update -p "$crate" >/dev/null 2>&1) || true
    done
    # A pin move can add or drop a member crate, which `-p` on a name that is no
    # longer in the graph cannot express; this settles the rest without
    # re-resolving unrelated packages.
    (cd "$ws" && NROS_CARGO_FLAGS= cargo update --workspace >/dev/null 2>&1) || true
done

# --- verify: every lock names exactly ONE revision, and it is the one asked for
fail=0
CHECKED_LOCKS=()
for ws in "${!WORKSPACES[@]}"; do
    rel="${ws#"$ROOT"/}"; [ "$rel" = "$ws" ] && rel=""
    lock="${rel:+$rel/}Cargo.lock"
    [ -f "$lock" ] || continue
    CHECKED_LOCKS+=("$lock")
    tags="$(grep -oE 'ros-launch-manifest[a-z-]*\.git\?tag=[^#"]*' "$lock" | sed 's/.*tag=//' | sort -u)"
    revs="$(grep -oE 'ros-launch-manifest[a-z-]*\.git\?tag=[^"]*#[a-f0-9]+' "$lock" | sed 's/.*#//' | sort -u)"
    n_tags="$(printf '%s\n' "$tags" | grep -c . || true)"
    n_revs="$(printf '%s\n' "$revs" | grep -c . || true)"
    if [ "$n_tags" -ne 1 ] || [ "$n_revs" -ne 1 ]; then
        echo "ERROR: $lock names $n_tags tag(s) and $n_revs revision(s); expected exactly 1 of each." >&2
        printf '%s\n' "$tags" | sed 's/^/      tag /' >&2
        fail=1
    elif [ "$tags" != "$TAG" ]; then
        echo "ERROR: $lock resolved to '$tags', not the requested '$TAG'." >&2
        fail=1
    else
        echo "  $lock: $tags @ ${revs:0:12} — single revision OK"
    fi
done
[ "$fail" -eq 0 ] || exit 1

# --- advisory: locks that hold an rlm revision but pin no manifest ----------
# `nros-launch-resolve` reaches rlm TRANSITIVELY through play_launch's layer 2
# and keeps its own lock, deliberately ("stays robust if the two ever diverge on
# a pin"). So it is out of scope for a rewrite — but staying SILENT about it
# would let this script report a clean single-revision bump while another lock
# in the tree names a different one. Report, do not fail: divergence there is a
# design choice, and an unreported cap reads as coverage.
for lock in $(git ls-files '*Cargo.lock'); do
    case " ${CHECKED_LOCKS[*]} " in *" $lock "*) continue ;; esac
    other="$(grep -oE 'ros-launch-manifest[a-z-]*\.git\?tag=[^#"]*' "$lock" 2>/dev/null | sed 's/.*tag=//' | sort -u || true)"
    # A plain `[ ] && [ ] && echo` here would be the LAST command in the loop
    # body, so its false case would fail the whole script under `set -e`.
    if [ -n "$other" ] && [ "$other" != "$TAG" ]; then
        echo "  note: $lock holds rlm $other (transitive; not rewritten by this tool)"
    fi
done

cleanup_ok=1
echo ""
echo "bumped to $TAG. REVIEW before committing:"
echo "    git diff -- '*Cargo.toml' '*Cargo.lock'"
echo "Then build both workspaces — a pin move that compiles is not the same as one that is correct."

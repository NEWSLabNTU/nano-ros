#!/usr/bin/env bash
# phase-343 W3 — the collision gate detects an unhashed artifact collision.
#
# `check-fixture-groups.py` is the thing standing between a shared cargo target
# dir and a green test running the WRONG binary. Two rows in one group that emit
# the same unhashed artifact name overwrite each other, last writer wins, and the
# resolver hands one row's test the other row's artifact.
#
# The gate has been widened twice, and both times the widening was verified by
# hand and the verification survived only in a commit message:
#
#   phase-340 W1  owners keyed on the ROW, not the leaf directory — without it
#                 `examples/native/rust/talker`'s four rows dedup to one owner
#                 and 11 real `linux` collisions report as zero.
#   phase-340 B1  LIB artifacts included — staticlib/cdylib/dylib land flat and
#                 unhashed. `libnros_c.a` exists at 438 copies across ~30
#                 distinct sizes; the gate had been reporting "no collisions"
#                 over a namespace it was not looking at.
#
# A gate whose only proof of life is prose in a commit is a gate nobody can
# re-check. This asserts both arms mechanically, on perturbed leaf manifests, so
# a future narrowing of `artifacts()` or `collisions()` fails HERE rather than in
# whichever migration first trusts a false "no collisions".
#
#   T1  a deliberately re-collided BINARY name must be REPORTED
#   T2  a deliberately re-collided STATICLIB name must be REPORTED — the arm B1
#       added, and the one a narrowing would silently drop
#   T3  the real tree must PASS with an empty record, so T1/T2 are evidence the
#       gate discriminates rather than evidence it is stuck red
#
# # A non-zero exit is NOT evidence
#
# The first draft of this file asserted only `rc != 0`, and T2 passed with B1
# reverted — because appending a second `[lib]` to a leaf that already has one
# is invalid TOML, so the gate died in `tomllib.load` with rc=1 and the
# assertion could not tell a crash from a detection. Both halves are fixed here:
# the perturbation EDITS the existing table instead of appending a second one,
# and every expectation matches the gate's MESSAGE for the artifact name it was
# supposed to find. Verified by reverting B1 and confirming T2 then fails.
set -uo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../../.." && pwd)"
cd "$repo_root"

# Issue 0726 — `expect_report` below concludes "the gate never mentioned
# <artifact>, so it did not detect this collision" from a `grep -q` that cannot
# tell rc 1 from rc>=2. Under the gate fan-out that verdict is reachable with
# the collision detected and the message printed. `nros_grep_q` exits 2 on
# rc>=2 rather than returning a status the caller reads as a finding.
# shellcheck source=../../../../scripts/lib/grep-q.sh
. "$repo_root/scripts/lib/grep-q.sh"

gate="scripts/check-fixture-groups.py"
collider="tripwire_collider"
fail=0

scratch="$(mktemp -d)"
restore_list="$scratch/restore"
: > "$restore_list"

restore_all() {
    if [ -f "$restore_list" ]; then
        while IFS=$'\t' read -r saved orig; do
            [ -f "$saved" ] && cp -p "$saved" "$orig"
        done < "$restore_list"
    fi
    : > "$restore_list"
}
trap 'restore_all; rm -rf "$scratch"' EXIT

# perturb <path> — back it up so `restore_all` puts it back.
perturb() {
    local orig="$1"
    local saved="$scratch/$(echo "$orig" | tr '/' '_')"
    cp -p "$orig" "$saved"
    printf '%s\t%s\n' "$saved" "$orig" >> "$restore_list"
}

# The platform A1 actually checks. Read from the file that owns the eligibility
# list rather than hardcoded: an arm keyed on a platform that has since left the
# shared list would pass vacuously.
shared_platform="$(bash -c '. scripts/build/fixtures-target-dir.sh
printf "%s" "$NROS_FIXTURE_SHARED_PLATFORMS"' | awk '{print $1}')"
if [ -z "$shared_platform" ]; then
    echo "  FAIL no shared platform in NROS_FIXTURE_SHARED_PLATFORMS —" \
         "A1 checks nothing and this tripwire would pass vacuously" >&2
    exit 1
fi

mapfile -t leaves < <(
    python3 scripts/build/fixtures-manifest.py list --lang rust --with-platform \
    | awk -F'\x1f' -v p="$shared_platform" '$1 == p { print $2 }' \
    | sort -u
)
if [ "${#leaves[@]}" -lt 2 ]; then
    echo "  FAIL platform $shared_platform has ${#leaves[@]} rust leaf dir(s);" \
         "a collision needs two" >&2
    exit 1
fi
a="${leaves[0]}"
b="${leaves[1]}"
echo "  .... platform $shared_platform, colliding $a against $b"

# expect_report <artifact> <label> — the gate must FAIL *and say why*.
#
# Matching the message is the whole point: rc alone cannot distinguish "found
# the collision" from "crashed before looking", and a crash is exactly what a
# malformed perturbation produces.
expect_report() {
    local artifact="$1" label="$2" out rc
    out="$(python3 "$gate" 2>&1)"
    rc=$?
    if [ "$rc" -eq 0 ]; then
        echo "  FAIL $label — gate PASSED on a tree containing the collision"
        echo "        gate said: $out"
        fail=1
        return
    fi
    # NOT `printf … | nros_grep_q`: a pipeline runs the helper in a SUBSHELL,
    # where its `exit 2` ends only that pipeline segment and hands the caller
    # back the exact rc it exists to remove. A herestring keeps it in-process.
    if ! nros_grep_q "$artifact" <<<"$out"; then
        echo "  FAIL $label — gate failed but never mentioned '$artifact', so it"
        echo "        did not detect this collision (a crash exits non-zero too)"
        echo "        gate said: $out"
        fail=1
        return
    fi
    echo "  ok   $label"
}

expect_clean() { # <label>
    local label="$1" out rc
    out="$(python3 "$gate" 2>&1)"
    rc=$?
    if [ "$rc" -ne 0 ]; then
        echo "  FAIL $label — gate FAILED on the unperturbed tree"
        echo "        gate said: $out"
        fail=1
        return
    fi
    echo "  ok   $label"
}

# --- T3 first: the real tree passes -------------------------------------
#
# Deliberately BEFORE the perturbations. If the tree is already red, T1 and T2
# would report "ok" for the wrong reason and this file would claim a working
# gate while proving nothing.
expect_clean "T3 real tree passes with an empty collision record"
if [ "$fail" -ne 0 ]; then
    echo "fixture_group_collision_gate: baseline is red — refusing to draw" \
         "conclusions from the tripwire arms" >&2
    exit 1
fi

# --- T1: two rows claiming one BINARY name ------------------------------
#
# `[[bin]]` is an array of tables, so an extra one is valid TOML on any leaf.
perturb "$a/Cargo.toml"
perturb "$b/Cargo.toml"
for f in "$a/Cargo.toml" "$b/Cargo.toml"; do
    printf '\n[[bin]]\nname = "%s"\npath = "src/main.rs"\n' "$collider" >> "$f"
done
expect_report "$collider" "T1 two rows claiming one BINARY name are reported"
restore_all

# --- T2: two rows claiming one STATICLIB name (the B1 arm) --------------
#
# EDIT the existing `[lib]` rather than appending a second one: these leaves
# already declare `[lib] crate-type = ["rlib"]`, and a duplicate table is a
# TOMLDecodeError, which the gate reports as rc=1 with no collision found.
perturb "$a/Cargo.toml"
perturb "$b/Cargo.toml"
python3 - "$collider" "$a/Cargo.toml" "$b/Cargo.toml" <<'PY'
import sys

name, paths = sys.argv[1], sys.argv[2:]

def set_lib(path):
    """Force `[lib] name = <collider>, crate-type = ["staticlib", "rlib"]`.

    Line-oriented on purpose: tomllib reads but cannot write, and pulling in a
    TOML writer for a tripwire would add a dependency the gate itself does not
    have. Rewrites the `[lib]` table in place when present, appends it when not.
    """
    lines = open(path).read().splitlines()
    try:
        start = next(i for i, l in enumerate(lines) if l.strip() == "[lib]")
    except StopIteration:
        lines += ["", "[lib]", f'name = "{name}"',
                  'crate-type = ["staticlib", "rlib"]']
        open(path, "w").write("\n".join(lines) + "\n")
        return
    end = len(lines)
    for i in range(start + 1, len(lines)):
        if lines[i].lstrip().startswith("["):
            end = i
            break
    body = [f'name = "{name}"', 'crate-type = ["staticlib", "rlib"]']
    open(path, "w").write("\n".join(lines[:start + 1] + body + lines[end:]) + "\n")

for p in paths:
    set_lib(p)
PY
expect_report "lib${collider}.a" \
    "T2 two rows claiming one STATICLIB name are reported (phase-340 B1 arm)"
restore_all

# --- T3 again: the restore actually restored ----------------------------
#
# Not ceremony. These scenarios edit REAL leaf manifests, and a restore that
# silently failed would leave the tree carrying a `[lib]` nobody authored,
# surfacing later as a mystery fixture-build failure.
expect_clean "T3' tree is clean again after the perturbations"

if [ "$fail" -ne 0 ]; then
    echo "fixture_group_collision_gate: FAILED" >&2
    exit 1
fi
echo "fixture_group_collision_gate: all checks passed"

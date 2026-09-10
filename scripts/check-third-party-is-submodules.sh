#!/usr/bin/env bash
# `third-party/` holds TRACKED SUBMODULES and nothing else (phase-440 W3).
#
# WHY THIS EXISTS
#
# The directory carried two kinds of thing at once: 20 tracked submodules whose
# pins are DECISIONS (cyclonedds tracks the version ROS ships; moving it is how
# you stop interoperating), and gitignored provisioning that `nros setup` writes.
# "Is this pinned source or provisioning?" then had no answer from the path — a
# reader had to consult `.gitmodules` AND `.gitignore` to tell them apart, and
# every consumer that walked the directory had to encode the distinction itself.
#
# That is also what made the box mirror unfixable (issue 1248): its rules told
# source from build output BY NAME, in directories where the two lived together.
#
# `make/` and `ninja/` are gone — both are provisioned into the store and their
# consumers already read it there ("the store rather than `third-party/make/`",
# jobserver-pool.sh). The residue was 1.2 MB of nothing.
#
# THE ONE DECLARED EXCEPTION, and why it is not fixed here
#
# `[source.rosidl]` still has `dest = "third-party/ros/rosidl"`, and
# `msg_to_cyclone_idl.py` resolves it as its last ladder rung so the cyclone
# msg->IDL step works with no ROS install. Moving it needs `dest` to be able to
# name the STORE, which is RFC-0095 D2/D4 and lands with phase-440 W4. Declaring
# it here rather than deleting the rule keeps the ratchet honest: the exception
# is visible, has a reason, and has somewhere to go.
#
# The list may only SHRINK. A new name here is a new provisioning root inside a
# directory that is supposed to have none.
set -uo pipefail

# `grep -q` cannot tell "no match" (1) from "grep failed" (>=2), and the two
# natural spellings fail in OPPOSITE directions (issue 0726). One helper.
# shellcheck source=scripts/lib/grep-q.sh
. "$(dirname "${BASH_SOURCE[0]}")/lib/grep-q.sh"

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd -P)"
cd "$repo_root" || exit 2

# EMPTY, and that is the point. `ros` was the one declared exception: its
# `[source.rosidl]` could only name a workspace-relative `dest`, so it had
# nowhere else to go. `dest` can name the STORE now (phase-440), rosidl went
# there, and the rule holds with no exemption. A name reappearing here is a new
# provisioning root inside a directory that is supposed to have none.
EXCEPTIONS=""

# Submodule PARENTS: the first path component under third-party/ for every
# declared submodule. Read from .gitmodules, never from a directory walk, so an
# uninitialised submodule still counts as one (a bare clone has empty dirs).
parents="$(git config -f .gitmodules --get-regexp '^submodule\..*\.path$' 2>/dev/null \
    | awk '{print $2}' | grep '^third-party/' | cut -d/ -f2 | sort -u)"

if [ -z "$parents" ]; then
    echo "check-third-party-is-submodules: no submodules declared under third-party/ —" >&2
    echo "  refusing to report OK over a reading that found nothing." >&2
    exit 2
fi

# ONE classification, called by the loop AND the selftest. Two copies would let
# the halves disagree: with the predicate inlined in the loop, mutating it to a
# substring match left the selftest green (measured), so the selftest was
# proving `nros_grep_q` behaves rather than that this gate uses it.
is_parent()    { nros_grep_q -x -- "$1" <<< "$2"; }
is_exception() { nros_grep_q -w -- "$1" <<< "$2"; }

# SELFTEST, called unconditionally below — not behind a flag. A selftest nobody
# runs decays into a comment (check-gate-selftests). These drive the same two
# predicates the loop uses, over synthetic inputs, so a refactor that breaks
# classification fails HERE rather than passing silently over a real tree.
selftest() {
    local fail=0
    _st() { # name, want, got
        [ "$2" = "$3" ] || { echo "  selftest FAIL: $1 (want $2, got $3)" >&2; fail=1; }
    }
    local parents=$'dds\nfreertos\nnuttx'
    _st "a submodule parent is recognised"      0 "$(is_parent dds    "$parents"; echo $?)"
    _st "a non-parent is not"                   1 "$(is_parent newsdk "$parents"; echo $?)"
    _st "a PREFIX of a parent is not a parent"  1 "$(is_parent dd     "$parents"; echo $?)"
    _st "a declared exception is recognised"    0 "$(is_exception ros    "ros"; echo $?)"
    _st "an undeclared name is not exempt"      1 "$(is_exception notros "ros"; echo $?)"
    return "$fail"
}

selftest || {
    echo "check-third-party-is-submodules: SELFTEST FAILED — not reporting on the tree" >&2
    exit 2
}

problems=0
for d in third-party/*/; do
    [ -d "$d" ] || continue
    name="$(basename "$d")"
    is_parent    "$name" "$parents"    && continue
    is_exception "$name" "$EXCEPTIONS" && continue
    problems=$((problems + 1))
    echo "check-third-party-is-submodules: third-party/$name is not a submodule parent" >&2
    echo "    $(du -sh "$d" 2>/dev/null | cut -f1) — provisioning belongs in the store" >&2
    echo "    (\$NROS_STORE), not beside pinned source. RFC-0095 D2/D3." >&2
done

# A declared exception that no longer exists is a stale rule, and a stale rule
# protects nothing while reading as though it does.
for e in $EXCEPTIONS; do
    [ -d "third-party/$e" ] || {
        echo "check-third-party-is-submodules: '$e' is declared an exception but" >&2
        echo "    third-party/$e does not exist here — if it has moved to the store," >&2
        echo "    delete it from EXCEPTIONS (the list may only shrink)." >&2
        # Not a failure: the directory is provisioned, so its absence is normal
        # on a host that has not run `nros setup --source rosidl`.
        :
    }
done

[ "$problems" -eq 0 ] || exit 1
echo "check-third-party-is-submodules: OK ($(wc -l <<< "$parents") submodule parent(s), $(wc -w <<< "$EXCEPTIONS") declared exception(s))"

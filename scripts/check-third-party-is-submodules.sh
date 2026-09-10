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
# `make/` and `ninja/` are gone — both are provisioned into the store, and their
# consumers read it there. That sentence used to end "already", and it was not
# true: `build-all`'s gate in `justfile` kept reading `third-party/make/make` and
# `third-party/ninja/ninja` after the move, so every migrated host silently lost
# the jobserver path. A directory rule whose READERS are unchecked has a reach
# narrower than the rule (0196's shape) — hence the reference scan below.
#
# THE DECLARED EXCEPTION IS GONE
#
# `ros` was one: `[source.rosidl]` could only name a workspace-relative `dest`.
# phase-440 let `dest` name the STORE, rosidl moved there, and `EXCEPTIONS` is
# empty (see the note beside it). Its one READER keeps a documented legacy
# fallback, which the reference scan declares separately in `REF_EXCEPTIONS`.
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

# Provisioning roots moved OUT of third-party/: make, ninja, ros (RFC-0095 D3) and
# zenoh (RFC-0075). The directory scan refuses them as DIRECTORIES; the reference
# scan refuses CODE that still reads them.
RETIRED="make ninja ros zenoh"
# `third-party/<root>` with nothing path-like before it (so the
# `packages/cli/third-party/...` tree is not this) and a non-name character or
# end-of-line after it (so `ros-launch-manifest` is not `ros`). No `\b`: it is
# zero-width, ugrep rejects it outright, and behind a `2>/dev/null` that is a
# scan reporting "no references" over a tree full of them — measured while this
# rule was being written.
RETIRED_PAT="(^|[^/A-Za-z0-9_.-])third-party/($(tr ' ' '|' <<< "$RETIRED"))([^A-Za-z0-9_.-]|\$)"

# Files allowed to READ a retired root, each with its reason. May only SHRINK,
# and a listed file that no longer reads one is a STALE entry and fails.
#   scripts/cyclonedds/msg_to_cyclone_idl.py — resolves the STORE first
#     (`nros sdk-path --source rosidl`) and keeps the pre-phase-440 location
#     BELOW it, so an earlier-provisioned host keeps working with no migration
#     step. A documented fallback, not a reader that missed the move.
REF_EXCEPTIONS="scripts/cyclonedds/msg_to_cyclone_idl.py"

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
# A CODE line that reads a retired root. A comment is prose, not a read — the
# retirement is explained in comments in several places, correctly.
is_retired_ref() {
    local body="${1#"${1%%[![:space:]]*}"}"
    case "$body" in \#*|//*) return 1 ;; esac
    nros_grep_q -E -- "$RETIRED_PAT" <<< "$1"
}
is_ref_exception() { case " $2 " in *" $1 "*) return 0 ;; esac; return 1; }

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
    # Built, never written out, so this file does not scan as a reader itself.
    local tp="third-party"
    _st "a code read of a retired root"         0 "$(is_retired_ref "    if [ -x $tp/make/make ]; then"; echo $?)"
    _st "ninja likewise"                        0 "$(is_retired_ref "   && [ -x $tp/ninja/ninja ]; then"; echo $?)"
    _st "a path in a python string is a read"   0 "$(is_retired_ref "    root / \"$tp/ros/rosidl\""; echo $?)"
    _st "a '#' comment naming it is prose"      1 "$(is_retired_ref "    # the store rather than $tp/make/"; echo $?)"
    _st "a '//' comment likewise"               1 "$(is_retired_ref "// $tp/zenoh/zenoh is gone"; echo $?)"
    _st "packages/cli/$tp is not repo-root"     1 "$(is_retired_ref "git -C packages/cli/$tp/make x"; echo $?)"
    _st "a longer name is not the root"         1 "$(is_retired_ref "see $tp/ros-launch-manifest"; echo $?)"
    _st "a submodule parent is not retired"     1 "$(is_retired_ref "cd $tp/dds/cyclonedds"; echo $?)"
    _st "a listed file is a ref exception"      0 "$(is_ref_exception a/b.py "a/b.py c.sh"; echo $?)"
    _st "a PREFIX of a listed file is not"      1 "$(is_ref_exception a/b "a/b.py c.sh"; echo $?)"
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

# REFERENCE scan: one `git grep` over what RUNS — recipes, scripts, cmake,
# workflows. Its status is branched on like `nros_grep_q`'s: 1 is "none", and
# past 1 is a scan that did not run, which must never read as a clean tree.
rc=0
refs="$(git grep -nE "$RETIRED_PAT" -- justfile 'just/*.just' 'scripts/**' \
    'cmake/**' '.github/**')" || rc=$?
case "$rc" in
    0|1) : ;;
    *)  echo "check-third-party-is-submodules: the reference scan did not run" >&2
        echo "    (git grep exit $rc) — refusing to call the tree clean without it." >&2
        exit 2 ;;
esac
refs_seen=" "
while IFS= read -r hit; do
    [ -n "$hit" ] || continue
    file="${hit%%:*}"; rest="${hit#*:}"; line="${rest#*:}"
    is_retired_ref "$line" || continue
    if is_ref_exception "$file" "$REF_EXCEPTIONS"; then
        refs_seen="$refs_seen$file "; continue
    fi
    problems=$((problems + 1))
    echo "check-third-party-is-submodules: $file:${rest%%:*} reads a retired root" >&2
    echo "    ${line#"${line%%[![:space:]]*}"}" >&2
    echo "    The tool is in the store now — resolve it with \`nros sdk-path <tool>\`" >&2
    echo "    (see nros_pinned_make / nros_pinned_ninja). RFC-0095 D3." >&2
done <<< "$refs"
for e in $REF_EXCEPTIONS; do
    is_ref_exception "$e" "$refs_seen" && continue
    problems=$((problems + 1))
    echo "check-third-party-is-submodules: '$e' is a declared reference exception but" >&2
    echo "    no longer reads a retired root — delete it from REF_EXCEPTIONS (the list" >&2
    echo "    may only shrink)." >&2
done

[ "$problems" -eq 0 ] || exit 1
echo "check-third-party-is-submodules: OK ($(wc -l <<< "$parents") submodule parent(s), $(wc -w <<< "$EXCEPTIONS") declared exception(s); no code reads a retired root ($RETIRED) outside $(wc -w <<< "$REF_EXCEPTIONS") declared reference exception(s))"

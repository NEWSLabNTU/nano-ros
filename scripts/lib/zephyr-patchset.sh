#!/usr/bin/env bash
# Which nano-ros patch set a Zephyr west workspace carries (issue 2764).
#
# `scripts/zephyr/patches/<line>.sh` edits tracked files of west projects in
# place (zephyr's nsos_*.c, zephyr-lang-rust's CMakeLists.txt, ...). Nothing
# recorded that it had run, so a workspace that never got it, or lost it to a
# `west update` that reset those edits, looked like any other workspace until a
# configure failed: the LLEXT EDK generator-expression error on 3.7, a Rust
# image refused at configure on 4.4.
#
# Whether a workspace carries the set is asked of the TREE (issue 2831): every
# patch script has a check mode (`nros_patch_would_apply` below), and
# `check-zephyr-pin-drift.py` runs the line's set that way. The record of
# which set had run (`<ws>/.west/nros-patchset`, a digest of the scripts) is
# retired: it keyed on the scripts' bytes, not on their edits, so branches
# sharing one workspace refused each other. A workspace provisioned before
# may still hold the file; nothing reads it.
#
# Sourced. Every function returns non-zero rather than exiting, except
# `nros_patch_would_apply` in check mode, whose exit IS the verdict. It also
# brings `nros_grep_q`, which a patch script's guard uses.

# shellcheck source=scripts/lib/grep-q.sh
source "$(dirname "${BASH_SOURCE[0]}")/grep-q.sh"

# The Zephyr line a workspace checks out, read from `<ws>/zephyr/VERSION`
# (`VERSION_MAJOR = 3`, `VERSION_MINOR = 7` -> `3.7`). This is the tree the
# patch set must fit, whatever manifest it came from.
nros_zephyr_workspace_line() {
    local v="$1/zephyr/VERSION" maj min
    [ -f "$v" ] || return 1
    maj="$(sed -n 's/^VERSION_MAJOR[[:space:]]*=[[:space:]]*\([0-9]*\).*/\1/p' "$v")"
    min="$(sed -n 's/^VERSION_MINOR[[:space:]]*=[[:space:]]*\([0-9]*\).*/\1/p' "$v")"
    [ -n "$maj" ] && [ -n "$min" ] || return 1
    printf '%s.%s\n' "$maj" "$min"
}

# nros_patch_would_apply <tag> <what>
# Issue 2831 — the CHECK mode every patch script shares. A patch script calls
# it immediately before EACH write it would make (after the guard that found
# the edit absent). Normally it returns 0 and the script writes. Under
# `NROS_PATCH_CHECK=1` it prints that the edit is missing and EXITS 1, so the
# script never writes: `NROS_PATCH_CHECK=1 bash patches/<line>.sh <ws>` asks
# the TREE whether this checkout's edits are present, instead of comparing a
# digest of the scripts that make them (which flipped on any byte, including
# a comment, between branches sharing one workspace).
nros_patch_would_apply() {
    if [ "${NROS_PATCH_CHECK:-0}" = 1 ]; then
        echo "[$1] MISSING: $2" >&2
        exit 1
    fi
}

# nros_patch_checking
# True under `NROS_PATCH_CHECK=1`, for a set's steps that are not tree edits
# (provisioning a venv, adding a rustup target): a check skips them.
nros_patch_checking() {
    [ "${NROS_PATCH_CHECK:-0}" = 1 ]
}

# nros_zephyr_apply_patchset <checkout> <workspace> <who>
# The ONE way a script that has just written a workspace's projects (`west
# init` + `west update`, or a `west update` alone) applies the line's patch set:
# read the line from the tree and run `<checkout>/scripts/zephyr/patches/<line>.sh`.
# Returns non-zero, saying the workspace is UNPATCHED, when the line is
# unreadable, the checkout has no set for it, or the set fails. <who> prefixes
# the messages.
#
# Issue 2764: two scripts wrote projects and each decided for itself what to
# re-apply. The sync relied on its caller, and `scripts/zephyr/setup.sh`, which
# documents itself as runnable on its own, applied two of a line's ~20 patches
# by name. `check-west-update-patchset.py` keeps every
# writer on this function.
nros_zephyr_apply_patchset() {
    local co="$1" ws="$2" who="$3" line set
    if ! line="$(nros_zephyr_workspace_line "$ws")"; then
        echo "$who: $ws/zephyr/VERSION is missing or unreadable, so which line's patch set" >&2
        echo "  to apply is unknown. The workspace is UNPATCHED;" >&2
        echo "  run: bash $co/scripts/zephyr/patches/<line>.sh $ws" >&2
        return 1
    fi
    set="$co/scripts/zephyr/patches/$line.sh"
    if [ ! -f "$set" ]; then
        echo "$who: the workspace checks out Zephyr $line and $co has no" >&2
        echo "  patch set for it ($set). The workspace is UNPATCHED." >&2
        return 1
    fi
    echo "[zephyr setup] applying the $line patch set ($set)"
    if ! bash "$set" "$ws"; then
        echo "$who: the $line patch set failed, so $ws is not (fully) patched;" >&2
        echo "  a build there fails at configure. Fix the failure above, then run:" >&2
        echo "  bash $set $ws" >&2
        return 1
    fi
}

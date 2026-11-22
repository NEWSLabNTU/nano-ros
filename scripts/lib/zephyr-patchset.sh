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
# The record is `<ws>/.west/nros-patchset`:
#
#     line <line>
#     digest <sha256 of the patch set's sources>
#     digest <...>                      (one per distinct set applied)
#
# One `digest` row per DISTINCT patch set applied since the workspace's
# projects last moved. A store workspace is shared by every checkout on the
# host, and every patch is idempotent and additive, so a checkout whose set is
# older or newer than the last one applied still finds its OWN row once it has
# run it. `sync-workspace-manifest.sh` deletes the file when a `west update`
# moves the projects, because the edits are gone then.
#
# Sourced. Every function returns non-zero rather than exiting, except on a
# grep that fails to RUN, which `nros_grep_q` turns into exit 2.

# shellcheck source=scripts/lib/grep-q.sh
source "$(dirname "${BASH_SOURCE[0]}")/grep-q.sh"

# The record's path.
nros_zephyr_patchset_stamp() {
    printf '%s/.west/nros-patchset\n' "$1"
}

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

# nros_zephyr_patchset_digest <checkout> <line>
# The sha256 of `patches/<line>.sh` plus every `scripts/zephyr/*.sh` it names,
# read from <checkout>. Derived from the set itself, so a patch added to it
# changes the digest with no list to keep beside it.
nros_zephyr_patchset_digest() {
    local co="$1" set="$1/scripts/zephyr/patches/$2.sh" f
    [ -f "$set" ] || return 1
    {
        cat "$set"
        while read -r f; do
            [ -n "$f" ] || continue
            printf '%s\n' "$f"
            cat "$co/$f" 2>/dev/null || printf 'absent\n'
        done <<<"$(grep -o 'scripts/zephyr/[A-Za-z0-9._-]*\.sh' "$set" | sort -u || true)"
    } | sha256sum | cut -d' ' -f1
}

# nros_zephyr_patchset_record <workspace> <checkout> <line>
# Called by the patch set as its LAST step, so a set that failed part-way
# records nothing. A directory with no `.west/` is not a west workspace, and
# nothing reads a record there: skipped, saying so.
nros_zephyr_patchset_record() {
    local ws="$1" co="$2" line="$3" stamp d
    if [ ! -d "$ws/.west" ]; then
        echo "[zephyr setup] $ws has no .west/, so the $line patch set is not recorded there"
        return 0
    fi
    d="$(nros_zephyr_patchset_digest "$co" "$line")" || return 1
    stamp="$(nros_zephyr_patchset_stamp "$ws")"
    if [ -f "$stamp" ] && [ "$(sed -n 's/^line //p' "$stamp")" = "$line" ]; then
        nros_grep_q -xF "digest $d" "$stamp" || printf 'digest %s\n' "$d" >> "$stamp"
    else
        printf 'line %s\ndigest %s\n' "$line" "$d" > "$stamp"
    fi
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
# (provisioning a venv, adding a rustup target, recording the run): a check
# skips them.
nros_patch_checking() {
    [ "${NROS_PATCH_CHECK:-0}" = 1 ]
}

#!/usr/bin/env bash
# Issue 2249 — bring an EXISTING west workspace to the checkout's manifest.
#
#   sync-workspace-manifest.sh <workspace> <checkout>
#
# A provisioned workspace does not read the checkout's `west.yml`. It reads a
# COPY, written into its manifest project when the workspace was created
# (issue 1258 / phase-449 W1 — the project is a manifest and nothing else, so
# the store workspace binds no checkout). `unbind-manifest-project.sh` writes
# that copy only when it is MISSING, and `just zephyr setup` ran no
# `west update` at all once `<ws>/zephyr` existed. So a manifest change never
# reached a persistent workspace: phase-524 W2 added `hal_stm32` to the 3.7
# line, and the self-hosted tier-2 runner's store workspace still lacked it
# three nights later — `stm32f767zitx-pinctrl.dtsi: No such file or directory`
# stopped the whole zephyr build stage, so no cell ran.
#
# What this does, and why each part:
#   - Compare the workspace's copy with the checkout's file BYTE FOR BYTE.
#     Equal => nothing to do and NO network: an offline host with a current
#     workspace is unaffected.
#   - Different => replace the copy, then `west update`. The copy IS the stamp,
#     so there is no second file to drift from it.
#   - `west update` fails => put the OLD copy back and fail. Otherwise the next
#     run would find the copy "current", skip, and leave the workspace
#     unfetched for good — the same defect, one run later.
#
# A store workspace is SHARED by every checkout wanting this Zephyr line
# (RFC-0095 D2), so two checkouts on different manifests each re-sync when
# they alternate. That is the price of one workspace per line, and it is
# correct: each build gets the modules its own manifest names. `west update`
# never prunes, so a module a newer manifest added stays for an older one.
#
# A workspace with no `.west/config` is not ours to repair: exit 0, the way
# `unbind-manifest-project.sh --from-config` treats it.
set -euo pipefail

ws="${1:?usage: sync-workspace-manifest.sh <workspace> <checkout>}"
checkout="${2:?usage: sync-workspace-manifest.sh <workspace> <checkout>}"

cfg="$ws/.west/config"
if [ ! -f "$cfg" ]; then
    echo "sync-workspace-manifest: $ws has no .west/config — nothing to sync" >&2
    exit 0
fi

source "$(dirname "${BASH_SOURCE[0]}")/../lib/west-config.sh"
name="$(west_config_field "$cfg" path || true)"
manifest="$(west_config_field "$cfg" file || true)"
[ -n "$manifest" ] || manifest="west.yml"
if [ -z "$name" ]; then
    echo "sync-workspace-manifest: $cfg names no [manifest] path — refusing to guess" >&2
    exit 1
fi

want="$checkout/$manifest"
have="$ws/$name/$manifest"
if [ ! -f "$want" ]; then
    echo "sync-workspace-manifest: the checkout has no $manifest ($want)" >&2
    exit 1
fi

if [ -f "$have" ] && cmp -s "$want" "$have"; then
    echo "[zephyr setup] workspace manifest current ($manifest)"
    exit 0
fi

echo "[zephyr setup] workspace manifest differs from $want — refreshing and running west update"
mkdir -p "$ws/$name"
backup=""
if [ -f "$have" ]; then
    backup="$(mktemp "$ws/.west/$manifest.prev.XXXXXX")"
    cp "$have" "$backup"
fi
cp "$want" "$have"

# The Zephyr lane's own venv holds `west` (issue 0698); the recipe calling this
# has not activated it. Only here, so the "current" path needs no venv either.
source "$(dirname "${BASH_SOURCE[0]}")/../build/zephyr-python.sh"
nros_zephyr_activate
# Our patch sets (`scripts/zephyr/patches/<line>.sh`) edit tracked files of
# west projects IN PLACE (zephyr's nsos_*.c, zephyr-lang-rust, ...). `west
# update` checks out the new revision with git, and git refuses to overwrite a
# modified file. So the first pin move after a patched setup failed on every
# existing workspace, and a persistent one (the self-hosted runner's store
# workspace) stayed red until someone repaired it by hand. Issue 2476.
#
# So before the update, every project with tracked edits has them SAVED to a
# diff under `.west/nros-preupdate/` and is reset to its HEAD. The patch set
# runs after this script on both callers (`just zephyr setup`,
# `scripts/zephyr/setup.sh`), and re-applies our edits to the new revision.
# Anything else in the diff (a user's own edit) is kept in that file and
# named on the console, never discarded. If the update fails, the saved
# diffs go back, so a failed run leaves each project as it found it.
#
# This covers every project and every patch at once. A list of the files our
# patches touch would be a second copy of the patch set, one new patch away
# from missing a file.
saved_dir="$ws/.west/nros-preupdate"
saved_list=""
set_aside_project_edits() {
    local name path top f projects
    # Listed first, not through a process substitution: a `west list` that
    # fails must fail the sync, never read as "no projects".
    projects="$(cd "$ws" && west list -f '{name} {abspath}')" || {
        echo "sync-workspace-manifest: west list failed in $ws" >&2
        return 1
    }
    while read -r name path; do
        [ -n "$path" ] && [ -d "$path" ] || continue
        # Only a project that IS its own git repository. A plain directory
        # (the unbound manifest project) has no repo, and `git -C` would walk
        # UP to an enclosing checkout and reset that instead.
        top="$(git -C "$path" rev-parse --show-toplevel 2>/dev/null)" || continue
        [ "$(cd "$top" && pwd -P)" = "$(cd "$path" && pwd -P)" ] || continue
        git -C "$path" diff --quiet HEAD -- 2>/dev/null && continue
        mkdir -p "$saved_dir"
        f="$saved_dir/$name.$(date +%Y%m%dT%H%M%S).diff"
        git -C "$path" diff --binary HEAD > "$f"
        git -C "$path" reset -q --hard HEAD
        saved_list="$saved_list$path $f
"
        echo "[zephyr setup] $name: set aside local edits before west update (saved to $f); the patch set re-applies nano-ros's own"
    done <<<"$projects"
}
restore_project_edits() {
    local path f
    while read -r path f; do
        [ -n "$f" ] || continue
        git -C "$path" apply --binary "$f" 2>/dev/null \
            || echo "sync-workspace-manifest: could not re-apply $f to $path; the edits are kept in that file" >&2
    done <<<"$saved_list"
}

rc=0
set_aside_project_edits || rc=$?
if [ "$rc" -eq 0 ]; then
    (cd "$ws" && west update) || rc=$?
fi
if [ "$rc" -ne 0 ]; then
    restore_project_edits
    if [ -n "$backup" ]; then
        mv -f "$backup" "$have"
    else
        rm -f "$have"
    fi
    echo "sync-workspace-manifest: west update failed (rc=$rc); restored the previous $manifest" >&2
    echo "  so the next run retries instead of reading the workspace as current." >&2
    exit "$rc"
fi
[ -z "$backup" ] || rm -f "$backup"
echo "[zephyr setup] workspace updated to $manifest"

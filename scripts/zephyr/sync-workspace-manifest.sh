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
rc=0
(cd "$ws" && west update) || rc=$?
if [ "$rc" -ne 0 ]; then
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

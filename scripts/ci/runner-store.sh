#!/usr/bin/env bash
#
# Inspect and reset the contained runner's persistent stores.
#
# WHY THIS EXISTS. `runner-container.sh` gives the ephemeral runner four named
# volumes. Three are caches; the fourth (`nros-runner-nros`) is the SDK store
# `nros setup` writes, and persisting it is what makes a label like
# `nros-sdk-zephyr` true across containers instead of re-provisioned per job.
#
# A store that persists is a store that can rot — the same trade
# `runner-sweep.sh` records for the bare-host runner ("a self-hosted runner is
# fast because it is PERSISTENT; that is the same property that lets it rot").
# The bake alternative has no rot but goes stale silently when
# `nros-sdk-index.toml` moves. Persist plus a reset verb is the trade we took,
# and this is the second half of it. Without a way back, "persist" means
# "accumulate".
#
# THE STORES ARE BIND-BACKED ON PURPOSE. Each volume is a `local` volume with
# `o=bind` onto a real directory, so every one of them is readable, measurable
# and removable with ordinary file tools. An anonymous volume under
# `/var/lib/docker` is none of those things without docker's help, and a store
# you cannot inspect is one nobody audits.
#
# Usage:
#   scripts/ci/runner-store.sh                 # report: path, size, backing disk
#   scripts/ci/runner-store.sh --reset <name>  # wipe ONE store's contents
#   scripts/ci/runner-store.sh --reset-all     # wipe every store's contents
#
# `--reset` clears CONTENTS and keeps the directory, its ACLs and the volume, so
# a reset store is immediately usable. Removing the volume instead would drop
# the ACL grant that lets the container's UID write, and the next start would
# fail somewhere less obvious than here.
set -euo pipefail

STORES=(work cargo sccache nros)
ROOT="${NROS_RUNNER_STORE_ROOT:-$HOME/nros-runner}"

die() { echo "runner-store: $*" >&2; exit 1; }

running() { docker ps --filter name=nano-ros-runner --format '{{.Names}}' 2>/dev/null | grep -q . ; }

report() {
    echo "runner-store: root=$ROOT"
    printf '  %-10s %-8s %-46s %s\n' STORE SIZE PATH VOLUME
    for s in "${STORES[@]}"; do
        local p="$ROOT/$s" size="-" vol="-"
        [ -d "$p" ] && size="$(du -sh "$p" 2>/dev/null | cut -f1)"
        docker volume inspect "nros-runner-$s" >/dev/null 2>&1 && vol="nros-runner-$s"
        printf '  %-10s %-8s %-46s %s\n' "$s" "$size" "$p" "$vol"
    done
    echo
    df -h "$ROOT" 2>/dev/null | tail -1 | sed 's/^/  backing disk: /'
}

reset_one() {
    local s="$1" p="$ROOT/$1"
    case " ${STORES[*]} " in *" $s "*) ;; *) die "unknown store '$s' (have: ${STORES[*]})" ;; esac
    [ -d "$p" ] || die "no such store directory: $p"
    # Refuse while a container holds it. A wipe under a running job is a
    # confusing failure two layers away, not a clean error here.
    running && die "container 'nano-ros-runner' is running — stop it before resetting a store"
    local before; before="$(du -sh "$p" 2>/dev/null | cut -f1)"
    find "$p" -mindepth 1 -maxdepth 1 -exec rm -rf {} + 2>/dev/null || true
    echo "runner-store: reset $s ($before -> $(du -sh "$p" 2>/dev/null | cut -f1)); dir, ACLs and volume kept"
}

case "${1:-}" in
    ""|--report) report ;;
    --reset)     reset_one "${2:?--reset needs a store name (${STORES[*]})}" ;;
    --reset-all) running && die "container 'nano-ros-runner' is running — stop it first"
                 for s in "${STORES[@]}"; do reset_one "$s"; done ;;
    *)           die "unknown argument '$1' (--report | --reset <name> | --reset-all)" ;;
esac

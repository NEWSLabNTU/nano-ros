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
#   scripts/ci/runner-store.sh --ensure        # create dirs + ACLs + volumes
#   scripts/ci/runner-store.sh --reset <name>  # wipe ONE store's contents
#   scripts/ci/runner-store.sh --reset-all     # wipe every store's contents
#
# `--ensure` is idempotent and is what `runner-container.sh` calls before it
# starts anything, so the storage a container needs is never a manual step
# someone can forget. It does three things per store: create the directory,
# grant the container's UID access by ACL, and create the bind-backed volume.
#
# ACL RATHER THAN chmod. The container runs as `RUNNER_UID` (1001 by default),
# the directory is owned by the operator, and the two are different users. The
# alternatives are worse in both directions: `chmod 0777` opens the store to
# everyone on the box, and `chown` needs root — which this repo never takes.
# `setfacl -m u:<uid>:rwx` grants exactly one UID exactly what it needs, and the
# `-d` default entry makes files the container CREATES inherit it, without which
# the second job fails on the first job's leftovers.
#
# THE UID IS MEASURED, NOT ASSUMED. It comes from the built image when one
# exists (`id -u` inside it), because that is the number that will actually
# write; the Dockerfile's ARG default is only the fallback for a first run.
# Getting this wrong is silent until a job writes, and then it is EACCES four
# layers from the cause.
#
# `--reset` clears CONTENTS and keeps the directory, its ACLs and the volume, so
# a reset store is immediately usable. Removing the volume instead would drop
# the ACL grant that lets the container's UID write, and the next start would
# fail somewhere less obvious than here.
set -euo pipefail

STORES=(work cargo rustup sccache nros src)
ROOT="${NROS_RUNNER_STORE_ROOT:-$HOME/nros-runner}"
REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
IMAGE="${NROS_RUNNER_IMAGE:-nano-ros-runner:local}"

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

# The UID that will write these stores. Measured from the image when it exists.
container_uid() {
    if [ -n "${NROS_RUNNER_UID:-}" ]; then echo "$NROS_RUNNER_UID"; return; fi
    local u
    u="$(docker run --rm --entrypoint id "$IMAGE" -u 2>/dev/null || true)"
    case "$u" in ''|*[!0-9]*) ;; *) echo "$u"; return ;; esac
    # No image yet: fall back to the Dockerfile's ARG default, read rather than
    # hardcoded a second time here.
    local df="$REPO_ROOT/ci/docker/runner/Dockerfile" d=""
    [ -f "$df" ] && d="$(sed -n 's/^ARG RUNNER_UID=\([0-9]\+\).*/\1/p' "$df" | head -1)"
    echo "${d:-1001}"
}

ensure() {
    local uid; uid="$(container_uid)"
    command -v setfacl >/dev/null 2>&1 || die "setfacl not found — install acl, or the container's UID cannot be granted access without chmod 0777"
    echo "runner-store: ensuring stores under $ROOT for container uid $uid"
    for s in "${STORES[@]}"; do
        local p="$ROOT/$s"
        mkdir -p "$p"
        # Access entry for the running container, plus DEFAULT entries so what
        # it creates stays reachable to both it and the operator.
        setfacl -m "u:$uid:rwx" "$p"
        setfacl -d -m "u:$uid:rwx" "$p"
        setfacl -d -m "u:$(id -u):rwx" "$p"
        if docker volume inspect "nros-runner-$s" >/dev/null 2>&1; then
            local dev; dev="$(docker volume inspect "nros-runner-$s" --format '{{.Options.device}}' 2>/dev/null || true)"
            [ "$dev" = "$p" ] || echo "  WARN nros-runner-$s is backed by '${dev:-<not a bind>}', not $p — remove it to re-point" >&2
        else
            docker volume create --driver local \
                --opt type=none --opt o=bind --opt device="$p" \
                "nros-runner-$s" >/dev/null
        fi
        printf '  %-10s %s\n' "$s" "$p"
    done
    echo "runner-store: ready"
}

case "${1:-}" in
    ""|--report) report ;;
    --ensure)    ensure ;;
    --reset)     reset_one "${2:?--reset needs a store name (${STORES[*]})}" ;;
    --reset-all) running && die "container 'nano-ros-runner' is running — stop it first"
                 for s in "${STORES[@]}"; do reset_one "$s"; done ;;
    *)           die "unknown argument '$1' (--report | --ensure | --reset <name> | --reset-all)" ;;
esac

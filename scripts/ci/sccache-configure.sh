#!/usr/bin/env bash
# Issue 2930 -- start sccache for a CI step and publish what the later steps
# need to `$GITHUB_ENV`. The one spelling of issue 0874's configure step.
#
# CONTRACT (issue 0874's): PROVE the server starts in this step and publish to
# `$GITHUB_ENV` only if it did. A broken sccache handed to rustc fails the
# compiler rather than losing a cache, so an absent or broken one is a WARNING
# and an uncached build, never a red step.
#
#   --dir DIR    the local cache directory (default: $RUNNER_TEMP/sccache, else
#                /tmp/sccache). Created either way, so a later actions/cache
#                save never warns about a missing path.
#   --size SIZE  SCCACHE_CACHE_SIZE (default 4G).
#   --log FILE   SCCACHE_ERROR_LOG (default $RUNNER_TEMP/sccache-server.log);
#                a later step that prints the server log reads this path.
#   --skip-self-hosted
#                on a self-hosted runner, leave sccache alone: it has its own
#                persistent volume (`SCCACHE_DIR` from the runner's env), and
#                pointing it at a per-job dir would throw that cache away.
#   --what TEXT  what runs uncached if the server cannot start, for the
#                warning ("the compile tier", "the link smoke").
#   --wrapper    also publish RUSTC_WRAPPER, as the SHIM
#                (`scripts/bin/rustc-wrapper/sccache`, issue 1646: a staticlib
#                runs uncached), for steps that call cargo outside `just`.
#   --stats      print `sccache --show-stats` (for an `if: always()` step at
#                the end of the job: the hit rate is the evidence).
set -uo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
dir="${RUNNER_TEMP:-/tmp}/sccache"
size=4G
wrapper=0
log="${RUNNER_TEMP:-/tmp}/sccache-server.log"
skip_self_hosted=0
what="this step's compiles"
while [ "$#" -gt 0 ]; do
    case "$1" in
        --dir) dir="${2:?--dir needs a value}"; shift 2 ;;
        --size) size="${2:?--size needs a value}"; shift 2 ;;
        --wrapper) wrapper=1; shift ;;
        --log) log="${2:?--log needs a value}"; shift 2 ;;
        --skip-self-hosted) skip_self_hosted=1; shift ;;
        --what) what="${2:?--what needs a value}"; shift 2 ;;
        --stats)
            if command -v sccache >/dev/null 2>&1; then
                sccache --show-stats || true
            else
                echo "sccache-configure: no sccache on PATH; nothing was cached"
            fi
            exit 0 ;;
        *) echo "usage: $0 [--dir DIR] [--size SIZE] [--log FILE] [--wrapper] [--skip-self-hosted] [--what TEXT] | --stats" >&2; exit 2 ;;
    esac
done

mkdir -p "$dir"
env_out="${GITHUB_ENV:-/dev/stdout}"
if ! command -v sccache >/dev/null 2>&1; then
    echo "::warning::sccache absent from the image -- $what run(s) UNCACHED."
    exit 0
fi
sccache --version
if [ "$skip_self_hosted" = 1 ] && [ "${RUNNER_ENVIRONMENT:-}" = self-hosted ]; then
    echo "self-hosted: sccache uses its own volume (${SCCACHE_DIR:-$HOME/.cache/sccache})"
    exit 0
fi
# A server may already be running: any earlier `just` step in the job reached
# the justfile's RUSTC_WRAPPER and started one with the DEFAULT dir and size.
# A server's dir and size are fixed when it starts, and a second
# `--start-server` fails with `Address in use`, which the step would read as
# "unavailable" and publish nothing (measured, live-peer run 45313891417: 0
# Rust requests in 2 h 20 min of fixture rows). Stop it first, so the server the
# later steps reach is the one configured here. Never on a self-hosted runner:
# its server is the host's shared one, on its persistent volume, and other
# jobs are using it.
if [ "${RUNNER_ENVIRONMENT:-}" != self-hosted ]; then
    sccache --stop-server >/dev/null 2>&1 || true
fi
start_log="${RUNNER_TEMP:-/tmp}/sccache-start.log"
# SCCACHE_LOG=warn: the stats give a write-error COUNT and no reason (777 of
# 777 writes failed on run 33294943815), and only the server's log says why;
# `warn` is one line per distinct failure, not one per unit. The server is
# started with no jobserver in its environment, as the shim starts it (issue
# 2284: a server that inherits MAKEFLAGS holds the run's tokens).
if env -u MAKEFLAGS -u MFLAGS -u CARGO_MAKEFLAGS \
    SCCACHE_DIR="$dir" SCCACHE_CACHE_SIZE="$size" SCCACHE_ERROR_LOG="$log" SCCACHE_LOG=warn \
    sccache --start-server >"$start_log" 2>&1; then
    {
        echo "SCCACHE_DIR=$dir"
        echo "SCCACHE_ERROR_LOG=$log"
        echo "SCCACHE_LOG=warn"
        echo "SCCACHE_CACHE_SIZE=$size"
        if [ "$wrapper" = 1 ]; then
            echo "RUSTC_WRAPPER=$repo_root/scripts/bin/rustc-wrapper/sccache"
        fi
    } >> "$env_out"
    echo "sccache-configure: server up, cache $dir ($size)$([ "$wrapper" = 1 ] && echo ', RUSTC_WRAPPER = the shim')"
else
    echo "::warning::sccache unavailable -- $what run(s) UNCACHED (issue 0874)."
    sed 's/^/  /' "$start_log" || true
fi
exit 0

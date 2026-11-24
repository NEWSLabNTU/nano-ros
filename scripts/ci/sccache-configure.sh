#!/usr/bin/env bash
# phase-591 W3 -- start sccache for a CI step that compiles OUTSIDE `just`, and
# publish what the later steps need to `$GITHUB_ENV`.
#
#   sccache-configure.sh [--dir DIR] [--size SIZE] [--wrapper]
#   sccache-configure.sh --stats
#
# WHY. The justfile sets `RUSTC_WRAPPER` for every `just` recipe, so a build
# that goes through `just` is cached without asking. A step that runs a fixture
# builder with plain `bash` (live-peer's host fixture loop, host-tests' fixture
# rows) never sees that export: it compiled every row uncached. Those loops
# also drop each row's compile intermediates to keep the runner's disk (issue
# 2702, `release-fixture-intermediates.sh`), so every row recompiled nros's
# core chain from scratch, about 41 s a Rust row on a hosted runner.
#
# Content-keyed, so it keeps issue 0616's build classes correct: a crate whose
# configuration differs between two rows misses, and an identical one hits
# whichever class dir asks. Measured by 241 (phase-591 W5): a second class
# hits 36 of 48 Rust units (75 %); the misses are exactly the crates with a
# build script, whose OUT_DIR path is in their key.
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
while [ "$#" -gt 0 ]; do
    case "$1" in
        --dir) dir="${2:?--dir needs a value}"; shift 2 ;;
        --size) size="${2:?--size needs a value}"; shift 2 ;;
        --wrapper) wrapper=1; shift ;;
        --stats)
            if command -v sccache >/dev/null 2>&1; then
                sccache --show-stats || true
            else
                echo "sccache-configure: no sccache on PATH; nothing was cached"
            fi
            exit 0 ;;
        *) echo "usage: $0 [--dir DIR] [--size SIZE] [--wrapper] | --stats" >&2; exit 2 ;;
    esac
done

mkdir -p "$dir"
env_out="${GITHUB_ENV:-/dev/stdout}"
if ! command -v sccache >/dev/null 2>&1; then
    echo "::warning::sccache absent from the image -- this step's compiles run UNCACHED."
    exit 0
fi
sccache --version
# A server may already be running: any earlier `just` step in the job reached
# the justfile's RUSTC_WRAPPER and started one with the DEFAULT dir and size.
# A server's dir and size are fixed when it starts, and a second
# `--start-server` fails with `Address in use`, which this step read as
# "unavailable" and published nothing (live-peer run 45313891417: 0 Rust
# requests in 2 h 20 min of fixture rows). Stop it first, so the server the
# later steps reach is the one configured here.
sccache --stop-server >/dev/null 2>&1 || true
log="${RUNNER_TEMP:-/tmp}/sccache-server.log"
start_log="${RUNNER_TEMP:-/tmp}/sccache-start.log"
if SCCACHE_DIR="$dir" SCCACHE_CACHE_SIZE="$size" SCCACHE_ERROR_LOG="$log" SCCACHE_LOG=warn \
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
    echo "::warning::sccache unavailable -- this step's compiles run UNCACHED (issue 0874)."
    sed 's/^/  /' "$start_log" || true
fi
exit 0

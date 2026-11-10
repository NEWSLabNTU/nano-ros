#!/usr/bin/env bash
# The SDK-store cache for the template-copy-out jobs (issue 2369 follow-up).
#
# Every merge group's copy-out shards fetched the same NEWSLabNTU-hosted dists
# and built Corrosion from source, once per shard: about 600 MiB of
# arm-none-eabi-gcc across the three FreeRTOS shards and four Corrosion builds
# per merge group. Each fetch is also a chance for the release host to 504.
# This keys one cache entry per job kind on exactly the `[tool.*]` sections
# of `nros-sdk-index.toml` that entry holds, so the entry moves only when one of
# those pins moves, and every merge group in between restores it.
#
# Two kinds, because the jobs fetch different tools and the first job to save a
# shared entry would decide its contents for everyone:
#   copy-out         the three FreeRTOS shards: arm-none-eabi-gcc, corrosion
#   copy-out-zephyr  the Zephyr shard:          clang-format, corrosion
#
# Not cached: the Zephyr SDK (1.4 GiB, a different CDN, see the job comment;
# issue 2219) and anything the jobs fetch that is not listed here.
#
# A restored prefix is a finished install: `nros setup` writes
# `<prefix>/.nros-provenance` only on success and treats a prefix holding it as
# present, so a restore turns the fetch into a no-op. A prefix saved without
# that marker (a job that died mid-install) is installed again over the top.
#
# Usage:
#   scripts/ci/sdk-store-cache.sh key   <kind>   one line: the cache key
#   scripts/ci/sdk-store-cache.sh paths <kind>   one store prefix per line
set -euo pipefail

cmd="${1:-}" kind="${2:-}"
root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
index="$root/nros-sdk-index.toml"

case "$kind" in
    copy-out) tools=(arm-none-eabi-gcc corrosion) ;;
    copy-out-zephyr) tools=(clang-format corrosion) ;;
    *)
        echo "sdk-store-cache: unknown kind '$kind' (copy-out | copy-out-zephyr)" >&2
        exit 2
        ;;
esac

# The text of `[tool.<t>]` and its sub-tables (`[tool.<t>.source]`, ...), up to
# the next table that is not one of them. Comments are included, so a comment
# edit costs one spurious miss, never a stale hit.
section() {
    awk -v t="$1" '
        /^\[/ {
            h = $0
            keep = (h == "[tool." t "]" || index(h, "[tool." t ".") == 1)
        }
        keep { print }
    ' "$index"
}

version_of() {
    section "$1" | awk -F'"' '/^version *=/ { print $2; exit }'
}

case "$cmd" in
    key)
        sum="$(for t in "${tools[@]}"; do
            echo "== $t"
            section "$t"
        done | sha256sum | cut -c1-16)"
        echo "sdk-store-${kind}-${sum}"
        ;;
    paths)
        store="${NROS_HOME:-$HOME/.nros}/sdk"
        for t in "${tools[@]}"; do
            v="$(version_of "$t")"
            if [ -z "$v" ]; then
                echo "sdk-store-cache: no version in [tool.$t] of $index" >&2
                exit 1
            fi
            echo "$store/$t/$v"
        done
        ;;
    *)
        echo "usage: sdk-store-cache.sh key|paths copy-out|copy-out-zephyr" >&2
        exit 2
        ;;
esac

#!/usr/bin/env bash
# B46 -- the zenoh-pico fork's shared BSD-socket connect helper finishes a
# connect interrupted by a signal instead of failing it. Compiles
# tests/c/zpico_connect_eintr.c against the fork's real helper and runs it on a
# loopback listener. Exit 78: the zenoh-pico source is not provisioned here.
set -euo pipefail
root="$(cd "$(dirname "$0")/.." && pwd)"
# shellcheck source=scripts/build/cargo.sh
source "$root/scripts/build/cargo.sh"
zp="$(nros_locate_source zenoh-pico)" || zp=""
helper_dir="$zp/src/link/transport/tcp"
if [ -z "$zp" ] || [ ! -f "$helper_dir/tcp_connect_within.h" ]; then
    echo "zenoh-pico source not provisioned (\`nros locate zenoh-pico --why\` says where it is looked for)"
    exit 78
fi
cc_bin="${CC:-cc}"
if ! command -v "$cc_bin" >/dev/null 2>&1; then
    echo "no C compiler ($cc_bin)"
    exit 78
fi
tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT
"$cc_bin" -std=c11 -Wall -Wextra -Werror -I"$helper_dir" \
    -o "$tmp/zpico_connect_eintr" "$root/tests/c/zpico_connect_eintr.c"
"$tmp/zpico_connect_eintr"

#!/usr/bin/env bash
# phase-492 — the C API's PUBLIC headers are freestanding.
#
# A user includes the C API at upstream's paths (`<rclc/rclc.h>`,
# `<rcl/rcl.h>`, `<rcl_action/*.h>`, ...); `<rclc/detail/*.h>` and the
# generated `<nros/*.h>` are internal (phase-501). Those
# headers must compile with no libc at all, because the parts they target
# (bare metal, an RTOS with a minimal C library) may have none, and a ported
# rclc file includes them first. Freestanding here is the ISO C list a
# `-ffreestanding` compiler supplies by itself: <stddef.h>, <stdint.h>,
# <stdbool.h>, <stdarg.h>, <limits.h>, <float.h>, <stdalign.h>,
# <stdnoreturn.h>, <iso646.h>.
#
# Until this gate, the umbrella every upstream-path header includes reached
# <stdio.h> (`log.h`, for an inline `vsnprintf`) and <string.h> (`view.h`, for
# a bit-cast `memcpy`), so all 51 of them failed.
#
# Each public header (every tracked `.h` under `packages/api/nros-c/include/`
# outside the internal `rclc/detail/` and `nros/`) is compiled on its own with `-ffreestanding
# -nostdinc` and only the compiler's own include dir. The size header is the
# committed buildless snapshot (`-DNROS_CONFIG_BUILDLESS`): this is a
# header-only check and builds nothing.
#
# Negative control: a header that includes <stdio.h> must FAIL under the same
# flags, or the probe could never fail.
#
# Run:  bash scripts/check-c-api-freestanding-headers.sh
# Gate: just check c-api-freestanding-headers

set -euo pipefail

repo="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
include="packages/api/nros-c/include"
cc="${CC:-cc}"

builtin_include="$("$cc" -print-file-name=include)"
if [ ! -f "$builtin_include/stddef.h" ]; then
    echo "check-c-api-freestanding-headers: $cc has no builtin include dir with stddef.h ($builtin_include)" >&2
    exit 1
fi

flags=(-x c -std=c11 -ffreestanding -nostdinc -isystem "$builtin_include"
       -DNROS_CONFIG_BUILDLESS -I "$repo/$include" -fsyntax-only -Wall -Werror)

probe() { # <header spelling as included>
    printf '#include <%s>\n' "$1" | "$cc" "${flags[@]}" - 2>&1
}

tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT

# Negative control, run on every invocation: a header that includes <stdio.h>
# must FAIL under the same flags, and a freestanding one must pass.
selftest() {
    mkdir -p "$tmp/neg"
    printf '#include <stdio.h>\n' > "$tmp/neg/hosted.h"
    printf '#include <stddef.h>\n#include <stdint.h>\n' > "$tmp/neg/free.h"
    if printf '#include <neg/hosted.h>\n' | "$cc" "${flags[@]}" -I "$tmp" - >/dev/null 2>&1; then
        echo "check-c-api-freestanding-headers: SELFTEST FAILED — a header including <stdio.h> compiled; the probe cannot fail" >&2
        return 1
    fi
    if ! printf '#include <neg/free.h>\n' | "$cc" "${flags[@]}" -I "$tmp" - >/dev/null 2>&1; then
        echo "check-c-api-freestanding-headers: SELFTEST FAILED — a freestanding header did not compile; the flags are wrong" >&2
        return 1
    fi
}
selftest

headers=()
while IFS= read -r path; do
    rel="${path#"$include"/}"
    case "$rel" in
        nros/* | rclc/detail/*) continue ;;
    esac
    headers+=("$rel")
done < <(git -C "$repo" ls-files -- "$include/*.h")

if [ "${#headers[@]}" -lt 50 ]; then
    echo "check-c-api-freestanding-headers: only ${#headers[@]} public header(s) found — the listing broke; refusing a vacuous pass" >&2
    exit 1
fi

failed=0
for h in "${headers[@]}"; do
    rc=0
    out="$(probe "$h")" || rc=$?
    if [ "$rc" -ne 0 ]; then
        failed=$((failed + 1))
        echo "  <$h> is not freestanding:"
        printf '%s\n' "$out" | grep -E 'error|note: in file included' | head -4 | sed 's/^/      /'
    fi
done

if [ "$failed" -ne 0 ]; then
    echo "check-c-api-freestanding-headers: FAIL — $failed of ${#headers[@]} public header(s) need a hosted libc." >&2
    echo "  Keep libc calls in the library (c-stubs/, Rust); a public header may include only" >&2
    echo "  the freestanding set listed at the top of this script." >&2
    exit 1
fi
echo "check-c-api-freestanding-headers: OK — ${#headers[@]} public header(s) compile with -ffreestanding -nostdinc"

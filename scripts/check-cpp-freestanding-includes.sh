#!/usr/bin/env bash
# Issue 0332 — the freestanding-header contract, enforced at the source.
#
# nros-cpp public headers must be includable on an embedded target with a
# MINIMAL C++ library (Zephyr's libcpp: `<cstdint>`/`<cstddef>` yes,
# `<string>`/`<vector>` no). The 0112 rule is that a hosted STL include gates on
# `NROS_CPP_STD`, never on `__STDC_HOSTED__` alone — a hosted compiler run
# `-nostdinc++` against that minimal libcpp still has no `<string>`.
#
# The `-ffreestanding` compile probe in `just check cpp` cannot see this: it runs
# against the host's full libstdc++, so an ungated `#include <string>` compiles
# clean. A `-nostdinc++` variant would need Zephyr's libcpp on the include path,
# which the probe host does not have. This gate detects the class at the source
# level instead: a hosted STL `#include` that is not inside an `#ifdef
# NROS_CPP_STD` / `#if defined(NROS_CPP_STD)` region is a violation.
set -euo pipefail
cd "$(dirname "$0")/.."

HEADER_DIR="packages/api/nros-cpp/include/nros"

# Hosted-only STL headers absent from a minimal freestanding libcpp. The
# freestanding-guaranteed set (`<cstdint>`, `<cstddef>`, `<cstdlib>`,
# `<cstring>`, `<type_traits>`, `<utility>`, `<new>`, `<initializer_list>`,
# `<limits>`, `<cstdarg>`, `<cstdio>`) is deliberately NOT listed — those are
# allowed ungated.
HOSTED='string|vector|map|unordered_map|unordered_set|set|functional|memory|chrono|sstream|iostream|fstream|ostream|istream|algorithm|deque|list|thread|mutex|future|regex'

violations=0

for hdr in "$HEADER_DIR"/*.hpp; do
    base="$(basename "$hdr")"
    # rclcpp_compat.hpp is a deliberately-ungated source-compat shim, excluded
    # from the freestanding probe by design (phase 209) — skip it here too.
    case "$base" in rclcpp_compat.hpp) continue ;; esac

    # Walk the file tracking NROS_CPP_STD guard depth. Flag a hosted `#include`
    # that appears at guard-depth 0.
    hits="$(awk -v hosted="$HOSTED" '
        BEGIN { depth = 0 }
        # Enter an NROS_CPP_STD region: `#ifdef NROS_CPP_STD` or
        # `#if defined(NROS_CPP_STD)`. Other #if/#ifdef push a neutral level so
        # a nested #endif does not close the NROS_CPP_STD region prematurely.
        /^[[:space:]]*#[[:space:]]*(ifdef|if)([[:space:]]|\().*NROS_CPP_STD/ { stack[++sp] = "std"; next }
        /^[[:space:]]*#[[:space:]]*(ifdef|ifndef|if)\b/ { stack[++sp] = "other"; next }
        /^[[:space:]]*#[[:space:]]*endif\b/ { if (sp > 0) sp-- ; next }
        {
            in_std = 0
            for (i = 1; i <= sp; i++) if (stack[i] == "std") in_std = 1
            if (in_std) next
            if ($0 ~ ("^[[:space:]]*#[[:space:]]*include[[:space:]]*<(" hosted ")>")) {
                printf "%d: %s\n", NR, $0
            }
        }
    ' "$hdr")"

    if [ -n "$hits" ]; then
        echo "check-cpp-freestanding-includes: $base includes a hosted STL header outside NROS_CPP_STD (issue 0332):" >&2
        while IFS= read -r line; do echo "  $base:$line" >&2; done <<<"$hits"
        violations=1
    fi
done

if [ "$violations" -ne 0 ]; then
    echo >&2
    echo "Fix: wrap the hosted section in \`#ifdef NROS_CPP_STD\` (see std_compat.hpp / bridge.hpp)." >&2
    exit 1
fi

echo "check-cpp-freestanding-includes: OK (no ungated hosted STL includes in nros-cpp headers)"

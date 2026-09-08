#!/usr/bin/env bash
# Issue 0332 — the freestanding-header contract, enforced at the source.
#
# Issue 1023 widened the SCOPE, and the reason is the 0196 rule: this gate's
# coverage was narrower than the rule it enforces. It read `nros-cpp`'s public
# headers only, while `cmake/toolchain/riscv64-threadx.cmake:219` puts
# `-nostdinc++ -isystem <cxx-compat>` on EVERY C++ TU built for that board —
# which includes the whole Cyclone backend. The class then bit there twice in
# one file: issue 0942 (`<cstdio>`) and issue 1014 (`<memory>` + `<string>`,
# which meant the Cyclone backend had never built for that board at all).
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
# `nros_grep_q` — a `grep -q` that cannot report a tool ERROR as a non-match
# (issue 0726). Fed a here-string, never a pipe (issue 1077).
# shellcheck source=lib/grep-q.sh
. scripts/lib/grep-q.sh

# The two trees this toolchain compiles with the shim on the include path. Not
# a universal claim: it is these two because these are what a board build
# reaches, and naming them is honest where "every C++ file" would not be.
#
# There is a THIRD location that compiles `-nostdinc++`, deliberately left out:
# `packages/api/nros-cpp/tests/compile/*.cpp`. Those TUs exist to BE compiled
# under the constraint, so a hosted include there fails the compile probe in
# `just check cpp` loudly and immediately. This gate earns its keep where no
# compile runs — a source-level check is a substitute for a build, not a
# duplicate of one. Measured 2026-09-05 by grepping `-nostdinc++` across
# `packages/` and `cmake/`; the only other hits are prose.
SCAN_DIRS="packages/api/nros-cpp/include/nros packages/rmw/cyclonedds/nros-rmw-cyclonedds/src"

# Known debt this walker could not see before issue 1223, as `<file> <header>`
# pairs. A RATCHET: an unlisted violation fails, and a listed pair that no
# longer offends ALSO fails, so the file can only shrink and only on purpose.
# phase-438 W2 empties it. Keyed on file+header rather than line so an edit
# above a site does not silently move the debt.
BASELINE=".config/cpp-freestanding-includes-baseline.txt"


# Hosted-only STL headers absent from a minimal freestanding libcpp. The
# freestanding-guaranteed set (`<cstdint>`, `<cstddef>`, `<cstdlib>`,
# `<cstring>`, `<type_traits>`, `<utility>`, `<new>`, `<initializer_list>`,
# `<limits>`, `<cstdarg>`, `<cstdio>`) is deliberately NOT listed — those are
# allowed ungated.
HOSTED='string|vector|map|unordered_map|unordered_set|set|functional|memory|chrono|sstream|iostream|fstream|ostream|istream|algorithm|deque|list|thread|mutex|future|regex'

# The walker, as a function, so the selftest below exercises the SAME code the
# gate runs. A selftest against a re-typed copy proves nothing about the gate.
walk_file() {
  awk -v hosted="$HOSTED" -v strict="$2" '
        BEGIN { sp = 0 }
        # Enter an NROS_CPP_STD region: `#ifdef NROS_CPP_STD` or
        # `#if defined(NROS_CPP_STD)`. Other #if/#ifdef push a neutral level so
        # a nested #endif does not close the NROS_CPP_STD region prematurely.
        /^[[:space:]]*#[[:space:]]*(ifdef|if)([[:space:]]|\().*NROS_CPP_STD/ { stack[++sp] = "std"; next }
        # `\b` is NOT a word boundary in POSIX ERE — awk reads it as an escape
        # with no such meaning, so these two rules MATCHED NOTHING. The "other"
        # push therefore never happened, which is the very bug the comment above
        # says it prevents: a nested `#endif` popped the NROS_CPP_STD region
        # early. Latent while every guarded include sat in a flat `#ifdef`;
        # found by issue 1023 when a backend TU with a real `#if/#elif/#else`
        # chain came into scope. `([[:space:]]|$)` is the portable spelling.
        /^[[:space:]]*#[[:space:]]*(ifdef|ifndef|if)([[:space:]]|$)/ { stack[++sp] = "other"; next }
        # `#elif` / `#else` REPLACE the top frame rather than leaving it alone
        # (issue 1223). They matched neither rule above, so they were ordinary
        # text and the frame from the opening `#if` survived into the
        # alternative arm — where its condition is false by construction. Every
        # `#elif defined(__has_include)` arm in nros-cpp scored as
        # NROS_CPP_STD-guarded because of it, and `#else` was worse: that arm is
        # the one taken when NROS_CPP_STD is undefined.
        #
        # An `#elif` naming NROS_CPP_STD really is a std region. Anything else
        # is guarded by SOMETHING, just not by that — which is why the
        # replacement is "other" and not a pop: at strict=0 the Cyclone
        # backend legitimately takes <chrono>/<thread> in the `#else` of an
        # NROS_PLATFORM_* chain, and depth must not fall to 0 there.
        /^[[:space:]]*#[[:space:]]*elif([[:space:]]|\()/ {
            if (sp == 0) sp = 1
            stack[sp] = ($0 ~ /NROS_CPP_STD/) ? "std" : "other"
            next
        }
        /^[[:space:]]*#[[:space:]]*else([[:space:]]|$)/ {
            if (sp == 0) sp = 1
            stack[sp] = "other"
            next
        }
        /^[[:space:]]*#[[:space:]]*endif([[:space:]]|$)/ { if (sp > 0) sp-- ; next }
        {
            guarded = 0
            if (strict == 1) {
                for (i = 1; i <= sp; i++) if (stack[i] == "std") guarded = 1
            } else {
                guarded = (sp > 0)
            }
            if (guarded) next
            if ($0 ~ ("^[[:space:]]*#[[:space:]]*include[[:space:]]*<(" hosted ")>")) {
                printf "%d: %s\n", NR, $0
            }
        }
    ' "$1"
}

# --- selftest ----------------------------------------------------------------
# The negative controls issue 1223 says this gate did not have. Cases 1 and 2
# are the two spellings that were invisible; 5 is the strict=0 shape the fix
# must NOT break, and it is the reason `#else` replaces the frame instead of
# popping it.
selftest() {
  local d rc=0 cases=0
  d="$(mktemp -d)"
  trap 'rm -rf "$d"' RETURN

  _case() { # name strict expect(hit|clean) body
    local name="$1" strict="$2" expect="$3" body="$4" got
    printf '%s' "$body" > "$d/probe.hpp"
    got="$(walk_file "$d/probe.hpp" "$strict")"
    cases=$((cases + 1))
    if [ "$expect" = hit ] && [ -z "$got" ]; then
      echo "check-cpp-freestanding-includes SELFTEST FAIL: '$name' should have been flagged and was not" >&2
      rc=1
    elif [ "$expect" = clean ] && [ -n "$got" ]; then
      echo "check-cpp-freestanding-includes SELFTEST FAIL: '$name' should be clean, got: $got" >&2
      rc=1
    fi
  }

  # 1. The issue-1223 shape: the `__has_include` arm is NOT NROS_CPP_STD-guarded.
  _case 'elif __has_include arm' 1 hit '#if defined(NROS_CPP_STD)
#include <memory>
#elif defined(__has_include)
#if __has_include(<string>)
#include <string>
#endif
#endif
'
  # 2. Worse spelling: the `#else` arm is the one taken when the macro is OFF.
  _case 'else fallback arm' 1 hit '#if defined(NROS_CPP_STD)
#include <memory>
#else
#include <vector>
#endif
'
  # 3. The correct shape stays clean.
  _case 'flat NROS_CPP_STD guard' 1 clean '#ifdef NROS_CPP_STD
#include <string>
#endif
'
  # 4. An `#elif` that names NROS_CPP_STD really is a std region.
  _case 'elif naming NROS_CPP_STD' 1 clean '#if defined(SOMETHING_ELSE)
#else
#elif defined(NROS_CPP_STD)
#include <string>
#endif
'
  # 5. strict=0: the Cyclone backend takes <chrono> in the `#else` of a platform
  #    chain. Any conditional counts there, so this must remain clean — a `#else`
  #    that POPPED instead of replacing would break it.
  _case 'backend platform else arm' 0 clean '#if defined(NROS_PLATFORM_ZEPHYR)
#include <cstdint>
#else
#include <chrono>
#endif
'
  # 6. Depth 0 is still a violation at both strictnesses.
  _case 'ungated at depth 0' 0 hit '#include <string>
'

  if [ "$rc" -ne 0 ]; then
    echo "check-cpp-freestanding-includes: the walker does not behave as documented; not scanning the tree." >&2
    exit 1
  fi
  echo "check-cpp-freestanding-includes self-test: OK ($cases cases)"
}
selftest

# --- baseline ----------------------------------------------------------------
if [ ! -f "$BASELINE" ]; then
    echo "check-cpp-freestanding-includes: missing $BASELINE" >&2
    exit 1
fi
# `<file> <header>` pairs, comments and blanks dropped.
baseline_pairs="$(grep -v '^[[:space:]]*#' "$BASELINE" | grep -v '^[[:space:]]*$' || true)"

violations=0
unlisted=""
observed=""

for entry in "packages/api/nros-cpp/include/nros:1" \
             "packages/rmw/cyclonedds/nros-rmw-cyclonedds/src:0"; do
  dir="${entry%:*}"
  strict="${entry##*:}"
  # `.cpp` too, not only headers: the 1014 break was in a TRANSLATION UNIT, and
  # a TU that cannot compile is exactly as broken as a header that cannot be
  # included.
  for hdr in $(ls "$dir"/*.hpp "$dir"/*.cpp 2>/dev/null); do
    base="$(basename "$hdr")"
    hits="$(walk_file "$hdr" "$strict")"
    [ -n "$hits" ] || continue

    while IFS= read -r line; do
        # `123: #include <string>` -> `<string>`
        stl="$(printf '%s' "$line" | sed -n 's/.*\(<[a-z_]*>\).*/\1/p')"
        pair="$base $stl"
        observed="$observed$pair
"
        if nros_grep_q -xF "$pair" <<<"$baseline_pairs"; then
            continue
        fi
        unlisted="$unlisted  $base:$line
"
        violations=1
    done <<<"$hits"
  done
done

if [ "$violations" -ne 0 ]; then
    echo "check-cpp-freestanding-includes: hosted STL header(s) included outside a guard (issues 0332/1023/1223):" >&2
    printf '%s' "$unlisted" >&2
    echo >&2
    echo "Fix: wrap the hosted section in a guard — \`#ifdef NROS_CPP_STD\` for nros-cpp" >&2
    echo "(std_compat.hpp / bridge.hpp), or a platform \`#if\` for a backend TU" >&2
    echo "(nros-rmw-cyclonedds/src/internal.hpp does the latter for <chrono>/<thread>)." >&2
    echo "If neither fits, the header is not available on that board: do without it." >&2
    echo >&2
    echo "An \`#elif\` or \`#else\` arm is NOT covered by the \`#if\` above it: that arm runs" >&2
    echo "precisely when the \`#if\` condition is false (issue 1223)." >&2
    exit 1
fi

# The ratchet's other direction: a baseline line that no longer offends is
# debt-paid that nobody removed, and leaving it means the next real violation at
# that site is silently excused.
stale=""
while IFS= read -r pair; do
    [ -n "$pair" ] || continue
    nros_grep_q -xF "$pair" <<<"$observed" || stale="$stale  $pair
"
done <<<"$baseline_pairs"

if [ -n "$stale" ]; then
    echo "check-cpp-freestanding-includes: $BASELINE lists site(s) that no longer offend." >&2
    printf '%s' "$stale" >&2
    echo >&2
    echo "Delete them. A baseline entry outlives its violation only by excusing the next one." >&2
    exit 1
fi

n_baseline="$(grep -c . <<<"$baseline_pairs" || true)"
count="$(for d in $SCAN_DIRS; do ls "$d"/*.hpp "$d"/*.cpp 2>/dev/null; done | wc -l)"
echo "check-cpp-freestanding-includes: OK ($count file(s) across nros-cpp headers and the Cyclone backend;" \
     "no unlisted ungated hosted STL includes; $n_baseline known site(s) in $BASELINE, all still present)"

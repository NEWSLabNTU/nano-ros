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
# Issue 1240 completed the predicate rather than replacing it: `__has_include`
# alone is not enough EITHER, because under `-ffreestanding` a full libstdc++
# still HAS the file and refuses to be included from it. So the shape this gate
# accepts is `NROS_CPP_STD` OR the conjunction `__STDC_HOSTED__ &&
# __has_include(<hdr>)`, and naming the token without `__STDC_HOSTED__` beside
# a live `__has_include` arm is a violation. See `std_frame` below.
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

# THE BASELINE IS A CONSTANT NOW, NOT A RATCHET (phase-456 W6).
#
# It held `<file> <header>` pairs of debt this walker could not see before
# issue 1223, as a ratchet: an unlisted violation failed, and a listed pair
# that no longer offended failed too, so it could only shrink. It has been
# EMPTY since issue 1240 paid all fourteen at once.
#
# It may now hold nothing at all: a pair in it is a hard failure. A ratchet is
# the right instrument for debt you intend to pay, and the debt is paid. What
# a slot holds after that is not tolerance for something measured, it is a
# place to put the NEXT violation -- and this rule has no legitimate exception,
# because the gated form is always available. An ungated hosted STL include in
# these two trees does not fail on the host; it fails on a board, at a build
# nobody runs per PR, which is why the source-level check exists at all.
#
# NOTHING IS FORECLOSED. A hosted include behind
# `#if defined(NROS_CPP_STD) || (defined(__STDC_HOSTED__) && __STDC_HOSTED__ &&
# __has_include(<hdr>))` is still legal and still used -- `bridge.hpp` does it,
# `std_detect.hpp` does it six times. The constant refuses the UNGATED form
# only. If a genuine exception ever appears, change this gate with the reason
# in the commit rather than appending a line nobody reviews.
#
# The file stays as the record of what the ratchet measured and why, which is
# the one thing deleting it would throw away.
BASELINE=".config/cpp-freestanding-includes-baseline.txt"

# Factored out so the selftest can drive it (phase-456 W6). Prints the offending
# rows and returns 1; silent and 0 for a file of comments and blanks.
baseline_rows() {
  grep -v '^[[:space:]]*#' "$1" | grep -v '^[[:space:]]*$' || true
}


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
        # Does this directive open a region in which a hosted include is SAFE?
        #
        # Naming NROS_CPP_STD is necessary and NOT sufficient (issue 1240). The
        # capability blocks are now written as one conjunction rather than an
        # `#if`/`#elif` pair:
        #
        #   #if defined(NROS_CPP_STD) || (defined(__STDC_HOSTED__)
        #       && __STDC_HOSTED__ && __has_include(<string>))
        #
        # and the `||` arm is live whenever the opt-in is absent -- which is
        # EVERY shipped configuration, since nothing defines NROS_CPP_STD. So
        # the token alone would score the corrected shape and the broken one
        # identically, and the broken one is what GCC 16 caught: with
        # `-ffreestanding` its libstdc++ is genuinely freestanding, so
        # `__has_include(<string>)` answers TRUE for a header whose first line
        # is `#error "This header is not available in freestanding mode."`.
        # `__STDC_HOSTED__` is the only probe that separates those, exactly as
        # `nros.hpp` measured for <chrono>.
        #
        # This is the 0196 rule applied to this gate: its reach must be the
        # rule it enforces, not the spelling the rule happened to have when it
        # was written.
        # BOTH directions, since issue 1461. The rule is a CONJUNCTION, and
        # each probe alone is measurably wrong:
        #
        #   __has_include alone   -- under -ffreestanding a full libstdc++ HAS
        #                            the header and opens it with #error "This
        #                            header is not available in freestanding
        #                            mode." (GCC 16 made `just check cpp`
        #                            unrunnable this way).
        #   __STDC_HOSTED__ alone -- Zephyr.s arm-none-eabi C++ build reports
        #                            hosted and has a MINIMAL libcpp, so the
        #                            header is simply absent. node.hpp guarded
        #                            <map> this way and every Zephyr C++ image
        #                            failed with `fatal error: map: No such
        #                            file or directory`.
        #
        # This gate rejected the first shape from the day issue 1240 landed and
        # accepted the second for as long, which is the 0196 rule applied to
        # this gate turning out to be half-applied. A frame naming EITHER probe
        # must name both.
        function std_frame(line) {
            if (line !~ /NROS_CPP_STD/) { return 0 }
            if (line ~ /__has_include/ && line !~ /__STDC_HOSTED__/) { return 0 }
            if (line ~ /__STDC_HOSTED__/ && line !~ /__has_include/) { return 0 }
            return 1
        }
        # Enter an NROS_CPP_STD region: `#ifdef NROS_CPP_STD`,
        # `#if defined(NROS_CPP_STD)`, or the conjunction above. Other
        # #if/#ifdef push a neutral level so a nested #endif does not close the
        # NROS_CPP_STD region prematurely.
        /^[[:space:]]*#[[:space:]]*(ifdef|if)([[:space:]]|\().*NROS_CPP_STD/ { stack[++sp] = std_frame($0) ? "std" : "other"; next }
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
            stack[sp] = std_frame($0) ? "std" : "other"
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

  # 7. Issue 1240 — the corrected capability block. `__STDC_HOSTED__` is what
  #    makes the `||` arm safe, so this shape is clean.
  _case 'conjunction with __STDC_HOSTED__' 1 clean '#if defined(NROS_CPP_STD) || (defined(__STDC_HOSTED__) && __STDC_HOSTED__ && __has_include(<string>))
#include <string>
#define NROS_CPP_HAS_STD_STRING 1
#endif
'
  # 8. The shape GCC 16 broke: NROS_CPP_STD is named, but the live arm asks only
  #    `__has_include`, which answers TRUE for a libstdc++ header that `#error`s
  #    under `-ffreestanding`. Naming the token is not enough.
  _case 'conjunction WITHOUT __STDC_HOSTED__' 1 hit '#if defined(NROS_CPP_STD) || __has_include(<string>)
#include <string>
#define NROS_CPP_HAS_STD_STRING 1
#endif
'
  # 9. Same omission in an `#elif` arm.
  _case 'elif __has_include without __STDC_HOSTED__' 1 hit '#if defined(SOMETHING)
#elif defined(NROS_CPP_STD) || __has_include(<memory>)
#include <memory>
#endif
'
  # 9b. Issue 1461 — THE MIRROR OF CASE 8, and the one this gate accepted for as
  #     long as it rejected case 8. `__STDC_HOSTED__` alone is not enough
  #     either: Zephyr's arm-none-eabi C++ build reports hosted and ships a
  #     MINIMAL libcpp, so the header is simply absent. `node.hpp` guarded
  #     `<map>` exactly like this, this gate said OK on 76 files, and every
  #     Zephyr C++ image failed with `fatal error: map: No such file or
  #     directory`. The rule is a conjunction; half of it is not a weaker
  #     version of it.
  _case 'NROS_CPP_STD || __STDC_HOSTED__ without __has_include' 1 hit '#if defined(NROS_CPP_STD) || (__STDC_HOSTED__ + 0)
#include <map>
#endif
'
  # 9c. The same omission spelled the long way, so the case is about the missing
  #     probe rather than about the `+ 0` idiom.
  _case 'defined(__STDC_HOSTED__) without __has_include' 1 hit '#if defined(NROS_CPP_STD) || (defined(__STDC_HOSTED__) && __STDC_HOSTED__)
#include <vector>
#endif
'

  # 10. phase-456 W6 — THE MUTATION, on a REAL header. Cases 1-9 drive the
  #     walker with hand-written snippets, which proves what the walker
  #     believes and not that it is pointed at the tree. This copies a tracked
  #     header, reintroduces a `std` TYPE in a public signature together with
  #     the ungated include that type needs, and asserts the walker flags it.
  #     The UNMUTATED copy runs first, for the reason the capability-layout
  #     gate's case 3 gives: if the gate already fires on a faithful copy, the
  #     mutated run proves nothing.
  local real mutated
  real="packages/api/nros-cpp/include/nros/publisher.hpp"
  if [ ! -f "$real" ]; then
    echo "check-cpp-freestanding-includes SELFTEST FAIL: $real is missing, so the real-header control could not run" >&2
    rc=1
  else
    cp "$real" "$d/real.hpp"
    cases=$((cases + 1))
    if [ -n "$(walk_file "$d/real.hpp" 1)" ]; then
      echo "check-cpp-freestanding-includes SELFTEST FAIL: an UNMUTATED copy of $real was flagged, so a mutation of it would prove nothing" >&2
      rc=1
    else
      mutated="$d/mutated.hpp"
      {
        printf '#include <string>\n'
        printf 'namespace rclcpp { struct Mutant { std::string topic_name(); }; }\n'
        cat "$d/real.hpp"
      } > "$mutated"
      cases=$((cases + 1))
      if [ -z "$(walk_file "$mutated" 1)" ]; then
        echo "check-cpp-freestanding-includes SELFTEST FAIL: a std type reintroduced into a public signature, with its ungated <string>, was NOT flagged" >&2
        rc=1
      fi
    fi
  fi

  # 11. phase-456 W6 — the CONSTANT itself. A row in the baseline file must be
  #     refused. Without this, turning the ratchet into a constant would be a
  #     claim in a comment: an appended pair would sail through and the gate
  #     would still print OK. The negative control is the second half — a file
  #     of comments and blanks must read as empty, which is the state the
  #     tracked file is in, so a reader that rejected everything would be red
  #     on a clean tree.
  printf '# a comment\n\npublisher.hpp <memory>\n' > "$d/baseline-dirty.txt"
  cases=$((cases + 1))
  if [ -z "$(baseline_rows "$d/baseline-dirty.txt")" ]; then
    echo "check-cpp-freestanding-includes SELFTEST FAIL: a row in the baseline file read as EMPTY, so the constant is decorative" >&2
    rc=1
  fi
  printf '# only comments\n\n   \n' > "$d/baseline-clean.txt"
  cases=$((cases + 1))
  if [ -n "$(baseline_rows "$d/baseline-clean.txt")" ]; then
    echo "check-cpp-freestanding-includes SELFTEST FAIL: a file of comments and blanks read as non-empty, so the gate would be red on a clean tree" >&2
    rc=1
  fi

  if [ "$rc" -ne 0 ]; then
    echo "check-cpp-freestanding-includes: the walker does not behave as documented; not scanning the tree." >&2
    exit 1
  fi
  echo "check-cpp-freestanding-includes self-test: OK ($cases cases)"
}
selftest

# --- the baseline must hold NO rows (phase-456 W6) ---------------------------
if [ ! -f "$BASELINE" ]; then
    echo "check-cpp-freestanding-includes: missing $BASELINE" >&2
    echo "  It is tracked, so its absence is a PATH bug. The file must EXIST and be" >&2
    echo "  empty of rows; an absent file and an empty one are different states and" >&2
    echo "  only one of them is checked." >&2
    exit 1
fi
baseline_pairs="$(baseline_rows "$BASELINE")"
if [ -n "$baseline_pairs" ]; then
    echo "check-cpp-freestanding-includes: $BASELINE holds row(s), and it must hold none:" >&2
    printf '%s\n' "$baseline_pairs" | sed 's/^/  /' >&2
    echo >&2
    echo "phase-456 W6 turned this ratchet into a CONSTANT. The debt it tracked was" >&2
    echo "paid by issue 1240; what the slot holds now is a place to put the NEXT" >&2
    echo "violation, and this rule has no legitimate exception because the GATED" >&2
    echo "form is always available:" >&2
    echo >&2
    echo "  #if defined(NROS_CPP_STD) || (defined(__STDC_HOSTED__) && __STDC_HOSTED__ \\" >&2
    echo "      && __has_include(<hdr>))" >&2
    echo >&2
    echo "Wrap the include in that, or do without the header on that board. If you" >&2
    echo "believe this is the exception, change the gate with the reason in the" >&2
    echo "commit rather than appending here." >&2
    exit 1
fi

violations=0
unlisted=""

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
        # Every hit is a violation. The "unless it is in the baseline" branch
        # that used to sit here is gone with the slot (phase-456 W6).
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
    echo "(bridge.hpp), or a platform \`#if\` for a backend TU" >&2
    echo "(nros-rmw-cyclonedds/src/internal.hpp does the latter for <chrono>/<thread>)." >&2
    echo "If neither fits, the header is not available on that board: do without it." >&2
    echo >&2
    echo "An \`#elif\` or \`#else\` arm is NOT covered by the \`#if\` above it: that arm runs" >&2
    echo "precisely when the \`#if\` condition is false (issue 1223)." >&2
    exit 1
fi

# The ratchet's other direction -- a listed pair that no longer offends -- is
# gone with the ratchet. It cannot arise: the file holds no rows, asserted
# above before any scanning happens.

count="$(for d in $SCAN_DIRS; do ls "$d"/*.hpp "$d"/*.cpp 2>/dev/null; done | wc -l)"
echo "check-cpp-freestanding-includes: OK ($count file(s) across nros-cpp headers and the Cyclone backend;" \
     "zero ungated hosted STL includes, and $BASELINE holds no rows — phase-456 W6" \
     "made that a CONSTANT rather than a ratchet)"

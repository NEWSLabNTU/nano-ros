#!/usr/bin/env bash
#
# A capability probe may gate a METHOD. It may never change `sizeof`.
#
# phase-417. This MEASURES the rule rather than grepping for it: it compiles a
# probe TU in several configurations and compares the reported `sizeof`. A text
# scanner for "member inside an `#if`" was written first and thrown away — it
# could not tell a member from a local variable, and the file-level include
# guard made every line look conditional (the same false-negative shape that
# made an earlier scanner in this tree report zero).
#
# WHY IT MATTERS. Two TUs of one image disagreeing about a capability is a
# SUPPORTED state, not a misconfiguration:
#
#   * `examples/px4/cpp/bridge/src/modules/nros_uorb_bridge/CMakeLists.txt:123`
#     sets `-DNROS_CPP_STD=1` on ONE module of a larger image, deliberately.
#   * `zephyr/cmake/nros_rmw_cyclonedds.cmake` adds the `cxx-compat` include
#     dir for some targets only, so `__has_include` can answer differently for
#     two TUs of one build.
#
# It already shipped once. `rclcpp::Node` held
# `std::vector<std::shared_ptr<detail::WallTimer>> timers_` behind
# `NROS_CPP_HAS_STD_CHRONO`, reachable only through `NROS_CPP_STD`, so the px4
# bridge module compiled a 3776-byte node while every other TU compiled a
# 3752-byte one. They linked. Each wrote the object through its own layout.
# Restoring both halves reproduces `3752` vs `3776` exactly.
#
# The probes are themselves unreliable — three distinct failure modes measured
# in one day: `<type_traits>` present-but-hollow on Zephyr, `NROS_CPP_STD` set
# by nothing that ships, `<memory>` present-then-`#error` under
# `-ffreestanding` on GCC 13. So this gate does not ask whether a probe is
# right. It removes the class of bug where being wrong changes a layout.
#
# --- WHAT THE FIRST VERSION OF THIS GATE GOT WRONG (issue 1204) --------------
#
# It had exactly ONE measurement arm: compile hosted, force each capability
# macro ON with `-D`, compare against the hosted baseline. But the headers
# self-define every `NROS_CPP_HAS_*` macro under `__has_include(<memory>)` and
# friends (`client.hpp:23-32`, and the same block in `publisher.hpp`,
# `service.hpp`, `subscription.hpp`, `polling_subscription.hpp`), and on a
# hosted compiler those always succeed. Forcing an already-on macro on is a
# strict no-op, so the gate compared the baseline against itself seven times
# per type and reported OK. Issue 1204 proved it by mutation: a
# `#ifdef NROS_CPP_HAS_SHARED_PTR`-gated `double` member added to a real type
# left the gate green at exit 0.
#
# The header comment had even anticipated the hazard in prose and mitigated it
# with a `selftest()` over a SYNTHETIC struct whose macro genuinely starts
# undefined. That selftest passed honestly and said nothing whatsoever about
# the real types — a negative control that cannot fail on the real subject is
# not a control over it. Both lessons are baked in below:
#
#   1. There is now a FREESTANDING arm. The capability macros are genuinely
#      off in exactly one place — `-nostdinc++` against a shim with no
#      `<memory>`/`<string>`/`<vector>`/`<functional>`/`<chrono>`/`<sstream>` —
#      so that, not `-D`, is where a gated member actually diverges. The
#      ThreadX `cxx-compat` shim is the one the `cpp` lane already parses every
#      header against (`just/check/lanes.just:708-725`), so this arm costs a
#      shim that is known to work and no cross toolchain.
#   2. "The type does not compile in this configuration" is no longer a silent
#      `continue`. That reading turned "the type is ABSENT" into "not a layout
#      question", which is backwards for a type that is supposed to exist
#      everywhere. Absence is now a FAILURE unless the type is named in
#      `hosted_only_reason` with a reason.
#   3. The selftest mutates a REAL type in a REAL header — through a temporary
#      copy of the include tree, never the tracked file — and asserts the gate
#      fails, then that it passes again without the injection.
#
# HONEST LIMIT, measured rather than assumed. The exact mutation issue 1204
# used cannot be caught by ANY measurement, and not because this gate is weak:
# `rclcpp::Node` lives inside
# `#if defined(NROS_CPP_HAS_SHARED_PTR) && defined(NROS_CPP_HAS_STD_STRING) &&
# defined(NROS_CPP_HAS_STD_VECTOR) && defined(NROS_CPP_HAS_STD_FUNCTION)`
# (`nros.hpp:447`), so an `#ifdef NROS_CPP_HAS_SHARED_PTR` *inside* that block
# is tautological — the member exists in every configuration where the type
# exists at all, and no layout diverges. A member gated on `NROS_CPP_HAS_STD_
# CHRONO` or `NROS_CPP_HAS_STD_SSTREAM` (not in the guard) IS a real
# divergence, and it is the one the px4 bug was — but reproducing it needs a
# configuration with `<memory>` and without `<chrono>`, which neither shim
# offers and which no host compiler can be talked into, because an include path
# can add a header and never hide one. So `rclcpp::Node` is hosted-only today
# and this gate covers it with the forced-macro arm alone. phase-427 merges the
# three node types into one that must exist freestanding; when it does, the
# type comes off `hosted_only_reason` (which fails loudly the moment it becomes
# measurable) and the freestanding arm covers it like the others.
#
# HOW THE SIZE IS READ. `-fsyntax-only` plus an intentionally incomplete
# template: `ShowSize<sizeof(T)> probe;` makes the compiler print the number in
# its own diagnostic. No link, no run, no library — so this needs nothing the
# header lane does not already have.

set -uo pipefail
cd "$(dirname "$0")/.."

# `nros_grep_q` rather than a bare `grep -q`: this script greps to confirm its
# own selftest mutation applied, and a grep that fails to START must not read as
# "the mutation is absent". That direction turns a broken control into a loud
# failure instead of a green. Issue 0726.
# shellcheck source=lib/grep-q.sh
. "$(dirname "$0")/lib/grep-q.sh"

WORK="$(mktemp -d)"
TU="$WORK/nros_capability_layout_probe.cpp"

# `INC` is deliberately a mutable global rather than a constant: the selftest
# prepends a mutated copy of the include tree to it so that the SAME
# measurement code paths run against the mutated headers. A selftest that
# reimplements the measurement tests the reimplementation.
INC=(-Itarget/nros-cpp-generated
     -Itarget/nros-c-generated
     -Ipackages/api/nros-cpp/include
     -Ipackages/api/nros-c/include
     -Ipackages/platform/nros-platform-api/include)
INC_BASE=("${INC[@]}")

# Types whose layout must not move. Add a type here when it gains members.
# `Result` and `ResultOf<int>` are the two halves of the error channel
# (phase-427 W8). They are the cheapest possible subjects — one enum and one
# value — which is exactly why they belong here: `result.hpp` is the header
# every other one reaches, so a capability gate that grew a member there
# would move a layout in every TU of every image.
TYPES=("rclcpp::Node" "::nros::Node" "::nros::QoS" "::nros::Result"
       "::nros::ResultOf<int>")

# Every capability macro the public headers define for themselves, plus the
# consumer-facing opt-in. Forcing one ON is exactly what px4 does.
CAPS=(NROS_CPP_STD
      NROS_CPP_HAS_SHARED_PTR
      NROS_CPP_HAS_STD_STRING
      NROS_CPP_HAS_STD_VECTOR
      NROS_CPP_HAS_STD_FUNCTION
      NROS_CPP_HAS_STD_CHRONO
      NROS_CPP_HAS_STD_SSTREAM)

# The two configurations. Hosted is c++17 because `rclcpp::Node`'s
# `if constexpr` needs it; the freestanding flags are copied verbatim from the
# `cpp` lane's `-nostdinc++` header-parse arm so the two agree by construction.
HOSTED_FLAGS=(-std=c++17)
THREADX_SHIM=packages/boards/nros-board-threadx-qemu-riscv64/cxx-compat
FREESTANDING_FLAGS=(-std=c++14 -ffreestanding -nostdinc++ -isystem "$THREADX_SHIM")

# The shim is the whole point of the freestanding arm, so its absence must be
# fatal rather than a skip. Same reasoning, same spelling, as the lane that
# parses every header against it: a gate that passes when its subject is
# missing is issue 0232's false green.
if [ ! -d "$THREADX_SHIM" ]; then
    echo "check-cpp-capability-layout: $THREADX_SHIM is MISSING" >&2
    echo "  The freestanding arm is the only configuration where the capability" >&2
    echo "  macros are genuinely off. Without it this gate would pass on absence." >&2
    exit 1
fi

size_of() { # $1 = type, rest = compiler flags (including -std)
    local ty="$1"; shift
    printf '#include <nros/nros.hpp>\ntemplate <int N> struct ShowSize;\nShowSize<static_cast<int>(sizeof(%s))> probe;\n' "$ty" > "$TU"
    c++ -fsyntax-only -fno-exceptions -fno-rtti "$@" "${INC[@]}" "$TU" 2>&1 |
        grep -oE 'ShowSize<[0-9]+' | head -1 | sed 's/ShowSize<//'
}

# --- the hosted-only registry ------------------------------------------------
#
# A type that cannot be measured in a configuration is either a real defect (it
# was supposed to exist there) or a documented fact about the API surface. The
# difference cannot be inferred from the compiler's output, so it is declared
# here, by name, with the reason. Anything not named here that fails to measure
# is a FAILURE — which is the half issue 1204's `continue` got wrong.
#
# Prints the reason on stdout for a hosted-only type, nothing otherwise.
hosted_only_reason() {
    case "$1" in
    "rclcpp::Node")
        # `nros.hpp:447` wraps the whole `rclcpp` node adapter in
        # `#if defined(NROS_CPP_HAS_SHARED_PTR) && defined(NROS_CPP_HAS_STD_STRING)
        #  && defined(NROS_CPP_HAS_STD_VECTOR) && defined(NROS_CPP_HAS_STD_FUNCTION)`,
        # because `std::shared_ptr` is in every one of its signatures.
        # phase-427 ("one node type") merges the three node types into a single
        # `rclcpp::Node` that must exist on freestanding targets; that work item
        # takes this entry off the list, and the check below fails the moment
        # the type becomes measurable so the removal cannot be forgotten.
        echo "guarded by defined(NROS_CPP_HAS_SHARED_PTR) && ... at nros.hpp:447; phase-427 removes this"
        ;;
    *) ;;
    esac
}

# Measures one type in every configuration. Writes any violation to stderr and
# returns 1; returns 0 when the layout is invariant. A baseline that cannot be
# measured at all is fatal here rather than a return, because every later
# comparison is against it.
check_type() {
    local ty="$1" base got fs reason bad=0
    base="$(size_of "$ty" "${HOSTED_FLAGS[@]}")"
    if [ -z "$base" ]; then
        echo "check-cpp-capability-layout: could not measure sizeof($ty) in the baseline configuration" >&2
        echo "  (the probe TU did not compile; this gate would otherwise pass on absence)" >&2
        exit 1
    fi

    # ARM 1 — force each capability macro ON against the hosted baseline. This
    # is what px4 does to one module of an image. On a hosted compiler the
    # headers have already self-defined most of these, so most of the arm is a
    # no-op; that is exactly why arm 2 exists. It is kept because it is the
    # only arm that models the `-DNROS_CPP_STD=1` consumer, and because a macro
    # that BREAKS the compile when forced on is itself a finding.
    for cap in "${CAPS[@]}"; do
        got="$(size_of "$ty" "${HOSTED_FLAGS[@]}" "-D${cap}=1")"
        if [ -z "$got" ]; then
            reason="$(hosted_only_reason "$ty")"
            if [ -n "$reason" ]; then
                echo "  note: $ty is not measurable with -D${cap}=1 — hosted-only ($reason)" >&2
            else
                echo "FAIL: $ty does not compile with -D${cap}=1" >&2
                echo "  px4 sets a capability macro on one module of a real image, so this" >&2
                echo "  configuration has to at least build. An earlier version of this gate" >&2
                echo "  treated it as 'not a layout question' and skipped it (issue 1204)." >&2
                bad=1
            fi
            continue
        fi
        if [ "$got" != "$base" ]; then
            echo "FAIL: sizeof($ty) changes with -D${cap}=1 — ${base} vs ${got}" >&2
            bad=1
        fi
    done

    # ARM 2 — the freestanding configuration, where the macros are genuinely
    # off because `__has_include(<memory>)` and its siblings answer NO against
    # the ThreadX shim. This is the arm with teeth: a member gated on any of
    # the seven macros is present hosted and absent here, so the two sizes
    # disagree.
    fs="$(size_of "$ty" "${FREESTANDING_FLAGS[@]}")"
    reason="$(hosted_only_reason "$ty")"
    if [ -z "$fs" ]; then
        if [ -n "$reason" ]; then
            echo "  note: $ty is hosted-only, freestanding arm skipped — $reason" >&2
        else
            echo "FAIL: $ty cannot be measured against the ThreadX shim, and is not declared hosted-only" >&2
            echo "  A type on the TYPES list is expected to exist on freestanding targets." >&2
            echo "  If it is legitimately hosted-only, say so in hosted_only_reason() with the" >&2
            echo "  reason; if it is not, the absence IS the defect." >&2
            bad=1
        fi
    elif [ -n "$reason" ]; then
        # The ratchet. A stale exemption is how a gate quietly narrows: the
        # reason stops being true, nobody re-reads the list, and the type keeps
        # its skip forever.
        echo "FAIL: $ty is listed hosted-only but DOES measure freestanding (${fs})" >&2
        echo "  Remove it from hosted_only_reason(); its exemption is stale." >&2
        echo "  Recorded reason was: $reason" >&2
        bad=1
    elif [ "$fs" != "$base" ]; then
        echo "FAIL: sizeof($ty) differs hosted vs -nostdinc++ freestanding — ${base} vs ${fs}" >&2
        bad=1
    fi

    return "$bad"
}

# NEGATIVE CONTROL, on the normal path.
#
# The measurement can only be trusted if it is known to FAIL when a layout does
# follow a probe. Case 1 and 2 keep the original synthetic pair — they pin the
# two directions of the rule (a gated MEMBER must diverge, a gated METHOD must
# not) in a standalone TU with no nros headers, which is the cheapest possible
# statement of what the gate believes.
#
# Case 3 is the one issue 1204 said was missing. The synthetic struct passed
# honestly while the real types were being compared against themselves, so the
# control has to reach a real type in a real header. It does that by copying
# the whole `nros-cpp` include tree to a temp dir, injecting a capability-gated
# member into `nros::Node`, prepending the copy to `INC`, and running the SAME
# `check_type` the main loop runs. No tracked header is touched.
#
# The subject is `::nros::Node` rather than `rclcpp::Node` for the reason in the
# header comment: `rclcpp::Node`'s own guard already implies every macro it
# could be gated on, so a mutation there is tautological and diverges nowhere.
selftest() {
    local tu="$WORK/selftest.cpp" a b c d mut before after msg
    cat > "$tu" <<'EOF'
struct Conditional {
    void* always;
#ifdef NROS_SELFTEST_CAP
    double gated_member;
#endif
};
struct Invariant {
    void* always;
#ifdef NROS_SELFTEST_CAP
    void gated_method();
#endif
};
template <int N> struct ShowSize;
#ifdef NROS_SELFTEST_PICK_INVARIANT
ShowSize<static_cast<int>(sizeof(Invariant))> probe;
#else
ShowSize<static_cast<int>(sizeof(Conditional))> probe;
#endif
EOF
    _st() { c++ -fsyntax-only -std=c++17 "$@" "$tu" 2>&1 | grep -oE 'ShowSize<[0-9]+' | head -1 | sed 's/ShowSize<//'; }
    a="$(_st)"; b="$(_st -DNROS_SELFTEST_CAP=1)"
    c="$(_st -DNROS_SELFTEST_PICK_INVARIANT=1)"
    d="$(_st -DNROS_SELFTEST_PICK_INVARIANT=1 -DNROS_SELFTEST_CAP=1)"
    if [ -z "$a" ] || [ -z "$b" ] || [ -z "$c" ] || [ -z "$d" ]; then
        echo "check-cpp-capability-layout --selftest: the size probe produced no number" >&2
        exit 1
    fi
    if [ "$a" = "$b" ]; then
        echo "check-cpp-capability-layout --selftest: a CONDITIONAL MEMBER did not change sizeof ($a vs $b)" >&2
        echo "  The measurement cannot see the defect it exists to catch." >&2
        exit 1
    fi
    if [ "$c" != "$d" ]; then
        echo "check-cpp-capability-layout --selftest: a gated METHOD changed sizeof ($c vs $d)" >&2
        echo "  The gate would fail on the shape it is supposed to permit." >&2
        exit 1
    fi

    # Case 3 — mutate a real header in a throwaway copy of the include tree.
    #
    # The two directions run in this order on purpose. The UNMUTATED copy comes
    # first, because it is what tells the mutated run apart from a broken copy:
    # if `check_type` already fails on a faithful copy, then whatever the
    # mutated run reports afterwards proves nothing.
    mut="$WORK/selftest-include"
    if ! cp -r packages/api/nros-cpp/include "$mut"; then
        echo "check-cpp-capability-layout --selftest: could not copy the include tree to $mut" >&2
        exit 1
    fi
    INC=("-I$mut" "${INC_BASE[@]}")
    check_type '::nros::Node' >/dev/null 2>&1
    before=$?
    INC=("${INC_BASE[@]}")
    if [ "$before" -ne 0 ]; then
        # Two very different causes land here and they must not be conflated.
        # Either the copy is not a faithful stand-in for the real include path
        # (a bug in this selftest), or the tracked headers genuinely violate the
        # rule right now — in which case the main loop below is the right place
        # to say so, with the type name and the sizes, and this control has
        # nothing left to demonstrate.
        check_type '::nros::Node' >/dev/null 2>&1
        if [ $? -ne 0 ]; then
            echo "check-cpp-capability-layout --selftest: case 3 SKIPPED — ::nros::Node already" >&2
            echo "  fails the rule in the tracked headers, so a mutation of it would prove" >&2
            echo "  nothing. The measurement below reports the real violation." >&2
            rm -rf "$mut"
            echo "check-cpp-capability-layout --selftest: 2 case(s) OK (gated member diverges, gated method does not); case 3 skipped, see above"
            return 0
        fi
        echo "check-cpp-capability-layout --selftest: an UNMUTATED copy of the include tree" >&2
        echo "  failed the gate while the tracked tree passed, so case 3 would be measuring" >&2
        echo "  the copy and not the mutation. Fix the copy at $mut." >&2
        exit 1
    fi

    # The anchor is `nros::Node`'s first data member. Injecting after it puts
    # the gated member inside the class body, which is what a real regression
    # looks like.
    sed -i 's|^    nros_cpp_node_t handle_;$|    nros_cpp_node_t handle_;\n#ifdef NROS_CPP_HAS_SHARED_PTR\n    double nros_selftest_mutation_member_;\n#endif|' "$mut/nros/node.hpp"
    nros_grep_q 'nros_selftest_mutation_member_' "$mut/nros/node.hpp"
    if [ $? -eq 1 ]; then
        # A control whose mutation silently failed to apply is a control that
        # cannot fail. If the anchor moved, this says so instead of reporting a
        # green.
        echo "check-cpp-capability-layout --selftest: the mutation did not apply to $mut/nros/node.hpp" >&2
        echo "  The anchor line '    nros_cpp_node_t handle_;' is gone from node.hpp." >&2
        echo "  Re-anchor the injection; without it case 3 proves nothing." >&2
        exit 1
    fi

    INC=("-I$mut" "${INC_BASE[@]}")
    msg="$(check_type '::nros::Node' 2>&1)"
    after=$?
    INC=("${INC_BASE[@]}")
    if [ "$after" -eq 0 ]; then
        echo "check-cpp-capability-layout --selftest: a capability-gated MEMBER injected into" >&2
        echo "  ::nros::Node did not fail the gate. This is issue 1204 exactly — the" >&2
        echo "  measurement is comparing a configuration against itself." >&2
        exit 1
    fi
    case "$msg" in
        *"::nros::Node"*) ;;
        *)
            echo "check-cpp-capability-layout --selftest: the gate failed on the injected member" >&2
            echo "  but its message does not name the type. Reported: $msg" >&2
            exit 1
            ;;
    esac
    rm -rf "$mut"

    echo "check-cpp-capability-layout --selftest: 3 case(s) OK (gated member diverges, gated method does not, real type ::nros::Node caught when mutated)"
}

selftest

fail=0
for ty in "${TYPES[@]}"; do
    check_type "$ty" || fail=1
done

if [ "$fail" -ne 0 ]; then
    cat >&2 <<'MSG'

  A capability probe changed a LAYOUT, or a type that should exist on a
  freestanding target does not. Two TUs of one image are allowed to disagree
  about a capability macro — px4's bridge sets -DNROS_CPP_STD on one module of
  a larger image on purpose — so they would link and then write the object
  through one layout and read it through the other. Silent. Issues 0135, 0460.

  Fix: hold the capability-dependent thing through an UNCONDITIONAL member.
  `rclcpp::Node` keeps wall-timer cells in
  `std::vector<std::shared_ptr<void>> owned_entities_` for exactly this reason,
  instead of the typed `timers_` that used to sit behind a `#ifdef`.
MSG
    exit 1
fi

echo "check-cpp-capability-layout: OK — ${#TYPES[@]} type(s) x ${#CAPS[@]} forced capability macro(s) hosted, plus a -nostdinc++ freestanding measurement against the ThreadX shim"

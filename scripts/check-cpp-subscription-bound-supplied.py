#!/usr/bin/env python3
"""Every C++ subscription registration states its receive bound — phase-456 W7.

WHAT IS BEING MEASURED

Issue 1319 enumerated five `RegistrationPath` rows, and two of them are the C
family: `c_typed_hint` (a bound was supplied) and `c_raw_no_hint` (none was).
The second is priced at the executor's closure buffer rather than at the
message's own size, and the sizing descriptor CANNOT tell them apart — its own
comment says so:

    A C/C++ entry that registers typed supplies `rx_size_bound<M>`; the raw
    no-hint row is a property of an individual call site, not of the image, and
    nothing this writer reads distinguishes them. The typed hint is therefore
    what a C/C++ entry is CREDITED with.

So a single registration site that omits the bound makes the descriptor's credit
untrue for the whole entry, silently and in the UNDER direction. Before W7 five
such sites existed in these headers, every one of them with the message type `M`
sitting in scope as a template parameter.

THE RULE, IN TWO HALVES

1. A call to one of the arena registration entry points must be preceded, in its
   own function body, by an assignment to `rx_buffer_hint` whose right-hand side
   NAMES where the number came from: `rx_buffer_capacity<M>`, `rx_size_bound<M>`,
   a required `rx_bytes` parameter, or `nros::rx_bound_unknown`.

2. `rx_bound_unknown` is the `c_raw_no_hint` row said out loud, and it is legal
   only where no message type is in scope. A function whose template parameter
   list declares `typename M` HAS the type, so it has the bound, and passing the
   unknown from there is the defect this gate exists to catch rather than a
   decision. The tree has exactly one legitimate site — `bind_subscription_raw`,
   whose callback takes bytes and whose type arrives as a NAME — and it is
   admitted by that structural rule, not by an authored allowlist that would
   drift the moment a second one appeared.

A literal `0` is not an accepted right-hand side. It is what the option default
already is, so writing it states nothing that the omission did not.

THE `create_subscription_raw` DEFAULT

Its `rx_bytes` parameter defaulted to 0 until W7, which is how four of the five
sites came to omit the number without anyone writing a 0. A default value for
this parameter is therefore checked for directly: the whole point is that a
caller has to say which row it is taking.

WHY A FAST-LANE GATE

No build, no SDK: a read of tracked headers. `check cpp` is `build-serial`,
which no merge-gating event runs (issues 1225, 1226, 1331), so a gate living
there gates nothing.

Usage::

    check-cpp-subscription-bound-supplied.py
    check-cpp-subscription-bound-supplied.py --selftest
"""

from __future__ import annotations

import os
import re
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
HEADER_DIR = os.path.join("packages", "api", "nros-cpp", "include", "nros")

# The arena registration entry points. A poll-style `nros_cpp_subscription_create`
# takes caller storage and no arena slot, so it has no row to take and is not here.
REGISTER_CALLS = (
    "nros_cpp_subscription_register",
    "nros_cpp_subscription_register_capturing",
    "nros_cpp_subscription_register_with_info",
    "nros_cpp_subscription_register_validated",
)

# A right-hand side that says where the number came from.
ACCEPTED_RHS = (
    "rx_buffer_capacity<",
    "rx_size_bound<",
    "rx_bound_unknown",
    "rx_bytes",
)

HINT_ASSIGN = re.compile(r"\.rx_buffer_hint\s*=\s*(?P<rhs>[^;]+);")
RAW_SIG = re.compile(r"\bcreate_subscription_raw\s*\(")
TEMPLATE_M = re.compile(r"\btemplate\s*<[^>]*\b(?:typename|class)\s+M\b")


def strip_comment(line):
    """`//` comments only. A block comment cannot introduce a call, and the one
    thing this must not do is mistake a doc line naming a function for a call to
    it — `#include "nros/subscription.hpp" // nros_cpp_subscription_register`
    reported as an unbounded registration on the gate's first run."""
    i = line.find("//")
    return line if i < 0 else line[:i]


def header_files():
    d = os.path.join(ROOT, HEADER_DIR)
    return sorted(
        os.path.join(HEADER_DIR, f) for f in os.listdir(d) if f.endswith(".hpp")
    )


def function_window(lines, idx):
    """Lines of the enclosing function body, back to the previous closing brace
    at column 0. Crude, and sufficient: these headers close every function
    definition with a `}` at column 0, so the window never spans two of them."""
    start = 0
    for i in range(idx - 1, -1, -1):
        if lines[i].startswith("}"):
            start = i + 1
            break
    return start, lines[start : idx + 1]


def template_head(window):
    """The `template <...>` line introducing the function this window belongs to.

    The window starts just after the previous function's closing brace, so the
    template header is INSIDE it — searched forward, not backward, which is the
    bug the selftest's third case caught on this gate's first run."""
    for line in window:
        if "template" in line and "<" in line:
            return line
    return ""


def audit_text(rel, text):
    """Returns a list of problem strings. Pure, so the selftest can mutate a
    copy of a real header rather than assert against a fabricated one."""
    problems = []
    lines = text.split("\n")

    for idx, raw_line in enumerate(lines):
        line = strip_comment(raw_line)
        called = next(
            (c for c in REGISTER_CALLS if re.search(r"\b%s\s*\(" % re.escape(c), line)),
            None,
        )
        if called is None:
            continue
        # The PROTOTYPE is not a call. These headers declare the ABI entry
        # points themselves (they are excluded from cbindgen), and a declaration
        # names its return type immediately before the symbol.
        if re.match(r"\s*nros_cpp_ret_t\s+%s\s*\(" % re.escape(called), line):
            continue

        start, window = function_window(lines, idx)
        joined = "\n".join(window)

        m = HINT_ASSIGN.search(joined)
        if not m:
            problems.append(
                "%s:%d: `%s` is called with no `rx_buffer_hint` stated in its function. "
                "This registration takes issue 1319's `c_raw_no_hint` row while the sizing "
                "descriptor credits the entry with a supplied hint. Set "
                "`rx_buffer_hint` from `nros::rx_buffer_capacity<M>::value`, or name "
                "`nros::rx_bound_unknown` if the site genuinely has no type."
                % (rel, idx + 1, called)
            )
            continue

        rhs = m.group("rhs")
        if not any(tok in rhs for tok in ACCEPTED_RHS):
            problems.append(
                "%s:%d: `%s` states `rx_buffer_hint = %s`, which does not say where the "
                "number came from. A literal is not a bound: 0 is what the option default "
                "already is. Accepted right-hand sides name %s."
                % (rel, idx + 1, called, rhs.strip(), ", ".join(ACCEPTED_RHS))
            )
            continue

        if "rx_bound_unknown" in rhs:
            head = template_head(window)
            if TEMPLATE_M.search(head):
                problems.append(
                    "%s:%d: `%s` passes `nros::rx_bound_unknown` from a function that "
                    "declares a message type parameter `M` (`%s`). The type is in scope, "
                    "so the bound is too — `nros::rx_buffer_capacity<M>::value`. The "
                    "unknown row is for a site whose type arrives as a NAME."
                    % (rel, idx + 1, called, head.strip())
                )

    for idx, line in enumerate(lines):
        if not RAW_SIG.search(line):
            continue
        # The declaration carries the parameter list across up to four lines.
        sig = " ".join(lines[idx : idx + 5])
        m = re.search(r"size_t\s+rx_bytes\s*=\s*", sig)
        if m and "inline Result create_subscription_raw" in sig:
            problems.append(
                "%s:%d: `create_subscription_raw` gives `rx_bytes` a DEFAULT. That default "
                "is how four registration sites came to omit the bound without anyone "
                "writing a 0 (phase-456 W7). The parameter is required; a caller with no "
                "type passes `nros::rx_bound_unknown`." % (rel, idx + 1)
            )
    return problems


def count_calls(text):
    """Call sites, excluding the ABI prototypes these headers also declare — the
    same filter `audit_text` applies, so the reported number is the number
    audited and not a larger one that reads like more coverage."""
    n = 0
    for raw_line in text.split("\n"):
        line = strip_comment(raw_line)
        for c in REGISTER_CALLS:
            if not re.search(r"\b%s\s*\(" % re.escape(c), line):
                continue
            if re.match(r"\s*nros_cpp_ret_t\s+%s\s*\(" % re.escape(c), line):
                continue
            n += 1
    return n


def selftest():
    """Four mutations of the real headers, each the shape this gate exists for."""
    failures = []
    sub = os.path.join(ROOT, HEADER_DIR, "subscription.hpp")
    comp = os.path.join(ROOT, HEADER_DIR, "component.hpp")
    sub_text = open(sub, encoding="utf8").read()
    comp_text = open(comp, encoding="utf8").read()

    cases = [
        (
            "a register call with the hint assignment deleted",
            "subscription.hpp",
            sub_text.replace(
                "    ffi_options.rx_buffer_hint = "
                "static_cast<uint32_t>(::nros::rx_buffer_capacity<M>::value);\n",
                "",
                1,
            ),
            "no `rx_buffer_hint` stated",
        ),
        (
            "the hint assigned a bare literal",
            "subscription.hpp",
            sub_text.replace(
                "ffi_options.rx_buffer_hint = "
                "static_cast<uint32_t>(::nros::rx_buffer_capacity<M>::value);",
                "ffi_options.rx_buffer_hint = 0;",
                1,
            ),
            "does not say where the number came from",
        ),
        (
            "`rx_bound_unknown` passed from a function that has `M`",
            "subscription.hpp",
            sub_text.replace(
                "ffi_options.rx_buffer_hint = "
                "static_cast<uint32_t>(::nros::rx_buffer_capacity<M>::value);",
                "ffi_options.rx_buffer_hint = ::nros::rx_bound_unknown;",
                1,
            ),
            "declares a message type parameter",
        ),
        (
            "`create_subscription_raw` regaining its default",
            "component.hpp",
            comp_text.replace(
                "void* ctx, const QoS& qos, size_t rx_bytes) {",
                "void* ctx, const QoS& qos, size_t rx_bytes = 0) {",
                1,
            ),
            "gives `rx_bytes` a DEFAULT",
        ),
    ]

    for label, rel, mutated, expect in cases:
        problems = audit_text(rel, mutated)
        if not any(expect in p for p in problems):
            failures.append(
                "SELFTEST: %s was NOT caught (expected a problem containing %r; got %s)"
                % (label, expect, problems or "nothing")
            )

    # Negative control: the tree as it stands must be clean, or the mutations
    # above prove nothing.
    for rel in (HEADER_DIR + "/subscription.hpp", HEADER_DIR + "/component.hpp"):
        text = open(os.path.join(ROOT, rel), encoding="utf8").read()
        clean = audit_text(rel, text)
        if clean:
            failures.append(
                "SELFTEST: the UNMUTATED %s reports %d problem(s), so a passing mutation "
                "case would prove nothing:\n  %s" % (rel, len(clean), "\n  ".join(clean))
            )
    return failures


def main():
    if "--selftest" in sys.argv:
        failures = selftest()
        if failures:
            print("check-cpp-subscription-bound-supplied --selftest FAILED:", file=sys.stderr)
            for f in failures:
                print("  " + f, file=sys.stderr)
            return 1
        print("check-cpp-subscription-bound-supplied --selftest: 4 mutation case(s) OK")
        return 0

    problems = []
    sites = 0
    for rel in header_files():
        text = open(os.path.join(ROOT, rel), encoding="utf8").read()
        sites += count_calls(text)
        problems.extend(audit_text(rel, text))

    if problems:
        print("", file=sys.stderr)
        print(
            "check-cpp-subscription-bound-supplied: %d registration site(s) do not state "
            "their receive bound:\n" % len(problems),
            file=sys.stderr,
        )
        for p in problems:
            print("  " + p, file=sys.stderr)
        print(
            "\nA subscription registered with no bound takes issue 1319's `c_raw_no_hint` "
            "row, which the sizing descriptor cannot see and therefore credits as if the "
            "hint were supplied. phase-456 W7 made the bound non-optional at every C++ "
            "registration site so that credit is earned rather than assumed.",
            file=sys.stderr,
        )
        return 1

    failures = selftest()
    if failures:
        print("check-cpp-subscription-bound-supplied: SELFTEST FAILED:", file=sys.stderr)
        for f in failures:
            print("  " + f, file=sys.stderr)
        return 1

    print(
        "check-cpp-subscription-bound-supplied: OK — every C++ arena registration states "
        "its receive bound (%d call site(s)); `c_raw_no_hint` is reachable only by naming "
        "`nros::rx_bound_unknown` from a function with no message type in scope" % sites
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())

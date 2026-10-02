#!/usr/bin/env python3
"""Issue 0454 / phase-354 W3 — an FFI taking `goal_cdr` must STRIP its header.

`PollingActionClientCore::send_goal_raw` takes goal FIELDS. Its doc says so
("the serialized goal data, without GoalId framing"), and it appends those bytes
after a header `tx_writer` (`CdrWriter::new_with_header`) has already written.

The C and C++ FFI arms named their parameter `goal_cdr` — CDR, which by
definition begins with the 4-byte encapsulation header — and passed it straight
through. Two headers reached the wire: issue 0448's defect, exported to peers.

The two INTERNAL callers had it right all along
(`strip_cdr_header(goal_data)` -> `goal_fields`), which is what makes this a
NAME-versus-BEHAVIOUR seam rather than a missing idea: one core function, two
call sites that stripped, two that did not, and a parameter name asserting the
contract the FFI then broke.

THE RULE

An `extern "C"` function whose parameter is named `goal_cdr` must call
`strip_cdr_header` in its body — or say in a comment why it does not (the
`tick_ctx` arm is a stub that discards its arguments and returns an error, so it
sends nothing).

WHY A GATE *AND* A TEST

The wire-level demonstration phase-354 W3 owed now exists:
`packages/testing/nros-tests/bins/action-raw-goal-probe` is the C caller these
arms never had (nothing invoked them, which is exactly why the defect survived
review and shipped), and `tests/action_raw_goal_e2e.rs` asserts the EFFECT
against a real peer — the C action server logs the order it decoded, 7 with the
strip and a measured 256 without.

This gate stays because it covers what the test cannot: the C++ arm, and any
future arm added without a peer to point at.
"""

import re
import subprocess
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent / "lib"))
import comments  # noqa: E402

# issue 1617 (W4): EVERY `extern "C" fn` is in the population — `pub`,
# `pub(crate)`, `unsafe` or not. Matching only `pub unsafe extern "C" fn`
# meant dropping `unsafe` from an arm took it out of the gate silently.
FN = re.compile(
    r'(?:pub(?:\([^)]*\))?\s+)?(?:unsafe\s+)?extern\s+"C"\s+fn\s+(\w+)\s*'
    r'\((?P<args>[^)]*)\)[^{;]*\{(?P<body>.*?)\n\}',
    re.S,
)


def arms(src):
    """[(name, ok)] for each goal_cdr-taking extern fn; ok is True when its
    body forwards to `send_goal_raw` only through `strip_cdr_header`."""
    out = []
    for m in FN.finditer(comments.strip_comments(src, "rust")):
        if "goal_cdr" not in m.group("args"):
            continue
        body = m.group("body")
        ok = "send_goal_raw" not in body or "strip_cdr_header" in body
        out.append((m.group(1), ok))
    return out


def self_test():
    good = 'pub unsafe extern "C" fn a(goal_cdr: *const u8) {\n    x.send_goal_raw(strip_cdr_header(s))\n}\n'
    bad = 'pub unsafe extern "C" fn a(goal_cdr: *const u8) {\n    x.send_goal_raw(s)\n}\n'
    assert arms(good) == [("a", True)], arms(good)
    assert arms(bad) == [("a", False)]
    # Dropping `unsafe` / `pub` must not take an arm out of the population.
    assert arms(bad.replace("pub unsafe ", "")) == [("a", False)]
    assert arms(bad.replace("unsafe ", "")) == [("a", False)]
    # A commented strip call is not a strip.
    assert arms(bad.replace("x.send_goal_raw(s)", "x.send_goal_raw(s) // strip_cdr_header")) == [("a", False)]


def main() -> int:
    self_test()
    files = subprocess.run(
        ["git", "grep", "-l", "goal_cdr", "--", "packages/api"],
        capture_output=True, text=True,
    ).stdout.split()

    bad = []
    checked = 0
    for path in files:
        try:
            src = open(path, encoding="utf-8").read()
        except OSError:
            continue
        for m in FN.finditer(src):
            if "goal_cdr" not in m.group("args"):
                continue
            checked += 1
            # COMMENTS OUT FIRST. The explanatory comment on the fixed arms
            # names `strip_cdr_header` in prose, so a naive substring test
            # passed on the text describing the fix while the call itself was
            # gone — the third gate today to report OK on documentation about
            # itself. Match code, not commentary.
            body = re.sub(r"//[^\n]*", "", m.group("body"))
            # The rule keys on SENDING, not on the presence of a stub marker.
            #
            # The first draft exempted any body containing `_ = (goal_cdr` or
            # `NROS_CPP_RET_ERROR`, meaning "this is a stub". That matched the
            # C arm's own `#[cfg(not(feature = "rmw-cffi"))]` fallback, which
            # sits in the SAME function as the live path — so the gate passed on
            # a deliberately reintroduced bug. An arm that never calls
            # `send_goal_raw` cannot double-encapsulate; one that does must
            # strip, whatever else its body contains.
            if "send_goal_raw" not in body:
                continue
            if "strip_cdr_header" in body:
                continue
            bad.append(f"{path}: {m.group(1)}")

    if bad:
        print(
            "ERROR: an FFI takes `goal_cdr` and does not strip its encapsulation header:",
            file=sys.stderr,
        )
        for b in bad:
            print(f"  {b}", file=sys.stderr)
        print(
            "\n  `send_goal_raw` takes goal FIELDS and appends them after a header\n"
            "  `tx_writer` already wrote. Passing CDR through unstripped puts TWO\n"
            "  encapsulation headers on the wire (issues 0448, 0454).\n"
            "  Fix:  core.send_goal_raw(strip_cdr_header(slice))",
            file=sys.stderr,
        )
        return 1

    if checked == 0:
        print(
            "check-goal-cdr-stripped: no `goal_cdr` FFI found — the gate is blind, "
            "the parameter was probably renamed",
            file=sys.stderr,
        )
        return 2

    print(f"goal_cdr stripped: OK ({checked} FFI arm(s) checked)")
    return 0


if __name__ == "__main__":
    sys.exit(main())

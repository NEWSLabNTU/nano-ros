#!/usr/bin/env python3
"""A test that cannot reach a ROS router must SKIP, not fail.

`ZenohRouter::start*` returns `TestResult`, and its `RouterUnavailable` variant
means one specific thing: this host has no `rmw_zenoh_cpp/rmw_zenohd`, so the
lane is not runnable here. `fixtures::or_skip` is the one place that reading
lives — it turns `RouterUnavailable` into a capability skip and leaves every
other error a hard failure, because a router that IS present and refuses to
start is a real fault.

`.expect(...)` on that call collapses the distinction. Thirty-one call sites did
it, and the cost was concrete: `just ci-matrix` reported 7 real failures on a
host with no ROS — six QEMU lanes plus one large-message lane — that were all
the same missing router. Nothing was broken; the suite simply could not tell
"not runnable here" from "broken", so tier 2 could not go green on any machine
without a ROS install. (This repo's ROS lives in a distrobox, and CLAUDE.md
forbids mixing that into the host tree, so "just install ROS" is not the
answer.)

The same shape as issue 0599, which is what `or_skip` was written for. It was
reachable only through the `zenohd()` / `zenohd_unique()` rstest fixtures; every
direct caller went around it.

Usage:
    scripts/check-zenohd-router-skips.py
    scripts/check-zenohd-router-skips.py --self-test
"""

import contextlib
import io
import argparse
import os
import re
import subprocess
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
sys.path.insert(0, os.path.join(ROOT, "scripts", "lib"))
import file_kinds  # noqa: E402  phase-472 W5

CALL = re.compile(r"(?:[A-Za-z_][A-Za-z0-9_]*::)*ZenohRouter::start[a-z_]*")
# What must not follow the call. `unwrap_or_else(|e| skip!(...))` is NOT here on
# purpose: it is over-tolerant rather than under-tolerant (it skips on a router
# that failed to start too), which is a different argument from this one.
BANNED = re.compile(r"\s*\.\s*(expect\(|unwrap\(\))")


def end_of_call(text, open_paren):
    """Index just past the ')' matching `text[open_paren]`, string-aware."""
    depth = 0
    i = open_paren
    while i < len(text):
        c = text[i]
        if c == "(":
            depth += 1
        elif c == ")":
            depth -= 1
            if depth == 0:
                return i + 1
        elif c == '"':
            i += 1
            while i < len(text) and text[i] != '"':
                if text[i] == "\\":
                    i += 1
                i += 1
        i += 1
    return -1


def offenders_in(text):
    """[(line_number, matched_suffix)] for every banned unwrap of a start call."""
    found = []
    pos = 0
    while True:
        m = CALL.search(text, pos)
        if not m:
            return found
        p = text.find("(", m.end())
        if p < 0 or text[m.end():p].strip():
            pos = m.end()
            continue
        end = end_of_call(text, p)
        if end < 0:
            pos = m.end()
            continue
        b = BANNED.match(text[end:])
        if b:
            found.append((text.count("\n", 0, m.start()) + 1, b.group(1)))
        pos = end


def sources():
    """Every tracked `.rs` — by file KIND, not by directory (issue 1544).

    This read `packages/testing` alone, while `ZenohRouter` is a dev-dependency
    any crate can take: six `.expect()`s sat in `packages/rmw/zenoh`'s own tests
    and the gate reported OK. A directory list is a claim about where the
    subject lives, and the subject moved.
    """
    # phase-472 W5 — the shared by-kind population (vendored and generated out).
    return file_kinds.files_of_kind("rust", repo=ROOT)


def self_test():
    bad = []

    if offenders_in('let r = ZenohRouter::start_unique().expect("x");') != [(1, "expect(")]:
        bad.append("a plain .expect on start_unique was not caught")
    if offenders_in("let r = ZenohRouter::start(port).unwrap();") != [(1, "unwrap()")]:
        bad.append(".unwrap() was not caught")
    if offenders_in("let r = or_skip(ZenohRouter::start_unique());"):
        bad.append("the correct or_skip form was flagged")
    # A nested paren and a string containing ')' must not end the call early.
    src = 'let r = ZenohRouter::start_serial(&[a(b), ")"]).expect("x");'
    if offenders_in(src) != [(1, "expect(")]:
        bad.append(f"balanced-paren scan broke: {offenders_in(src)}")
    # `.expect` on something else entirely is not this rule's business.
    if offenders_in('let p = PathBuf::from("x").expect("y");'):
        bad.append("an unrelated .expect was flagged")

    # The population must reach outside `packages/testing` (issue 1544) — a
    # green over a scan that has narrowed back is indistinguishable from a
    # real one without this floor.
    srcs = sources()
    if not any(f.startswith("packages/rmw/") for f in srcs):
        bad.append("the scan no longer reaches packages/rmw/ — sources() has narrowed")
    if not srcs:
        bad.append("the scan found no .rs files at all")

    if bad:
        for b in bad:
            sys.stderr.write("check-zenohd-router-skips --self-test: " + b + "\n")
        return 2
    print("check-zenohd-router-skips --self-test: OK (7 case(s))")
    return 0


def main(argv):
    ap = argparse.ArgumentParser()
    ap.add_argument("--self-test", action="store_true")
    args = ap.parse_args(argv)

    if args.self_test:
        return self_test()
    # Always, not only behind the flag (phase-472 W9): a negative control
    # nobody runs decays into a comment. Quiet on success.
    with contextlib.redirect_stdout(io.StringIO()) as _selftest_out:
        _selftest_rc = self_test()
    if _selftest_rc:
        sys.stdout.write(_selftest_out.getvalue())
        return _selftest_rc

    hits = []
    for rel in sources():
        try:
            text = open(os.path.join(ROOT, rel), encoding="utf-8", errors="replace").read()
        except OSError:
            continue
        for line, what in offenders_in(text):
            hits.append((rel, line, what))

    if hits:
        sys.stderr.write(
            f"ERROR: {len(hits)} ZenohRouter::start* call(s) that turn an absent router "
            "into a failure:\n"
        )
        for rel, line, what in hits:
            sys.stderr.write(f"  {rel}:{line}  .{what}\n")
        sys.stderr.write(
            "\nWrap the call in `fixtures::or_skip(...)`. It skips ONLY on\n"
            "`RouterUnavailable` — a router that is present and will not start still\n"
            "fails, which is the distinction `.expect` throws away.\n"
        )
        return 1

    print(f"check-zenohd-router-skips: OK ({len(sources())} tracked .rs file(s), no bare unwraps)")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))

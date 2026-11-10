#!/usr/bin/env python3
"""No backend status folded into a data flag by hand in the RMW C shim.

Issue 2410 (found by issue 2403). `nros-rmw-cffi` adapts C vtable slots that
return a status plus an out-flag (`has_data`, `has_request`, `take_event`,
`next_deadline_ms`, the in-place probe) to trait methods that answer `bool` or
`Option` with no error channel. The adaptation used to be written by hand:

    rc == NROS_RMW_RET_OK && has
    if rc != NROS_RMW_RET_OK || !taken { continue; }

which makes a FAILING backend give the same answer as a quiet one. On NuttX
every `orb_check` failed with EBADF and `has_data` read "no data", so a uORB
subscription never delivered and nothing said why.

The rule: in `packages/rmw/cffi/src/`, a comparison against `NROS_RMW_RET_OK`
may not be joined with `&&` / `||` to another condition. Route the status
through `status_report::status_ok(rc, &SITE)`, which answers the same boolean
and reports the first error per site through `nros_log`.

A plain `rc == NROS_RMW_RET_OK` (a registration result, an early return on
error) is not this shape and is not matched: the status is the whole answer
there, or the error is handled on its own line.

Buildless. Runs its self-test on every invocation (`--self-test` runs only it).
"""

import argparse
import os
import re
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
SCAN_DIR = os.path.join(ROOT, "packages", "rmw", "cffi", "src")

OK = r"NROS_RMW_RET_OK\b"
PATTERNS = [
    # `rc == OK && x`, `rc != OK || x`
    re.compile(r"(?:==|!=)\s*" + OK + r"\s*(?:&&|\|\|)"),
    # `OK == rc && x`
    re.compile(r"\b" + OK + r"\s*(?:==|!=)\s*[\w.]+\s*(?:&&|\|\|)"),
    # `x && rc == OK`, `x || rc != OK`
    re.compile(r"(?:&&|\|\|)\s*!?[\w.]+\s*(?:==|!=)\s*" + OK),
]


def strip_comments(text):
    """Blank `//` comments, keeping offsets so line numbers stay true."""
    return re.sub(r"//[^\n]*", lambda m: " " * len(m.group(0)), text)


def scan_text(text):
    """Return the 1-based line numbers of every folded status."""
    code = strip_comments(text)
    hits = set()
    for pat in PATTERNS:
        for m in pat.finditer(code):
            hits.add(code.count("\n", 0, m.start()) + 1)
    return sorted(hits)


SELF_TEST = [
    # (source, expected hit count)
    ("rc == NROS_RMW_RET_OK && has", 1),
    ("if rc != NROS_RMW_RET_OK || !taken { continue; }", 1),
    ("if ret != NROS_RMW_RET_OK\n    || !has_deadline {", 1),
    ("NROS_RMW_RET_OK == rc && has", 1),
    ("has && rc == NROS_RMW_RET_OK", 1),
    ("!taken || rc != NROS_RMW_RET_OK", 1),
    ("status_ok(rc, &SITE) && has", 0),
    ("if !status_ok(rc, &SITE) || !taken {", 0),
    ("rc == NROS_RMW_RET_OK", 0),
    ("if rc != NROS_RMW_RET_OK { return Err(e); }", 0),
    ("// rc == NROS_RMW_RET_OK && has (prose)", 0),
]


def self_test():
    failed = 0
    for src, want in SELF_TEST:
        got = len(scan_text(src))
        if got != want:
            failed += 1
            print(f"  self-test: {src!r}: {got} hit(s), expected {want}")
    return failed


def main():
    ap = argparse.ArgumentParser(
        description="No backend status folded into a data flag in the RMW C shim (issue 2410)."
    )
    ap.add_argument("--self-test", action="store_true", help="run only the self-test")
    args = ap.parse_args()

    if self_test():
        print("check-cffi-swallowed-status: self-test FAILED")
        return 1
    if args.self_test:
        print("check-cffi-swallowed-status self-test: OK")
        return 0

    files = sorted(
        os.path.join(SCAN_DIR, f) for f in os.listdir(SCAN_DIR) if f.endswith(".rs")
    )
    if not files:
        print(f"check-cffi-swallowed-status: no .rs files under {SCAN_DIR} - refusing to pass")
        return 1
    bad = []
    for path in files:
        with open(path, encoding="utf-8") as fh:
            for line in scan_text(fh.read()):
                bad.append(f"{os.path.relpath(path, ROOT)}:{line}")
    if bad:
        print("check-cffi-swallowed-status: FAIL - a backend status is folded into a data flag")
        for b in bad:
            print(f"  {b}")
        print(
            "  An error here reads as 'no data' (issue 2410). Write\n"
            "      static SITE: StatusSite = StatusSite::new(\"<slot>\");\n"
            "      status_ok(rc, &SITE) && flag\n"
            "  so the first error is reported (packages/rmw/cffi/src/status_report.rs)."
        )
        return 1
    print(
        f"check-cffi-swallowed-status: OK ({len(files)} file(s), every status on a "
        "no-error-channel path goes through status_ok)"
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())

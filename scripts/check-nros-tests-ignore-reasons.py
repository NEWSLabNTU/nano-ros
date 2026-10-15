#!/usr/bin/env python3
"""Every `#[ignore]` in nros-tests names the issue that decides its fate.

`just test-ignored` runs the `#[ignore]`d tests of every OTHER crate nightly
(issue 1507), so an ignore elsewhere rots in sight. nros-tests is out of that
recipe — its tests need fixtures — so nothing runs an ignored test here, and
its reason string is the only record of why it is off and what would turn it
back on. Two audits (2026-07-17 stale-ignore, 2026-07-28 E4) found ignores
here citing only a phase number, with no open item behind them; the cleanup
that followed deleted four tests that could no longer pass at all (a fixture
file gone, a retired fixture, an empty placeholder body).

So the rule: an `#[ignore]` in `packages/testing/nros-tests` carries a reason
(`#[ignore = "..."]`, never a bare `#[ignore]`) and that reason names an issue
(`issue NNNN`, `#NNNN`). A phase name is not enough: a phase is archived when
it ends, and the ignore outlives it.

Run: python3 scripts/check-nros-tests-ignore-reasons.py [--self-test]
"""

import re
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
SCOPE = "packages/testing/nros-tests"

# An attribute line: `#[ignore]` or `#[ignore = "..."]`, possibly continued
# onto later lines with `\`.
IGNORE = re.compile(r"^\s*#\[ignore\b")
ISSUE = re.compile(r"\bissue[s]?\s+#?\d{3,4}\b|#\d{3,4}\b", re.I)


def attributes(text):
    """(line_no, full attribute text) for every ignore attribute."""
    lines = text.splitlines()
    out = []
    i = 0
    while i < len(lines):
        if IGNORE.match(lines[i]):
            start = i
            attr = lines[i]
            while not attr.rstrip().endswith("]") and i + 1 < len(lines):
                i += 1
                attr += "\n" + lines[i]
            out.append((start + 1, attr))
        i += 1
    return out


def violations(path, text):
    errs = []
    for n, attr in attributes(text):
        if re.match(r"^\s*#\[ignore\s*\]", attr):
            errs.append(f"{path}:{n}: bare `#[ignore]` — say why, and name the issue")
        elif not ISSUE.search(attr):
            errs.append(
                f"{path}:{n}: `#[ignore]` reason names no issue — nothing runs an "
                "ignored nros-tests test, so the reason is its only tracker"
            )
    return errs


def self_test():
    ok = "#[test]\n#[ignore = \"ESP32 support is dormant (issue 1525)\"]\nfn a() {}\n"
    assert violations("x", ok) == [], violations("x", ok)
    hashed = "#[ignore = \"blocked on #233\"]\n"
    assert violations("x", hashed) == []
    bare = "#[test]\n#[ignore]\nfn a() {}\n"
    assert violations("x", bare) and "bare" in violations("x", bare)[0]
    phase = "#[ignore = \"Phase 212.L.5 follow-up\"]\n"
    assert violations("x", phase) and "names no issue" in violations("x", phase)[0]
    # A reason continued over several lines is read whole.
    cont = '#[ignore = "phase 212 M.10: the follow-up \\\n            is issue 1916"]\n'
    assert violations("x", cont) == [], violations("x", cont)
    # A comment that MENTIONS the attribute is not one.
    assert violations("x", "// tests here are #[ignore]d\n") == []


def main():
    self_test()
    if "--self-test" in sys.argv:
        print("check-nros-tests-ignore-reasons --self-test: OK")
        return 0
    files = subprocess.run(
        ["git", "ls-files", "--", f"{SCOPE}/*.rs"],
        cwd=ROOT,
        capture_output=True,
        text=True,
        check=True,
    ).stdout.split()
    if not files:
        print("check-nros-tests-ignore-reasons: FAIL — no .rs files under " + SCOPE)
        return 1
    errs = []
    count = 0
    for f in files:
        text = (ROOT / f).read_text(encoding="utf8")
        count += len(attributes(text))
        errs += violations(f, text)
    if errs:
        print("check-nros-tests-ignore-reasons: FAIL")
        for e in errs:
            print("  " + e)
        return 1
    print(
        f"check-nros-tests-ignore-reasons: OK — {count} ignore(s) across "
        f"{len(files)} file(s), each naming its issue"
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())

#!/usr/bin/env python3
"""A test that PASSES over rows it did not run must have its summary printed.

Issue 2004. Several runtime tests run a whole matrix of cells inside ONE
`#[test]` (`realtime_tiers_e2e`, `sched_dims_applied_e2e`, `entry_e2e`, ...).
A cell outside the run's coordinates is recorded with
`nros_tests::lane_scope::skip_note` and the test still passes when the rest
ran, so the test's own stdout is the only place that says "6 rows ran, 14 out
of lane". Issue 0571 made each of them print that summary for exactly this
reason, since "1 of 16 ran" and "16 of 16 passed" otherwise read the same.

nextest throws a passing test's output away (`success-output = "never"` is its
default), so on every lane the summary existed and nobody could read it. The
tier-2 nightly of 2026-10-22 (run 39739466151) reported `PASS realtime_tiers`
and `PASS sched_dims_applied` and could not say which RTOS cells had run, which
is the one question phase-466 W5 asks of it.

The rule: every test file that calls `skip_note(` to SUMMARISE (as opposed to
passing it straight to `lane_skip!`, which skips the whole test and is printed
as a skip anyway) must be selected by a `.config/nextest.toml` override that
sets `success-output` to show output on success. The subject is derived from
the sources, so a new consolidated test joins the rule by being written.

And no command line may pass `--success-output never`: a nextest CLI flag
outranks every per-test override, so that flag silently undid the rule above
on every lane recipe that ran these binaries. The first fix landed the
override and the summaries still never reached a log (issue 2004, measured on
run-matrix 41013529125); only the junit carried them. Without the flag the
profile decides, and the profile's default for every other binary is already
`never`.
"""

from __future__ import annotations

import argparse
import re
import subprocess
import sys
from pathlib import Path

try:
    import tomllib
except ModuleNotFoundError:  # Python < 3.11
    import tomli as tomllib

TESTS_GLOB = "packages/testing/nros-tests/tests/*.rs"
BINARY_RE = re.compile(r"\bbinary\(\s*=?([A-Za-z0-9_]+)\s*\)")
# nextest's values that print a passing test's output.
SHOWN = {"final", "immediate", "immediate-final"}
# How far back a `lane_skip!(` may sit and still own the `skip_note(` call.
LANE_SKIP_REACH = 3
# Where a nextest command line is written.
CLI_PATHS = ["justfile", "just/*.just", "scripts/**", ".github/workflows/*.yml"]
# `--success-output never`, `--success-output=never`, or a Python argv pair.
CLI_NEVER_RE = re.compile(
    r"""--success-output(?:\s+|=)never\b|["']--success-output["']\s*,\s*["']never["']"""
)


def repo_root() -> Path:
    return Path(
        subprocess.run(
            ["git", "rev-parse", "--show-toplevel"],
            capture_output=True,
            text=True,
            check=True,
        ).stdout.strip()
    )


def summarises(text: str) -> bool:
    """True when the file calls `skip_note(` for a summary, not for `lane_skip!`."""
    lines = text.splitlines()
    for i, line in enumerate(lines):
        code = line.split("//", 1)[0]
        if "skip_note(" not in code or "fn skip_note" in code:
            continue
        window = [ln.split("//", 1)[0] for ln in lines[max(0, i - LANE_SKIP_REACH) : i + 1]]
        if not any("lane_skip!(" in w for w in window):
            return True
    return False


def subjects(root: Path) -> list[str]:
    files = subprocess.run(
        ["git", "ls-files", TESTS_GLOB],
        capture_output=True,
        text=True,
        check=True,
        cwd=root,
    ).stdout.split()
    return sorted(
        Path(f).stem for f in files if summarises((root / f).read_text(encoding="utf-8"))
    )


def shown_binaries(doc: dict) -> set[str]:
    """Binaries a `success-output` override shows on success, in any profile."""
    out: set[str] = set()
    for profile in (doc.get("profile") or {}).values():
        if not isinstance(profile, dict):
            continue
        for ov in profile.get("overrides") or []:
            if not isinstance(ov, dict) or ov.get("success-output") not in SHOWN:
                continue
            # Only a filter that is a plain union of binaries is credited: an
            # `and test(...)` would show some of the binary's tests, not all.
            expr = ov.get("filter", "")
            if not isinstance(expr, str) or re.search(r"\band\b|\bnot\b|test\(", expr):
                continue
            out.update(BINARY_RE.findall(expr))
    return out


def cli_suppressions(text: str) -> list[int]:
    """1-based lines that pass `--success-output never` on a command line."""
    return [
        i
        for i, line in enumerate(text.splitlines(), 1)
        if not line.lstrip().startswith("#") and CLI_NEVER_RE.search(line)
    ]


def cli_offenders(root: Path) -> list[str]:
    files = subprocess.run(
        ["git", "ls-files", "--", *CLI_PATHS],
        capture_output=True,
        text=True,
        check=True,
        cwd=root,
    ).stdout.split()
    me = Path(__file__).resolve()
    out = []
    for f in files:
        path = root / f
        if path.resolve() == me or not path.is_file():
            continue
        try:
            text = path.read_text(encoding="utf-8")
        except UnicodeDecodeError:
            continue
        out.extend(f"{f}:{n}" for n in cli_suppressions(text))
    return out


def check(names: list[str], doc: dict) -> list[str]:
    shown = shown_binaries(doc)
    return [n for n in names if n not in shown]


def selftest() -> int:
    fail = 0

    def want(label: str, got, exp) -> None:
        nonlocal fail
        if got != exp:
            print(f"  [FAIL] {label}: want {exp!r}, got {got!r}")
            fail += 1

    want("a skip_note collected into a summary", summarises(
        "out.push(lane_scope::skip_note(p, l));\nprintln!(\"{} out of lane\", out.len());"), True)
    want("a skip_note handed to lane_skip! skips the whole test", summarises(
        "nros_tests::lane_skip!(\n    \"{}\",\n    lane_scope::skip_note(\n        P, \"x\")\n);"), False)
    want("a skip_note in a comment", summarises("// see lane_scope::skip_note(p, l)"), False)
    want("the helper's own definition", summarises("pub fn skip_note(p: P, l: &str) -> String {"), False)
    doc = tomllib.loads(
        '[[profile.default.overrides]]\nfilter = "binary(a) or binary(=b)"\nsuccess-output = "final"\n'
        '[[profile.default.overrides]]\nfilter = "binary(c)"\nretries = 2\n'
        '[[profile.default.overrides]]\nfilter = "binary(d) and test(x)"\nsuccess-output = "final"\n'
        '[[profile.ci.overrides]]\nfilter = "binary(e)"\nsuccess-output = "immediate"\n'
    )
    want("binaries a shown override credits", sorted(shown_binaries(doc)), ["a", "b", "e"])
    want("a binary with only an unrelated override is missing", check(["a", "c"], doc), ["c"])
    want("a test()-narrowed override does not cover the whole binary", check(["d"], doc), ["d"])
    want("a recipe's CLI suppression", cli_suppressions(
        '    args+=(--failure-output never)\n    args+=(--success-output never --failure-output never)'), [2])
    want("the = spelling and a Python argv pair", cli_suppressions(
        'x --success-output=never\nargs += ["--success-output", "never"]'), [1, 2])
    want("a CLI that SHOWS output, and a comment naming the flag", cli_suppressions(
        'args+=(--success-output final)\n# never pass --success-output never'), [])
    if fail:
        return 1
    print("check-matrix-summary-visible --selftest: OK (10 cases)")
    return 0


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    ap.add_argument("--selftest", action="store_true", help="run the self-test only")
    args = ap.parse_args()
    rc = selftest()
    if rc or args.selftest:
        return rc
    root = repo_root()
    names = subjects(root)
    if not names:
        print("check-matrix-summary-visible: no test calls skip_note( for a summary; "
              "the derivation found nothing, which is not believable", file=sys.stderr)
        return 1
    doc = tomllib.loads((root / ".config/nextest.toml").read_text(encoding="utf-8"))
    rc = 0
    cli = cli_offenders(root)
    if cli:
        print("check-matrix-summary-visible: FAIL -- a command line passes "
              "`--success-output never`, which outranks the override below and hides every "
              "summary it shows (issue 2004):", file=sys.stderr)
        for loc in cli:
            print(f"  {loc}", file=sys.stderr)
        print("  fix: drop the flag; the profile already hides a passing test's output for "
              "every binary the override does not name", file=sys.stderr)
        rc = 1
    missing = check(names, doc)
    if missing:
        print("check-matrix-summary-visible: FAIL -- these tests pass over rows they did not "
              "run, and nextest discards the summary that says so (issue 2004):", file=sys.stderr)
        for n in missing:
            print(f"  {n}", file=sys.stderr)
        print("  fix: add binary(<name>) to the `success-output = \"final\"` override in "
              ".config/nextest.toml", file=sys.stderr)
        return 1
    if rc:
        return rc
    print(f"check-matrix-summary-visible: OK -- {len(names)} summarising test(s), "
          f"each printed on PASS: {', '.join(names)}")
    return 0


if __name__ == "__main__":
    sys.exit(main())

#!/usr/bin/env python3
"""phase-373 W1 — every `test()` / `binary()` predicate in the nextest config
must match at least one real test.

Sibling of `check-nextest-binary-filters.py`, and the split between them is COST,
not scope. That one is static and cheap, so it runs on the fast line, and it
checks `binary()` only — deliberately, because `test()` names are rstest-generated
cases that appear literally nowhere in the sources, and deriving them means
compiling the workspace. Its own note calls that gap real and already bitten.

This gate is that gap. It runs from `test-all`, where the binaries exist and the
compile is already paid for, and asks nextest itself what the names are.

A nextest override that selects NOTHING is not an error and not visible:
`show-config test-groups` prints the override with an empty body, and if the
filter has another disjunct that DOES match, the group looks populated and
healthy. So a filter can rot into a no-op and every reading of the file still
says it works.

That is not hypothetical. `zephyr-qos-port` existed to serialize two tests that
share one baked image and its baked router port (issue #141):

    filter = "(binary(entry_e2e) and test(zephyr_rust_qos)) or binary(qos_zephyr_ros2_interop_e2e)"

`test(zephyr_rust_qos)` matched zero tests from the moment phase-329 W1 folded
`entry_e2e`'s 15 cells into one `entry_matrix`. Only the second disjunct
selected, so `entry_e2e` fell through to a different group, the two stopped
being mutually exclusive, and the flake the group prevents came back — silently,
for six phases, because the group was never empty.

`just check` cannot see any of this: it runs no test list. This gate needs the
binaries, so it belongs to a lane that has them (`test-all`), not the fast line.

Run: python3 scripts/check-nextest-test-filters.py [--self-test]
"""
import json
import re
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
CONFIG = ROOT / ".config" / "nextest.toml"
PRED = re.compile(r"\b(test|binary|binary_id)\(([^)]*)\)")


def predicates(text):
    """(kind, needle, line_no) for every predicate in a `filter = "..."` value."""
    out = []
    for i, line in enumerate(text.splitlines(), 1):
        s = line.lstrip()
        if s.startswith("#"):
            continue
        m = re.match(r"filter\s*=\s*'''(.*)|filter\s*=\s*\"(.*)\"", line)
        if not m:
            continue
        expr = m.group(1) or m.group(2) or ""
        for kind, needle in PRED.findall(expr):
            needle = needle.strip().strip("'\"")
            # `=` / `~` / `/regex/` forms carry their own matching semantics;
            # only the bare substring form is checked here.
            if needle.startswith(("=", "~", "/")):
                continue
            out.append((kind, needle, i))
    return out


def test_index():
    """(binary_ids, (binary_id, test_name)) from `cargo nextest list`.

    JSON, not the human output: that is a flat `<binary-name> <test-path>` per
    line, which loses the binary-ID spelling `binary()` actually matches. The
    first cut of this gate parsed the human form as an indented tree — a shape
    it has never had — and reported every predicate in the file as dead, which
    is at least a loud way to be wrong.
    """
    proc = subprocess.run(
        # No `--all-features`: this workspace has mutually exclusive features
        # (`c-stub-test` vs `posix-c-port` both define the canonical
        # `nros_platform_*` symbols, and the build script `compile_error!`s on
        # the pair). Plain `--workspace` is also what `test-all` lists.
        ["cargo", "nextest", "list", "--workspace", "--message-format", "json"],
        cwd=ROOT,
        capture_output=True,
        text=True,
    )
    if proc.returncode != 0:
        print(
            "check-nextest-test-filters: could not list tests — build them first "
            f"(`cargo nextest list` exited {proc.returncode})",
            file=sys.stderr,
        )
        print(proc.stderr[-2000:], file=sys.stderr)
        sys.exit(2)
    data = json.loads(proc.stdout)
    binaries, tests = set(), set()
    for suite in data.get("rust-suites", {}).values():
        bid = suite.get("binary-id") or ""
        binaries.add(bid)
        binaries.add(suite.get("binary-name") or "")
        for case in suite.get("testcases", {}):
            tests.add((bid, case))
    binaries.discard("")
    if not tests:
        print(
            "check-nextest-test-filters: the test list is EMPTY — refusing to pass.\n"
            "  Every predicate would look dead, which is the same output as a\n"
            "  clean config and must not be reported as one.",
            file=sys.stderr,
        )
        sys.exit(2)
    return binaries, tests


def unmatched(preds, binaries, tests):
    bad = []
    for kind, needle, line in preds:
        if kind in ("binary", "binary_id"):
            hit = any(needle in b for b in binaries)
        else:
            hit = any(needle in name or needle in f"{b}::{name}" for b, name in tests)
        if not hit:
            bad.append((kind, needle, line))
    return bad


def self_test():
    """Both directions — a checker that stopped checking passes silently."""
    text = 'filter = "(binary(a_e2e) and test(gone)) or binary(b_e2e)"\n'
    preds = predicates(text)
    fails = []
    if sorted(preds) != sorted(
        [("binary", "a_e2e", 1), ("test", "gone", 1), ("binary", "b_e2e", 1)]
    ):
        fails.append(f"parse: {preds}")
    binaries = {"crate::a_e2e", "crate::b_e2e"}
    tests = {("crate::a_e2e", "a_matrix"), ("crate::b_e2e", "b_case")}
    bad = unmatched(preds, binaries, tests)
    if [p[1] for p in bad] != ["gone"]:
        fails.append(f"MISSED the dead disjunct: {bad}")
    if unmatched(predicates('filter = "binary(a_e2e)"\n'), binaries, tests):
        fails.append("false positive on a live predicate")
    if predicates('# filter = "test(commented_out)"\n'):
        fails.append("read a commented-out filter")
    if fails:
        print("check-nextest-test-filters --self-test FAILED:")
        print("\n".join("  " + f for f in fails))
        return 1
    print("check-nextest-test-filters --self-test: 4 case(s) OK")
    return 0


def main():
    if "--self-test" in sys.argv:
        return self_test()
    if self_test():
        return 1
    preds = predicates(CONFIG.read_text())
    binaries, tests = test_index()
    bad = unmatched(preds, binaries, tests)
    if bad:
        print("check-nextest-test-filters: FAIL\n", file=sys.stderr)
        for kind, needle, line in bad:
            print(
                f"  .config/nextest.toml:{line}: `{kind}({needle})` matches NOTHING",
                file=sys.stderr,
            )
        print(
            "\n  A predicate that selects nothing makes its disjunct a no-op. If the\n"
            "  filter has another disjunct the override still looks healthy in\n"
            "  `show-config test-groups`, so nothing else will tell you. Either the\n"
            "  test was renamed or folded (point the predicate at what replaced it),\n"
            "  or the override is dead and should go.",
            file=sys.stderr,
        )
        return 1
    print(
        f"check-nextest-test-filters: OK ({len(preds)} predicate(s) over "
        f"{len(binaries)} binaries / {len(tests)} tests)"
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())

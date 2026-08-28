#!/usr/bin/env python3
"""A test that can SKIP must be run by a runner that understands skips.

`nros_tests::skip!` reports an unmet precondition by PANICKING with a
`[SKIPPED]` marker, because Rust's harness has no native skip. Only
`_nextest-tolerant` rewrites that marker into a real skip before tallying; a
bare `cargo test` / `cargo nextest run` counts it as a FAILURE.

So a skip-capable target invoked bare turns an ENVIRONMENT FACT into a red, and
the red is maximally misleading: it names a test, in a change that did not touch
it, on a host that simply lacks a toolchain. That is what made the merge group
fail on `cross_libc_precedence_gate` — a test which was already correct, whose
own comment reads "Skip rather than false-fail".

CLAUDE.md documents the hazard and issue 0673 fixed one instance. Nothing
enforced it, so the ratio drifted to 30 bare invocations against 6 tolerant
ones, and two more skip-capable targets (`staticlib_duplicate_symbols`,
`borrowed_e2e`) sit on the bare path today waiting for a host without their
preconditions.

WHAT IS CHECKED, AND WHAT IS NOT

Only `--test <NAME>` invocations, because those resolve to exactly one file
(`tests/<NAME>.rs`) whose `skip!` calls can be read. `--lib`, `--doc` and
`--workspace` runs are NOT checked: a lib test can skip too, but the target set
is not statically resolvable from the recipe, and a gate that guesses there
would either miss cases or fire on ones it cannot justify. That limit is stated
rather than hidden — the honest scope is the one the evidence supports.

Usage::

    check-skippable-tests-tolerant.py [--selftest]
"""

import os
import re
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
JUSTFILE = os.path.join(ROOT, "justfile")
TESTS_DIR = os.path.join(ROOT, "packages", "testing", "nros-tests", "tests")

# A bare runner: `cargo test …` / `cargo nextest run …`, NOT `just
# _nextest-tolerant …`.
BARE = re.compile(r"^\s*(?!#)(?:[A-Za-z_][A-Za-z0-9_]*=\S+\s+)*cargo\s+(?:test|nextest\s+run)\b")
TEST_ARG = re.compile(r"--test\s+([A-Za-z0-9_]+)")


def skippable(name):
    """Does `tests/<name>.rs` reach `nros_tests::skip!`?"""
    path = os.path.join(TESTS_DIR, f"{name}.rs")
    if not os.path.exists(path):
        return False
    with open(path, encoding="utf8", errors="replace") as fh:
        for line in fh:
            s = line.strip()
            if s.startswith("//"):
                continue
            if "skip!" in s:
                return True
    return False


def scan(justfile_text):
    """[(line_no, recipe_line, [skippable targets])] for bare runs of skippable tests."""
    out = []
    for i, line in enumerate(justfile_text.split("\n"), 1):
        if not BARE.match(line):
            continue
        names = TEST_ARG.findall(line)
        bad = [n for n in names if skippable(n)]
        if bad:
            out.append((i, line.strip(), bad))
    return out


def main():
    if "--selftest" in sys.argv:
        return selftest(verbose=True)
    # Always, not only behind the flag: a negative control nobody runs decays
    # into a comment.
    selftest()

    with open(JUSTFILE, encoding="utf8") as fh:
        text = fh.read()
    problems = scan(text)

    if problems:
        print("check-skippable-tests-tolerant: a skip-capable test is run BARE:\n",
              file=sys.stderr)
        for lineno, line, names in problems:
            print(f"  justfile:{lineno}", file=sys.stderr)
            print(f"    {line[:100]}", file=sys.stderr)
            print(f"    skip-capable: {', '.join(names)}", file=sys.stderr)
        print(
            "\n  `nros_tests::skip!` PANICS with a `[SKIPPED]` marker — Rust's harness\n"
            "  has no native skip — and only `_nextest-tolerant` rewrites that marker\n"
            "  before tallying. A bare runner counts it as a FAILURE, so an unmet\n"
            "  precondition becomes a red naming a test the change never touched.\n"
            "\n"
            "  Route it through the tolerant runner:\n"
            "      just _nextest-tolerant -p nros-tests --no-fail-fast --test <name>\n"
            "\n"
            "  A REAL failure still fails: the tolerance keys on the marker, and a\n"
            "  build/setup error (nextest exit != 100) is never absorbed.",
            file=sys.stderr,
        )
        return 1

    total = len(re.findall(r"--test\s+[A-Za-z0-9_]+", text))
    print(f"check-skippable-tests-tolerant OK — no skip-capable target is run bare "
          f"({total} `--test` reference(s) scanned).")
    return 0


def selftest(verbose=False):
    """Prove it can fail. Runs on every invocation."""
    import tempfile

    global TESTS_DIR
    real = TESTS_DIR
    ok = fail = 0

    def chk(desc, cond):
        nonlocal ok, fail
        if verbose or not cond:
            print(f"  {'ok   ' if cond else 'FAIL '} {desc}")
        if cond:
            ok += 1
        else:
            fail += 1

    with tempfile.TemporaryDirectory() as d:
        globals()["TESTS_DIR"] = d
        with open(os.path.join(d, "skippy.rs"), "w", encoding="utf8") as fh:
            fh.write('fn t() { nros_tests::skip!("no toolchain"); }\n')
        with open(os.path.join(d, "plain.rs"), "w", encoding="utf8") as fh:
            fh.write("fn t() { assert!(true); }\n")
        # A `skip!` that only appears in a COMMENT must not count — otherwise
        # the gate fires on any test that merely explains the hazard.
        with open(os.path.join(d, "commented.rs"), "w", encoding="utf8") as fh:
            fh.write("// we could nros_tests::skip! here but do not\nfn t() {}\n")

        chk("a bare run of a skip-capable target is caught",
            bool(scan("recipe:\n    cargo test -p nros-tests --test skippy\n")))
        chk("a bare run of a NON-skipping target is fine",
            not scan("recipe:\n    cargo test -p nros-tests --test plain\n"))
        chk("the tolerant runner is fine",
            not scan("recipe:\n    just _nextest-tolerant -p nros-tests --test skippy\n"))
        chk("`skip!` in a comment does not count",
            not scan("recipe:\n    cargo test -p nros-tests --test commented\n"))
        chk("`cargo nextest run` is bare too",
            bool(scan("recipe:\n    cargo nextest run -p nros-tests --test skippy\n")))
        chk("an env-prefixed bare run is still bare",
            bool(scan("recipe:\n    FOO=1 cargo test -p nros-tests --test skippy\n")))
        chk("a commented-out recipe line is not a finding",
            not scan("recipe:\n    # cargo test -p nros-tests --test skippy\n"))

    globals()["TESTS_DIR"] = real
    if verbose:
        print(f"\n{ok} passed, {fail} failed")
    if fail:
        print("check-skippable-tests-tolerant self-test: FAILED", file=sys.stderr)
        raise SystemExit(1)
    return 0


if __name__ == "__main__":
    sys.exit(main())

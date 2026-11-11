#!/usr/bin/env python3
"""A test writes an executable stub through the ONE helper (issue 0476).

A test that writes a script with `std::fs::write` and then makes it executable
holds the write descriptor while sibling test threads fork. A child forked in
that window keeps the descriptor until its own exec, so exec'ing the stub fails
with ETXTBSY ("Text file busy"), intermittently, and only under load. The fix
is `nros_launcher::test_support::write_executable_stub` (re-exported as
`crate::test_support::write_executable_stub` in nros-cli-core), which writes the
file from a child process so this process never holds the descriptor.

Issue 0476 fixed one site and added no gate; by 2026-11-07 seven more test
sites had the old shape, and one (`run_exec::a_runner_without_the_verb_or_the_
protocol_is_refused`) failed `just ci gate` with exactly that error.

The rule, over every tracked `.rs` file: inside TEST code (a file under a
`tests/` directory, or anything after a `#[cfg(test)]` line), no
`set_permissions(..., from_mode(0o7..))`. Production code may chmod: it does
not run beside forking test threads. The helper's own module is exempt.
"""

from __future__ import annotations

import re
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
CHMOD_EXEC = re.compile(r"from_mode\(\s*0o7[0-7][0-7]\s*\)")
CFG_ATTR = re.compile(r"^\s*#\[cfg\((?P<pred>.*)\)\]")
STRING = re.compile(r'"[^"]*"')
TEST_WORD = re.compile(r"\btest\b")


def is_test_cfg(line: str) -> bool:
    """A `#[cfg(...)]` that compiles only under `test` (issue 2450).

    `#[cfg(test)]` and `#[cfg(all(test, unix))]` both are. The first version
    matched the literal `#[cfg(test)]` alone, so a module under
    `#[cfg(all(test, unix))]` was never read, and its hand-written stub failed
    3 runs in 30 with ETXTBSY. Not a test cfg: `test` only inside a string
    (`feature = "test-harness"`), under `not(`, or one arm of a top-level
    `any(` (that code is also built without `test`)."""
    m = CFG_ATTR.match(line)
    if not m:
        return False
    pred = STRING.sub('""', m.group("pred")).strip()
    if not TEST_WORD.search(pred) or re.search(r"not\(\s*test\b", pred):
        return False
    return pred == "test" or pred.startswith("all(")
EXEMPT = {
    "packages/cli/nros-launcher/src/test_support.rs": "the helper itself",
    "packages/testing/nros-tests/src/process.rs":
        "makes EMPTY files executable for a path-resolution test; nothing execs them",
}


def offenders(rel: str, text: str) -> list[int]:
    in_test = "/tests/" in f"/{rel}"
    out = []
    for n, line in enumerate(text.split("\n"), 1):
        if is_test_cfg(line):
            in_test = True
        if in_test and CHMOD_EXEC.search(line) and not line.lstrip().startswith("//"):
            out.append(n)
    return out


def self_test() -> None:
    bad = "fn f() {}\n#[cfg(test)]\nmod t {\n  fn s() { set_permissions(&p, Permissions::from_mode(0o755)).unwrap(); }\n}\n"
    assert offenders("a/src/x.rs", bad) == [4], offenders("a/src/x.rs", bad)
    prod = "fn f() { set_permissions(&p, Permissions::from_mode(0o755)).unwrap(); }\n#[cfg(test)]\nmod t {}\n"
    assert offenders("a/src/x.rs", prod) == [], "production chmod was flagged"
    integ = "fn s() { set_permissions(&p, Permissions::from_mode(0o775)).unwrap(); }\n"
    assert offenders("a/tests/y.rs", integ) == [1], "an integration-test chmod was missed"
    assert offenders("a/tests/y.rs", "// from_mode(0o755) in a comment\n") == []
    # Issue 2450: a compound test cfg is a test cfg; these others are not.
    allcfg = "#[cfg(all(test, unix))]\nmod t {\n  fn s() { set_permissions(&p, Permissions::from_mode(0o755)); }\n}\n"
    assert offenders("a/src/x.rs", allcfg) == [3], offenders("a/src/x.rs", allcfg)
    for cfg in ('#[cfg(not(test))]', '#[cfg(any(test, feature = "x"))]',
                '#[cfg(all(feature = "test-harness", unix))]'):
        txt = f"{cfg}\nfn s() {{ set_permissions(&p, Permissions::from_mode(0o755)); }}\n"
        assert offenders("a/src/x.rs", txt) == [], cfg


def main() -> int:
    self_test()
    files = subprocess.run(["git", "ls-files", "*.rs"], cwd=ROOT, capture_output=True,
                           text=True, check=True).stdout.split()
    bad = []
    for rel in files:
        if rel in EXEMPT or rel.startswith("third-party/"):
            continue
        p = ROOT / rel
        if not p.is_file():
            continue
        for n in offenders(rel, p.read_text(encoding="utf-8", errors="replace")):
            bad.append(f"  {rel}:{n}")
    if bad:
        print("check-executable-stub-writes: a test makes a file executable by hand "
              "(issue 0476):", file=sys.stderr)
        print("\n".join(bad), file=sys.stderr)
        print("  Write the stub with `write_executable_stub(&path, body)` "
              "(nros_launcher::test_support):\n  a test that holds the write "
              "descriptor while another test forks gets ETXTBSY on exec.", file=sys.stderr)
        return 1
    print(f"check-executable-stub-writes OK — {len(files)} Rust file(s), every test stub "
          "through the helper")
    return 0


if __name__ == "__main__":
    sys.exit(main())

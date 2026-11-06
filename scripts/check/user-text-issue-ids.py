#!/usr/bin/env python3
"""Issue 2270: an internal tracker id never reaches a user.

A newcomer's first build printed ``(issue 1572)`` and ``(issue 1352)``. Those
numbers mean something to whoever filed them and nothing to the reader, who
cannot open our tracker from a terminal and reads an id as "this is a known
bug". Comments, docs and test assertions keep their ids — that is where an id
is the shortest way to say "the reason is written down over there".

What this reads: Rust STRING LITERALS (plain and raw) outside comments and
outside test code (a ``#[cfg(test)]`` module, inline or ``mod x;``), in the
crates whose text a user sees — the CLI and every build script (``build.rs``,
and the build-helper crates a build script calls). A literal mentioning
``issue NNNN`` fails, unless the literal IS a comment being written into a
generated file (it starts with ``#``, ``//`` or ``/*``): a comment keeps its id
there for the same reason it does in source.

``--self-test`` checks the scanner on inline fixtures, both directions.
"""

from __future__ import annotations

import re
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]

# What a user reads: the CLI's sources, every build script, and the libraries a
# build script calls to print (their `cargo:warning`s are the build's output).
SCOPE_GLOBS = (
    "packages/cli/nros-cli/src/**/*.rs",
    "packages/cli/nros-cli-core/src/**/*.rs",
    "packages/cli/cargo-nano-ros/src/**/*.rs",
    "packages/**/build.rs",
    "packages/rmw/zenoh/nros-zpico-build/src/**/*.rs",
    "packages/tooling/*/src/**/*.rs",
)

ISSUE_ID = re.compile(r"\bissues?\s+#?\d{3,4}\b", re.IGNORECASE)


def literals(text: str):
    """Yield (line, literal body) for every string literal outside comments
    and outside a `#[cfg(test)]` module."""
    i, n, line = 0, len(text), 1
    test_depth = None  # brace depth at which a cfg(test) module opened
    depth = 0
    pending_test = False
    special = re.compile(r"""[\n/#{};'"br]""")
    while i < n:
        m = special.search(text, i)
        if m is None:
            break
        i = m.start()
        c = text[i]
        if c == "\n":
            line += 1
            i += 1
            continue
        if text.startswith("//", i):
            j = text.find("\n", i)
            i = n if j < 0 else j
            continue
        if text.startswith("/*", i):
            j = text.find("*/", i + 2)
            j = n if j < 0 else j + 2
            line += text.count("\n", i, j)
            i = j
            continue
        if text.startswith("#[cfg(test)]", i):
            pending_test = True
            i += len("#[cfg(test)]")
            continue
        if c == "{":
            depth += 1
            if pending_test and test_depth is None:
                test_depth = depth
            pending_test = False
            i += 1
            continue
        if c == "}":
            if test_depth is not None and depth == test_depth:
                test_depth = None
            depth -= 1
            i += 1
            continue
        if c == ";":
            pending_test = False
        # Character literal: skip so a '"' char is not a string start.
        if c == "'":
            m = re.match(r"'(\\.|[^\\'])'", text[i:])
            if m:
                i += m.end()
                continue
            i += 1
            continue
        raw = re.match(r'b?r(#*)"', text[i:])
        if raw:
            hashes = raw.group(1)
            start = i + raw.end()
            end = text.find('"' + hashes, start)
            end = n if end < 0 else end
            body = text[start:end]
            if test_depth is None:
                yield line, body
            line += body.count("\n")
            i = end + 1 + len(hashes)
            continue
        if c == '"' or text.startswith('b"', i):
            start = i + (2 if c == "b" else 1)
            j = start
            while j < n and text[j] != '"':
                j += 2 if text[j] == "\\" else 1
            body = text[start:j]
            if test_depth is None:
                yield line, body
            line += body.count("\n")
            i = j + 1
            continue
        i += 1


COMMENT_LITERAL = re.compile(r"^(?:\\n|\s)*(?:#|//|/\*)")


def scan_text(text: str) -> list[tuple[int, str]]:
    out = []
    for line, body in literals(text):
        if COMMENT_LITERAL.match(body):
            continue
        for m in ISSUE_ID.finditer(body):
            if not on_generated_comment_line(body, m.start()):
                out.append((line, m.group(0)))
                break
    return out


def on_generated_comment_line(body: str, at: int) -> bool:
    """Is position `at` on a line of generated content that is a comment?

    A literal that writes a TOML or CMake file carries `# ...` lines among its
    `\\n`-separated rows; those are comments in the file it writes."""
    start = max(body.rfind("\n", 0, at) + 1, body.rfind("\\n", 0, at) + 2)
    return COMMENT_LITERAL.match(body[start:at]) is not None


TEST_MOD = re.compile(r"#\[cfg\(test\)\]\s*mod\s+(\w+)\s*;")


def test_only_files(files: list[Path]) -> set[Path]:
    """Files a `#[cfg(test)] mod x;` declares: test code, out of scope."""
    out = set()
    for p in files:
        text = p.read_text(encoding="utf-8", errors="replace")
        base = p.parent if p.name in ("mod.rs", "lib.rs", "main.rs") else p.with_suffix("")
        for m in TEST_MOD.finditer(text):
            out.add(base / f"{m.group(1)}.rs")
            out.add(base / m.group(1) / "mod.rs")
    return out


def tracked_files() -> list[Path]:
    out = subprocess.run(
        # `:(glob)` so `**/` matches zero directories too: a plain pathspec
        # `src/**/*.rs` skips every file directly under `src/`, which is how
        # this gate's first version missed `lib.rs` and its siblings.
        ["git", "ls-files", "--", *(f":(glob){g}" for g in SCOPE_GLOBS)],
        cwd=ROOT,
        check=True,
        capture_output=True,
        text=True,
    ).stdout.split()
    return [ROOT / p for p in out]


def self_test() -> int:
    bad = 'fn f() { eprintln!("the pool is full (issue 1572)"); }'
    raw = 'fn f() { let s = r#"see issue 1352"#; }'
    ok_comment = 'fn f() {\n    // issue 1572: why\n    eprintln!("the pool is full");\n}'
    ok_test = '#[cfg(test)]\nmod tests {\n    fn t() { assert!(s.contains("issue 1572")); }\n}'
    ok_char = "fn f() { let q = '\"'; eprintln!(\"plain\"); }"
    after_test = ok_test + '\nfn g() { eprintln!("issue 2270"); }'
    generated_comment = 'fn f() { out.push_str("# why this row (issue 1111)\\n"); }'
    toml_comment = 'fn f() { let t = "[package]\\n# issue 0288 - why\\nname = 1\\n"; }'
    toml_message = 'fn f() { let t = "[package]\\nname = 1 (issue 0288)\\n"; }'
    cases = [
        (bad, 1),
        (raw, 1),
        (ok_comment, 0),
        (ok_test, 0),
        (ok_char, 0),
        (after_test, 1),
        (generated_comment, 0),
        (toml_comment, 0),
        (toml_message, 1),
    ]
    failed = 0
    for text, want in cases:
        got = len(scan_text(text))
        if got != want:
            print(f"self-test FAIL: want {want} got {got}:\n{text}", file=sys.stderr)
            failed += 1
    if failed:
        return 1
    print(f"user-text-issue-ids: self-test OK ({len(cases)} cases)")
    return 0


def main(argv: list[str]) -> int:
    if "--self-test" in argv:
        return self_test()
    # The negative control runs on the normal path too, so it cannot decay.
    if self_test() != 0:
        return 1
    hits = []
    files = tracked_files()
    tests = test_only_files(files)
    for p in files:
        if p in tests:
            continue
        for line, what in scan_text(p.read_text(encoding="utf-8", errors="replace")):
            hits.append(f"{p.relative_to(ROOT)}:{line}: {what}")
    if hits:
        print(
            "user-text-issue-ids: FAIL -- an internal issue id in text a user reads "
            "(issue 2270).\n"
            "  Say what happened and what to do; keep the id in a COMMENT beside the\n"
            "  string, where a maintainer looks for it.",
            file=sys.stderr,
        )
        for h in hits:
            print(f"  {h}", file=sys.stderr)
        return 1
    print("user-text-issue-ids: OK")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))

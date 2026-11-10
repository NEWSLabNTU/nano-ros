#!/usr/bin/env python3
"""Issue 2422: a recursive directory walk in nros-tests names the build-output
predicate, or says why it walks build output on purpose.

`nros_tests::treewalk` is the one answer to "is this directory build output?".
Its header records five walkers that each had their own spelling and did not
agree, and one (`examples_canonical_shape`) blew past nextest's 60 s on a tree
holding built fixtures. Issue 2422 is the sixth: `zephyr_prjconf_requirements`'s
`collect_overlays` recursed into every west build tree and cargo target under
`examples/zephyr` and timed out a tier-2 preflight. The predicate existed; that
walker had simply never been moved onto it, and nothing asked.

So this asks, of every function in `packages/testing/nros-tests/{tests,src}`:

- it is a RECURSIVE walk when its body calls `read_dir` and either calls itself
  or keeps a worklist (`while let Some(..) = <x>.pop()` with a `.push(`);
- such a walk passes when its body names `treewalk::`, `is_skipped_dir` or
  `is_build_output_dir`, or calls a function in the same file whose body does,
  or carries a `treewalk-exempt: <reason>` comment (a walk whose subject IS
  build output: an artifact search under a `target/` or cmake binary dir).

A one-level `read_dir` is not a walk and is not asked about.

`--self-test` runs the scanner over the pre-fix `collect_overlays` (must fail)
and its fixed form and an exempt walker (must pass), so the rule is shown able
to fail before any verdict is believed.

Run:  python3 scripts/check-test-tree-walkers.py [--self-test]
"""

from __future__ import annotations

import re
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
CRATE = ROOT / "packages/testing/nros-tests"
# The predicate's own module defines it and walks nothing.
SKIP = {CRATE / "src/treewalk.rs"}

PREDICATE = re.compile(r"treewalk::|\bis_skipped_dir\b|\bis_build_output_dir\b")
EXEMPT = re.compile(r"treewalk-exempt:\s*\S")
FN = re.compile(r"\bfn\s+([A-Za-z_][A-Za-z0-9_]*)\s*(?:<[^{;]*?>)?\s*\(")
WORKLIST = re.compile(r"while\s+let\s+Some\s*\([^)]*\)\s*=\s*[A-Za-z_][A-Za-z0-9_.]*\.pop\(\)")


def strip_strings(text: str) -> str:
    """Blank string and char literals (keeping length), so braces inside them
    do not unbalance the body scan. Comments are kept: the exemption is one."""
    out = []
    i, n = 0, len(text)
    while i < n:
        c = text[i]
        if text.startswith("//", i):
            j = text.find("\n", i)
            j = n if j < 0 else j
            out.append(text[i:j])
            i = j
        elif c == "r" and re.match(r'r#*"', text[i:]):
            m = re.match(r'r(#*)"', text[i:])
            hashes = m.group(1)
            end = text.find('"' + hashes, i + len(m.group(0)))
            end = n if end < 0 else end + 1 + len(hashes)
            out.append(" " * (end - i))
            i = end
        elif c == '"':
            j = i + 1
            while j < n and text[j] != '"':
                j += 2 if text[j] == "\\" else 1
            out.append(" " * (min(j + 1, n) - i))
            i = j + 1
        elif c == "'" and re.match(r"'(\\.|[^\\'])'", text[i:]):
            m = re.match(r"'(\\.|[^\\'])'", text[i:])
            out.append(" " * len(m.group(0)))
            i += len(m.group(0))
        else:
            out.append(c)
            i += 1
    return "".join(out)


def functions(text: str) -> list[tuple[str, int, str]]:
    """`(name, line, body)` for every fn with a body, nested ones included."""
    clean = strip_strings(text)
    out = []
    for m in FN.finditer(clean):
        brace = clean.find("{", m.end())
        semi = clean.find(";", m.end())
        if brace < 0 or (0 <= semi < brace):
            continue
        depth, j = 0, brace
        while j < len(clean):
            if clean[j] == "{":
                depth += 1
            elif clean[j] == "}":
                depth -= 1
                if depth == 0:
                    break
            j += 1
        line = clean.count("\n", 0, m.start()) + 1
        out.append((m.group(1), line, clean[brace : j + 1]))
    return out


def is_recursive_walk(name: str, body: str) -> bool:
    if "read_dir" not in body:
        return False
    calls_self = re.search(rf"\b{re.escape(name)}\s*\(", body) is not None
    worklist = WORKLIST.search(body) is not None and ".push(" in body
    return calls_self or worklist


def offenders_in(text: str, label: str) -> list[str]:
    fns = functions(text)
    naming = {n for n, _, b in fns if PREDICATE.search(b)}
    out = []
    for name, line, body in fns:
        if not is_recursive_walk(name, body):
            continue
        if PREDICATE.search(body) or EXEMPT.search(body):
            continue
        if any(re.search(rf"\b{re.escape(h)}\s*\(", body) for h in naming if h != name):
            continue
        out.append(f"{label}:{line}: fn {name}")
    return out


def tracked_sources() -> list[Path]:
    """The crate's tracked `tests/*.rs` and `src/**/*.rs`, from the index (a
    walk would stat build output, the very thing this gate is about)."""
    rel = CRATE.relative_to(ROOT)
    listed = subprocess.run(
        ["git", "-C", str(ROOT), "ls-files", "-z", "--", f"{rel}/tests/*.rs", f"{rel}/src/*.rs"],
        check=True,
        capture_output=True,
    ).stdout
    files = [ROOT / p for p in listed.decode().split("\0") if p]
    # `tests/*.rs` in git's pathspec also matches nested dirs; keep the top
    # level, where cargo looks for test targets.
    return sorted(
        f for f in files
        if f.parent != CRATE / "tests" and f.is_relative_to(CRATE / "src")
        or f.parent == CRATE / "tests"
    )


def scan() -> list[str]:
    files = tracked_sources()
    out = []
    for f in files:
        if f in SKIP:
            continue
        out.extend(offenders_in(f.read_text(encoding="utf-8"), str(f.relative_to(ROOT))))
    return out


PRE_FIX = """
fn collect_overlays(dir: &std::path::Path, out: &mut Vec<PathBuf>) {
    let Ok(rd) = std::fs::read_dir(dir) else { return; };
    for e in rd.flatten() {
        let p = e.path();
        if p.is_dir() {
            collect_overlays(&p, out);
        } else if let Some(name) = p.file_name().and_then(|n| n.to_str())
            && name.starts_with("prj-")
        {
            out.push(p);
        }
    }
}
"""

FIXED = PRE_FIX.replace(
    "        if p.is_dir() {\n            collect_overlays",
    "        if p.is_dir() {\n"
    "            if nros_tests::treewalk::is_skipped_dir(&e.file_name().to_string_lossy()) {\n"
    "                continue;\n"
    "            }\n"
    "            collect_overlays",
)

EXEMPT_WALK = """
fn walk(root: &Path) -> Vec<PathBuf> {
    // treewalk-exempt: the root is a cargo build dir and the file sought is build output.
    let mut out = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(p) = stack.pop() {
        if p.is_dir() {
            if let Ok(rd) = fs::read_dir(&p) { for e in rd.flatten() { stack.push(e.path()); } }
        } else { out.push(p); }
    }
    out
}
"""

UNMARKED_WORKLIST = EXEMPT_WALK.replace("treewalk-exempt:", "note:")

VIA_HELPER = """
fn skip(name: &str) -> bool { nros_tests::treewalk::is_skipped_dir(name) }
fn walk(dir: &Path, hits: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(dir) else { return; };
    for e in entries.flatten() {
        let p = e.path();
        if p.is_dir() && !skip("x") { walk(&p, hits); }
    }
}
"""

ONE_LEVEL = """
fn list(dir: &Path) -> Vec<PathBuf> {
    std::fs::read_dir(dir).unwrap().flatten().map(|e| e.path()).collect()
}
"""


def self_test() -> None:
    cases = [
        ("the pre-fix collect_overlays (issue 2422)", PRE_FIX, 1),
        ("collect_overlays with the predicate", FIXED, 0),
        ("an exempt artifact search", EXEMPT_WALK, 0),
        ("the same worklist with no exemption", UNMARKED_WORKLIST, 1),
        ("a walk through a helper that names the predicate", VIA_HELPER, 0),
        ("a one-level read_dir", ONE_LEVEL, 0),
    ]
    for desc, text, want in cases:
        got = len(offenders_in(text, "selftest"))
        assert got == want, f"self-test: {desc}: {got} offender(s), want {want}"


def main(argv: list[str]) -> int:
    self_test()
    if "--self-test" in argv[1:]:
        print("check-test-tree-walkers: self-test ok")
        return 0
    bad = scan()
    if bad:
        print(
            "ERROR: a recursive directory walk in nros-tests names no build-output "
            "predicate (issue 2422):",
            file=sys.stderr,
        )
        for b in bad:
            print(f"  {b}", file=sys.stderr)
        print(
            "\n  A walk that descends into `build*/` or `target*/` reads every\n"
            "  built fixture, which is how `collect_overlays` timed out a tier-2\n"
            "  preflight. Skip build output with\n"
            "      nros_tests::treewalk::is_skipped_dir(name)       (source walks)\n"
            "      nros_tests::treewalk::is_build_output_dir(name)  (walks that want generated/)\n"
            "  or, when build output IS the subject (an artifact search), say so in\n"
            "  the function:  // treewalk-exempt: <why>",
            file=sys.stderr,
        )
        return 1
    print("check-test-tree-walkers: OK (every recursive walk names the predicate or an exemption)")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))

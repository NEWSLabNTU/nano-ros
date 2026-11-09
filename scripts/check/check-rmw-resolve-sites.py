#!/usr/bin/env python3
"""No CLI site answers an RMW name from the baked table — RFC-0071 D16.

## The rule

RFC-0071 D14 (phase-545 W1): every CLI site resolves an RMW name through the
provider scan (`RmwIndex`, `resolve_rmw_in`). The table
`cargo-nano-ros/build.rs` bakes from ONE checkout's `packages/rmw/`
(`RMW_ROWS`) stays only as the compile-time check on the in-tree descriptors
(RFC-0087 D4) and as the test oracle that holds it equal to the scan.

Issue 2295 is what a second answer costs: `nros build` read the table, so an
image declaring `rmw = "acme"` for a workspace provider was refused as an
unknown RMW while `nros dev rmw dispatch acme` resolved it through the scan.
A table that still answers somewhere is two answers to one question, and the
table answers wrong in exactly the case it is consulted for — a workspace
whose provider it does not hold.

## What this gate refuses

In `packages/cli/nros-cli-core/` and `packages/cli/cargo-nano-ros/src/`
(`.rs` files the tree tracks):

* `resolve_rmw(` as a FREE function call or definition. A METHOD call
  (`.resolve_rmw(`) is not it: `cmd/config.rs` calls the knob tree's
  `resolve_rmw` method, which resolves the `rmw` knob GROUP and reads no
  backend table;
* `RMW_ROWS`;
* the static capability helpers that read the table: `known_rmw`,
  `canonical_rmw`, `backend_declares_capability`, `backend_states_fact`,
  `backend_capability_feature`, and `backends_declaring` as a free call
  (`RmwIndex::backends_declaring`, a method on the scan, is the replacement).

## The exceptions are structural, and there is no allowlist

* `cargo-nano-ros/build.rs` generates the table (it is outside `src/`);
* in `cargo-nano-ros/src/rmw_resolver.rs`, the items behind `#[cfg(test)]` —
  the table's lookups and the tests that compare it with the scan. The same
  name OUTSIDE a `#[cfg(test)]` item there is refused like anywhere else.

Comments are stripped first: a doc comment that names a deleted helper is
history, not a call site.

`--self-test` plants each violation (and each look-alike that must pass) on
synthetic inputs; the normal path runs it first.
"""

from __future__ import annotations

import re
import subprocess
import sys
from pathlib import Path

REPO = Path(__file__).resolve().parents[2]

# What the gate reads: tracked `.rs` files under these prefixes.
SCOPES = ("packages/cli/nros-cli-core/", "packages/cli/cargo-nano-ros/src/")

# The one file whose `#[cfg(test)]` items may name the table.
RESOLVER = "packages/cli/cargo-nano-ros/src/rmw_resolver.rs"

# (pattern, what it is). `(?<![.\w])` keeps a METHOD call and a longer
# identifier (`known_rmw_in`, `scaffold_package_rejects_unknown_rmw`) out.
FORBIDDEN: tuple[tuple[re.Pattern[str], str], ...] = (
    (re.compile(r"(?<![.\w])resolve_rmw\s*\("), "the free `resolve_rmw(` (the baked table)"),
    (re.compile(r"\bRMW_ROWS\b"), "`RMW_ROWS`, the baked table"),
    (re.compile(r"(?<![.\w])known_rmw\b(?!_)"), "`known_rmw` (the baked table)"),
    (re.compile(r"(?<![.\w])canonical_rmw\b"), "`canonical_rmw` (the baked table)"),
    (
        re.compile(r"(?<![.\w])backend_declares_capability\b"),
        "`backend_declares_capability` (now `ResolvedRmwIn::declares_capability`)",
    ),
    (
        re.compile(r"(?<![.\w])backend_states_fact\b"),
        "`backend_states_fact` (now `ResolvedRmwIn::states_fact`)",
    ),
    (
        re.compile(r"(?<![.\w])backend_capability_feature\b"),
        "`backend_capability_feature` (now `ResolvedRmwIn::capability_feature`)",
    ),
    (
        re.compile(r"(?<![.\w:])backends_declaring\s*\((?!\s*&?\s*(?:mut\s+)?self\b)"),
        "the free `backends_declaring(` (now `RmwIndex::backends_declaring`)",
    ),
)

_CFG_TEST = re.compile(r"^\s*#\[cfg\(test\)\]")


def strip_comments(text: str) -> list[str]:
    """Each line with its `//` comment removed (string literals respected).

    Block comments are not used in these crates for prose; a `/* … */` is
    stripped too, line by line, so a commented-out call is never a site.
    """
    out: list[str] = []
    in_block = False
    for line in text.splitlines():
        buf: list[str] = []
        i = 0
        in_str = False
        while i < len(line):
            c = line[i]
            if in_block:
                if line.startswith("*/", i):
                    in_block = False
                    i += 2
                    continue
                i += 1
                continue
            if in_str:
                buf.append(c)
                if c == "\\" and i + 1 < len(line):
                    buf.append(line[i + 1])
                    i += 2
                    continue
                if c == '"':
                    in_str = False
                i += 1
                continue
            if line.startswith("//", i):
                break
            if line.startswith("/*", i):
                in_block = True
                i += 2
                continue
            if c == '"':
                in_str = True
            buf.append(c)
            i += 1
        out.append("".join(buf))
    return out


def cfg_test_lines(lines: list[str]) -> set[int]:
    """0-based indices of lines inside an item behind `#[cfg(test)]`.

    The item runs from the attribute to the `;` that ends it (a `use`, a
    `static`) or to the `}` that closes the brace it opens (a `fn`, a `mod`,
    an `impl`, a `struct`), whichever comes first at brace depth zero.
    """
    covered: set[int] = set()
    i = 0
    while i < len(lines):
        if not _CFG_TEST.match(lines[i]):
            i += 1
            continue
        depth = 0
        opened = False
        j = i
        done = False
        while j < len(lines) and not done:
            covered.add(j)
            for c in lines[j] if j != i else "":
                if c == "{":
                    depth += 1
                    opened = True
                elif c == "}":
                    depth -= 1
                    if opened and depth == 0:
                        done = True
                        break
                elif c == ";" and depth == 0 and not opened:
                    done = True
                    break
            j += 1
        i = j
    return covered


def scan(path: str, text: str) -> list[str]:
    """Violations in one file, as `path:line: what`."""
    lines = strip_comments(text)
    exempt = cfg_test_lines(lines) if path == RESOLVER else set()
    errors = []
    for n, line in enumerate(lines):
        if n in exempt:
            continue
        for pat, what in FORBIDDEN:
            if pat.search(line):
                errors.append(f"{path}:{n + 1}: {what}")
    return errors


def tracked_files() -> list[str]:
    out = subprocess.run(
        ["git", "-C", str(REPO), "ls-files", "-z", "--", *SCOPES],
        check=True,
        capture_output=True,
    ).stdout.decode()
    return sorted(f for f in out.split("\0") if f.endswith(".rs"))


def self_test(quiet: bool = False) -> int:
    """Plant each violation and each look-alike, on SYNTHETIC inputs."""
    other = "packages/cli/nros-cli-core/src/orchestration/x.rs"
    must_fail = {
        "free resolve_rmw call": (other, 'let r = resolve_rmw("zenoh")?;\n'),
        "qualified free call": (other, "let r = rmw_resolver::resolve_rmw(n)?;\n"),
        "RMW_ROWS": (other, "for r in RMW_ROWS {}\n"),
        "known_rmw": (other, "let k = known_rmw();\n"),
        "canonical_rmw": (other, "let c = canonical_rmw(x);\n"),
        "capability helper": (other, "if backend_declares_capability(b, c) {}\n"),
        "states_fact helper": (other, "backend_states_fact(b, f);\n"),
        "capability_feature helper": (other, "backend_capability_feature(b, c);\n"),
        "free backends_declaring": (other, 'let b = backends_declaring("safety");\n'),
        "free backends_declaring definition": (other, "pub fn backends_declaring(cap: &str) {}\n"),
        "table outside cfg(test) in the resolver": (
            RESOLVER,
            "pub fn known() -> usize {\n    RMW_ROWS.len()\n}\n",
        ),
        "after a cfg(test) item ends": (
            RESOLVER,
            "#[cfg(test)]\nuse baked::RMW_ROWS;\npub fn f() { resolve_rmw(\"x\"); }\n",
        ),
    }
    must_pass = {
        "method call on the knob tree": (other, "tree.resolve_rmw(&p, None, &e, d)?;\n"),
        "method on the index": (other, 'idx.backends_declaring("safety");\n'),
        "the index's method definition": (
            RESOLVER,
            "impl RmwIndex {\n    pub fn backends_declaring(&self, c: &str) {}\n}\n",
        ),
        "scan-based sibling": (other, "let k = known_rmw_in(&scan);\nresolve_rmw_in(&s, n)?;\n"),
        "longer identifier": (other, "fn scaffold_package_rejects_unknown_rmw() {}\n"),
        "doc comment": (other, "/// was ~~`backend_declares_capability`~~ and `RMW_ROWS`\n"),
        "line comment": (other, "let x = 1; // resolve_rmw(\"zenoh\")\n"),
        "cfg(test) use in the resolver": (RESOLVER, "#[cfg(test)]\nuse baked::RMW_ROWS;\n"),
        "cfg(test) fn in the resolver": (
            RESOLVER,
            "#[cfg(test)]\npub fn known_rmw() -> Vec<&'static str> {\n"
            "    RMW_ROWS.iter().map(|r| r.declared).collect()\n}\n",
        ),
        "cfg(test) mod tests in the resolver": (
            RESOLVER,
            "#[cfg(test)]\nmod tests {\n    #[test]\n    fn t() {\n"
            '        assert!(resolve_rmw("zenoh").is_ok());\n    }\n}\n',
        ),
    }
    bad = []
    for name, (path, text) in must_fail.items():
        if not scan(path, text):
            bad.append(f"planted violation not caught: {name}")
    for name, (path, text) in must_pass.items():
        got = scan(path, text)
        if got:
            bad.append(f"look-alike refused: {name}: {got}")
    if bad:
        print("check-rmw-resolve-sites --self-test: FAIL", file=sys.stderr)
        for b in bad:
            print(f"  - {b}", file=sys.stderr)
        return 1
    if not quiet:
        print(
            f"check-rmw-resolve-sites --self-test: OK ({len(must_fail)} planted "
            f"violations caught, {len(must_pass)} look-alikes passed)"
        )
    return 0


def main() -> int:
    if "--self-test" in sys.argv[1:]:
        return self_test()
    if self_test(quiet=True):
        return self_test()
    files = tracked_files()
    if not files:
        print("check-rmw-resolve-sites: FAIL — no tracked .rs file in scope", file=sys.stderr)
        return 1
    errors = []
    for f in files:
        errors += scan(f, (REPO / f).read_text(encoding="utf-8"))
    if errors:
        print(
            "check-rmw-resolve-sites: FAIL — a CLI site reads the baked RMW table "
            "(RFC-0071 D14/D16, issue 2295):",
            file=sys.stderr,
        )
        for e in errors:
            print(f"  {e}", file=sys.stderr)
        print(
            "  Resolve the name through the provider scan: take an `RmwIndex` "
            "(orchestration::rmw_index) and ask `resolve`/`backends_declaring`; "
            "a capability is `ResolvedRmwIn::declares_capability`.",
            file=sys.stderr,
        )
        return 1
    print(f"check-rmw-resolve-sites: OK ({len(files)} file(s), no baked-table site)")
    return 0


if __name__ == "__main__":
    sys.exit(main())

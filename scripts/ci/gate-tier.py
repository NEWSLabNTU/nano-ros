#!/usr/bin/env python3
"""Which tier `just ci gate` owes this diff: `docs` (`check fast` alone) or `full`.

User ruling, 2026-11-14: a docs-only diff is pushed after `just check fast`;
the full gate (compile, unit, lane contracts) is for anything else. The tier is
chosen by the diff, so it is computed here and printed, never asserted by
whoever runs the gate. There is no environment knob that picks it.

THE DIFF. Every path that differs from the merge base with `origin/main` (or
`--base`): committed on the branch, staged, unstaged, and untracked files that
are not ignored. A file the gate would see is a file that counts.

A PATH IS DOCS when it is

  * under `book/` or `book-internals/` (the two books: pages, SUMMARY, the
    page map, assets; a generated page counts too, because the fast tier's
    staleness gates are what check it against its source), or
  * a `*.md` file anywhere,

unless Rust source reads it as an INPUT, which only the full tier would
notice:
  * a file named in an `include_str!` / `include_bytes!` reaches a compile
    and `rustdoc`;
  * a repo path spelled as a string literal in a `.rs` file
    (`repo_root().join("book/src/guide/platforms/zephyr-west-app.md")` in
    cargo-nano-ros's scaffold tests) is read by a unit test that `check fast`
    never runs. Measured: three such pages on 2026-11-14.
The fast tier's own gates read docs too, and they run either way.

Everything else is `full`, `docs/` data files included: `docs/` holds TOML and
JSON that gates read (`reviewed-pairs.toml`, the edition snapshots), and
reading a non-Markdown file under `docs/` as prose would be a guess.

An empty diff is `full`: nothing to judge is not a docs diff.

Usage:
  gate-tier.py [--base <ref>]     print `docs` or `full`, then one reason line
  gate-tier.py --self-test
"""
from __future__ import annotations

import re
import subprocess
import sys
from pathlib import Path

REPO = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(REPO / "scripts" / "lib"))
import git_grep  # noqa: E402  (raises on a search that did not run)
from git_history import HistoryUnknown, history_git  # noqa: E402  (bounded)
BOOK_DIRS = ("book/", "book-internals/")
INCLUDE = re.compile(r'include_(?:str|bytes)!\s*\(\s*"([^"]+\.md)"')


def _git(*args: str) -> str:
    return subprocess.run(["git", *args], cwd=REPO, check=True,
                          capture_output=True, text=True).stdout


def changed_paths(base: str) -> list[str]:
    mb = _git("merge-base", base, "HEAD").strip()
    # Bounded: the merge base's tree is a history object, which a treeless CI
    # clone fetches lazily. A read that does not finish raises, and the caller
    # takes the full gate.
    paths = set(history_git(REPO, "diff", "--name-only", mb, fetch=True).stdout.split())
    paths |= set(_git("ls-files", "--others", "--exclude-standard").split())
    return sorted(paths)


LITERAL = re.compile(r'"((?:book|book-internals|docs)/[^"\s]+|[^"\s]+\.md)"')


def included_markdown() -> set[str]:
    """Repo-relative documentation files some tracked Rust source reads."""
    out: set[str] = set()
    lit = git_grep.run(["-h", "-o", "-E", r'"((book|book-internals|docs)/[^" ]+|[^" ]+\.md)"', "--", "*.rs"], REPO)
    for m in LITERAL.finditer(lit):
        rel = m.group(1)
        if (REPO / rel).is_file():
            out.add(rel)
    for line in git_grep.lines(["-n", "-E", r'include_(str|bytes)!\s*\(\s*"[^"]+\.md"', "--", "*.rs"], REPO):
        src, _, text = line.partition(":")
        for rel in INCLUDE.findall(text):
            target = (REPO / src).parent / rel
            try:
                out.add(target.resolve().relative_to(REPO).as_posix())
            except ValueError:
                pass
    return out


def tier(paths: list[str], included: set[str]) -> tuple[str, str]:
    if not paths:
        return "full", "no changed path to judge"
    for p in paths:
        if p not in included and (p.startswith(BOOK_DIRS) or p.endswith(".md")):
            continue
        why = "read by Rust source (include_str! or a path literal)" if p in included else "not documentation"
        return "full", f"{p} is {why}"
    return "docs", f"all {len(paths)} changed path(s) are documentation"


def self_test(quiet: bool = False) -> int:
    inc = {"packages/core/nros-core/README.md", "book/src/guide/platforms/zephyr-west-app.md"}
    cases = [
        ("markdown and book pages", ["docs/guides/cpp-api.md", "book/src/a.md", "book-internals/src/b.png"], "docs"),
        ("negative control: docs plus one .rs", ["docs/guides/cpp-api.md", "packages/core/nros-core/src/lib.rs"], "full"),
        ("a README a crate include_str!s", ["packages/core/nros-core/README.md"], "full"),
        ("a non-markdown file under docs/", ["docs/design/reviewed-pairs.toml"], "full"),
        ("a plain README elsewhere", ["examples/linux/rust/talker/README.md"], "docs"),
        ("a book page a Rust test reads", ["book/src/guide/platforms/zephyr-west-app.md"], "full"),
        ("CLAUDE.md alone", ["CLAUDE.md"], "docs"),
        ("an empty diff", [], "full"),
    ]
    fail = 0
    for name, paths, want in cases:
        got, why = tier(paths, inc)
        ok = got == want
        fail |= not ok
        if not quiet or not ok:
            print(f"  {'ok  ' if ok else 'FAIL'} {name}: {got} ({why})", file=sys.stderr if quiet else sys.stdout)
    if fail:
        print("gate-tier self-test: FAILED", file=sys.stderr)
        return 1
    if not quiet:
        print(f"gate-tier self-test: OK ({len(cases)} cases)")
    return 0


def main(argv: list[str]) -> int:
    if argv[:1] == ["--self-test"]:
        return self_test()
    # The negative control runs on the normal path: a classifier that has drifted
    # toward "docs" must not get to choose the tier.
    if self_test(quiet=True) != 0:
        print("full")
        print("the gate-tier self-test failed")
        return 0
    base = "origin/main"
    if argv[:1] == ["--base"] and len(argv) > 1:
        base = argv[1]
    elif argv:
        print(__doc__, file=sys.stderr)
        return 2
    try:
        paths = changed_paths(base)
    except (subprocess.CalledProcessError, HistoryUnknown) as e:
        # Cannot see the diff: the full gate, never a guess toward less.
        print("full")
        print(f"could not diff against {base}: {getattr(e, 'stderr', '') or e}")
        return 0
    try:
        included = included_markdown()
    except git_grep.GitGrepFailed as e:
        print("full")
        print(f"could not search the Rust sources: {e}")
        return 0
    t, why = tier(paths, included)
    print(t)
    print(why)
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))

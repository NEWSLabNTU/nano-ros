#!/usr/bin/env python3
"""A Python `git grep` goes through `scripts/lib/git_grep.py`.

Issue 1816. `git grep` exits 1 for "matched nothing" and 128 for "could not
search" — outside a repository, or on a pattern git's regex engine rejects —
and prints nothing on stdout either way. 13 of the 14 call sites in `scripts/`
turned "git did not run" into "the pattern is absent" — by reading only stdout,
or by mapping any non-zero status to "none found" — so a gate that forbids a
spelling passed over a `git archive` export, where git searched nothing. The
fix is one helper that raises on any status but 0/1; this keeps a raw call from
coming back.

What is checked: a list or tuple LITERAL, anywhere in a tracked `.py` file,
whose string elements include `"git"` followed (at any distance — `-C <dir>`
sits between them) by `"grep"`. That is the shape all fourteen had. A command
built up piecewise is not seen; reaching the helper is what a literal can show,
the same bargain `check-ancestry-truncation` makes.

Not checked: the shell half. `scripts/lib/grep-q.sh` covers `grep -q`, and
a shell `git grep` inside `$(…) || true` swallows 128 the same way. That is
tracked in issue 1816 rather than ruled here.

Run:  python3 scripts/check/check-git-grep-status.py [--self-test]
"""

from __future__ import annotations

import ast
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / "scripts" / "lib"))
import git_grep  # noqa: E402

EXEMPT = {
    "scripts/lib/git_grep.py": "this IS the rule",
    "scripts/check/check-git-grep-status.py": "this gate, whose self-test spells the call",
}


def raw_calls(src: str) -> list[int]:
    """Line numbers of a `["git", …, "grep", …]` literal."""
    try:
        tree = ast.parse(src)
    except SyntaxError:
        return []
    found = []
    for node in ast.walk(tree):
        if not isinstance(node, (ast.List, ast.Tuple)):
            continue
        words = [e.value for e in node.elts
                 if isinstance(e, ast.Constant) and isinstance(e.value, str)]
        if "git" in words and "grep" in words[words.index("git") + 1:]:
            found.append(node.lineno)
    return found


def self_test() -> None:
    assert raw_calls('subprocess.run(["git", "grep", "-n", "x"])') == [1]
    assert raw_calls('subprocess.run(["git", "-C", str(R), "grep", "-l", x])') == [1]
    assert raw_calls('run(("git", "-C", d, "grep", p))') == [1]
    assert raw_calls('git_grep.run(["-n", "x"], ROOT)') == []
    assert raw_calls('subprocess.run(["git", "ls-files"])') == []
    # `grep` alone is a different tool with its own rule (grep-q.sh).
    assert raw_calls('subprocess.run(["grep", "-q", "git", f])') == []
    assert raw_calls('# ["git", "grep"] in a comment') == []


def main() -> int:
    self_test()
    # The helper's own negative controls: a rule nobody runs decays into a comment.
    git_grep.self_test()
    if sys.argv[1:] == ["--self-test"]:
        print("check-git-grep-status: self-test OK")
        return 0
    files = subprocess.run(["git", "-C", str(ROOT), "ls-files", "-z", "--", "*.py",
                            ":!third-party"],
                           capture_output=True, text=True, check=True).stdout.split("\0")
    files = [f for f in files if f]
    assert files, "git ls-files named no Python file — the gate would pass on nothing"
    bad = []
    for rel in files:
        if rel in EXEMPT:
            continue
        try:
            src = (ROOT / rel).read_text(encoding="utf-8")
        except (OSError, UnicodeDecodeError):
            continue
        if "grep" not in src:
            continue
        bad += [f"  {rel}:{n}" for n in raw_calls(src)]
    if bad:
        print("check-git-grep-status: FAIL — a raw `git grep` subprocess reads exit 128 "
              "(\"did not search\") as \"no match\":", file=sys.stderr)
        print("\n".join(bad), file=sys.stderr)
        print("  Use scripts/lib/git_grep.py (run / lines / any_match), which raises "
              "instead (issue 1816).", file=sys.stderr)
        return 1
    print(f"check-git-grep-status: OK — {len(files)} tracked Python file(s), every "
          f"`git grep` through scripts/lib/git_grep.py")
    return 0


if __name__ == "__main__":
    sys.exit(main())

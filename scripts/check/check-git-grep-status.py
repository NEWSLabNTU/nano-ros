#!/usr/bin/env python3
"""Every `git grep` goes through a helper that refuses exit 128.

Python: `scripts/lib/git_grep.py`. Shell: `nros_git_grep` / `nros_git_grep_q`
in `scripts/lib/grep-q.sh`.

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

The shell half (tracked `*.sh`, `*.bash`, `*.just`, `justfile`, workflow
`*.yml`): a `git grep` / `git -C <dir> grep` in COMMAND POSITION — at the
start of a line or after `$(`, `<(`, `|`, `;`, `&&`, `||`, `!`, `if`, `then`,
`do` — on a line that is not a comment. That is every shape the ~40 swept sites
had: `hits="$(git grep … || true)"` and `done < <(git grep …)` both swallow 128
with 1, and `if git grep -q …` reads it as "absent". Prose that NAMES the
command (`` `git grep` ``, `echo "… git grep failed"`) is not in command
position and is not matched. A POSIX `sh` script cannot source the bash helper,
so it spells the status check inline and is listed in `SHELL_EXEMPT` with that
reason.

Run:  python3 scripts/check/check-git-grep-status.py [--self-test]
"""

from __future__ import annotations

import ast
import re
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


SHELL_EXEMPT = {
    "scripts/lib/grep-q.sh": "this IS the rule (nros_git_grep, nros_git_grep_q)",
    "scripts/stage-sdk-root.sh": "POSIX sh: checks `|| gg_rc=$?` against > 1 inline",
}

SHELL_GLOBS = ["*.sh", "*.bash", "*.just", "justfile", ".github/workflows/*.yml"]

SHELL_RAW = re.compile(
    r"(?:^|[;&|(!]|\$\(|<\(|\bif|\bthen|\bdo|\belif)\s*"
    r"git\s+(?:-C\s+\S+\s+)?grep\b"
)


def shell_raw_calls(src: str) -> list[int]:
    """Line numbers of a shell `git grep` in command position."""
    found = []
    for n, line in enumerate(src.splitlines(), 1):
        if line.lstrip().startswith("#"):
            continue
        if SHELL_RAW.search(line):
            found.append(n)
    return found


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
    # The shell shapes the sweep removed, each one red.
    for raw in ['hits="$(git grep -n x -- s || true)"',
                'done < <(git grep -l x -- s 2>/dev/null)',
                'if git grep -q x -- s; then',
                'if ! git -C "$d" grep -q x; then',
                '    git grep -nE x \\',
                'for d in $(git -C "$repo" grep -h -o x HEAD); do',
                'a && git grep x | sort']:
        assert shell_raw_calls(raw) == [1], raw
    # Prose, comments and the helper itself are not calls.
    for ok in ['# hits="$(git grep -n x || true)"',
               'echo "  use `git grep -- <pathspec>`"',
               'echo "nano-ros: git grep for deps failed (rc=$rc)" >&2',
               'nros_git_grep hits -n x -- s',
               'if nros_git_grep_q x -- s; then',
               'git ls-files -- s']:
        assert shell_raw_calls(ok) == [], ok


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
    shell = subprocess.run(["git", "-C", str(ROOT), "ls-files", "-z", "--", *SHELL_GLOBS,
                            ":!third-party"],
                           capture_output=True, text=True, check=True).stdout.split("\0")
    shell = [f for f in shell if f]
    assert shell, "git ls-files named no shell file — the gate would pass on nothing"
    for rel in shell:
        if rel in SHELL_EXEMPT:
            continue
        try:
            src = (ROOT / rel).read_text(encoding="utf-8")
        except (OSError, UnicodeDecodeError):
            continue
        if "grep" not in src:
            continue
        bad += [f"  {rel}:{n}" for n in shell_raw_calls(src)]
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
        print("check-git-grep-status: FAIL — a raw `git grep` reads exit 128 "
              "(\"did not search\") as \"no match\":", file=sys.stderr)
        print("\n".join(bad), file=sys.stderr)
        print("  Python: scripts/lib/git_grep.py (run / lines / any_match). Shell: "
              "`nros_git_grep <var> …` or `nros_git_grep_q …` from scripts/lib/grep-q.sh. "
              "Both exit/raise instead (issue 1816).", file=sys.stderr)
        return 1
    print(f"check-git-grep-status: OK — {len(files)} Python + {len(shell)} shell "
          f"file(s), every `git grep` through a helper")
    return 0


if __name__ == "__main__":
    sys.exit(main())

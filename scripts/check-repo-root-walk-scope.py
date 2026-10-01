#!/usr/bin/env python3
"""issue 1565 — a walk rooted at the checkout root must not count other repositories.

`check-no-tracked-file-find` forbids `find`ing a file git TRACKS, for speed.
This is the correctness half of the same rule. A `find` / `os.walk` / `rglob`
from the checkout root answers "what is on this disk under this path", and in
this checkout that set holds two things that are not this repository:

* `.claude/worktrees/*` — every parallel agent session's linked worktree and
  its build output (measured: 19 copies of a generated header where this tree
  held 1);
* submodules — a walk goes straight through a gitlink into another repository
  (measured: 44 `*.contract.yaml` against 17 tracked, the gap all
  `play_launch`).

So a committed script whose walk is ROOTED AT THE CHECKOUT ROOT must say how it
excludes them: name `.claude` in the same command (`find . -path ./.claude
-prune -o …`), or go through `scripts/lib/repo_walk.py`, which stops at every
nested repository (a directory holding a `.git` entry — worktrees and
submodules alike) and at the top-level `.claude/`. A walk rooted BELOW the root
(a build directory, `examples/`) is not this gate's subject: `.claude/` cannot
be under it, and the cost of a walk there is `check-no-tracked-file-find`'s.

What counts as "the checkout root" is DERIVED per file, never a list of
variable names: a shell variable assigned from `git rev-parse --show-toplevel`
or from `cd "$(dirname "$0")/<..×k>" && pwd` with k reaching the root from
that script's own depth; a Python name assigned from `__file__` with exactly as
many `parents`/`.parent`/`dirname` steps as the file is deep, or from
`--show-toplevel`; `{{justfile_directory()}}`, and `.` in a just recipe. A
root this cannot see (a function parameter called `root`) is out of reach, and
that limit is stated rather than guessed at.

The two errors 1565 records were made in AD-HOC analysis, which no gate reads.
For that, `python3 scripts/lib/repo_walk.py PATTERN…` is the scoped walk, and
`git ls-files` remains the answer for anything tracked.
"""

from __future__ import annotations

import os
import re
import subprocess
import sys
from pathlib import Path

REPO = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(REPO / "scripts" / "lib"))
import comments  # noqa: E402  phase-472 W3 — the one comment stripper

SELF = "scripts/check-repo-root-walk-scope.py"
EXCLUDES = re.compile(r"\.claude|repo_walk")
IDENT = r"[A-Za-z_][A-Za-z0-9_]*"


# --------------------------------------------------------------------------
# which spellings are the checkout root, per file


def depth(rel: str) -> int:
    """Path components between the checkout root and `rel` itself."""
    return len(Path(rel).parts)


def py_root_names(rel: str, code: str) -> set[str]:
    """Module-level names this file binds to the checkout root."""
    names = set()
    for m in re.finditer(rf"^({IDENT})\s*(?::[^=\n]*)?=\s*(.+)$", code, re.M):
        name, expr = m.group(1), m.group(2)
        if "show-toplevel" in expr:
            names.add(name)
            continue
        if "__file__" not in expr:
            continue
        up = 0
        p = re.search(r"\.parents\[(\d+)\]", expr)
        if p:
            up += int(p.group(1)) + 1
        up += len(re.findall(r"\.parent\b", expr))
        up += len(re.findall(r"\bdirname\(", expr))
        if up == depth(rel):
            names.add(name)
    # `ROOT = REPO` and friends.
    for m in re.finditer(rf"^({IDENT})\s*=\s*({IDENT})\s*$", code, re.M):
        if m.group(2) in names:
            names.add(m.group(1))
    return names


def sh_root_names(rel: str, code: str) -> set[str]:
    """Shell variables this file binds to the checkout root."""
    names = set()
    for m in re.finditer(rf"(?:^|[\s;])(?:export\s+|local\s+)?({IDENT})=(\S.*)$", code, re.M):
        name, expr = m.group(1), m.group(2)
        if "show-toplevel" in expr:
            names.add(name)
            continue
        d = re.search(r'dirname\s+"?\$\{?(?:0|BASH_SOURCE(?:\[0\])?)\}?"?\)"?((?:/\.\.)*)', expr)
        if d and 1 + d.group(1).count("..") == depth(rel):
            names.add(name)
    return names


# --------------------------------------------------------------------------
# where the walks are


def py_walks(rel: str, text: str):
    """(line, root expression) for each recursive walk in a Python file."""
    code = comments.strip_comments(text, "python")
    # Docstrings and string contents blanked: a walk QUOTED in prose is not one.
    shape = comments.strip_comments(text, "python", strings=True)
    roots = py_root_names(rel, code)
    root_rx = "|".join(re.escape(n) for n in sorted(roots)) or r"(?!x)x"
    plain = rf"(?:str\(|Path\(|pathlib\.Path\()?\s*(?:{root_rx})\s*\)?"
    cwd = r'(?:"\."|\'\.\'|Path\(\s*["\']\.["\']\s*\)|Path\.cwd\(\)|os\.getcwd\(\))'
    pats = [
        re.compile(rf"\bos\.walk\(\s*(?:{plain}|{cwd})\s*[,)]"),
        re.compile(rf"(?:\b(?:{root_rx})\b|{cwd})\s*\.rglob\("),
        re.compile(rf"(?:\b(?:{root_rx})\b|{cwd})\s*\.glob\(\s*[\"']\*\*"),
        re.compile(r"\bglob\.i?glob\(\s*[\"']\*\*[^\"']*[\"'][^)]*recursive\s*=\s*True"),
    ]
    lines = text.split("\n")
    for pat in pats:
        # Match on the original text, keep only hits whose call is CODE.
        for m in pat.finditer(code):
            if not shape[m.start()].strip():
                continue
            ln = code.count("\n", 0, m.start()) + 1
            yield ln, "\n".join(lines[ln - 1:ln + 6])


def sh_finds(rel: str, text: str, just: bool):
    """(line, command) for each `find` whose first operand is the checkout root."""
    code = comments.strip_comments(text, "just" if just else "sh")
    roots = sh_root_names(rel, code)
    var = "|".join(re.escape(n) for n in sorted(roots))
    alts = [r"\{\{\s*justfile_directory\(\)\s*\}\}", r"\$\(git rev-parse --show-toplevel\)"]
    if var:
        alts.append(rf"\$\{{?(?:{var})\}}?")
    if just:
        alts.append(r"\.")
    root = "|".join(alts)
    operand = re.compile(rf"""^["']?(?:{root})/?["']?$""")
    lines = code.split("\n")
    i = 0
    while i < len(lines):
        start, buf = i, lines[i]
        while buf.rstrip().endswith("\\") and i + 1 < len(lines):
            i += 1
            buf = buf.rstrip()[:-1] + " " + lines[i].strip()
        i += 1
        for m in re.finditer(r"(?:^|[\s;(|&`$])find\s+(.*)", buf):
            head = m.group(1).split("|")[0]
            ops = []
            for tok in head.split():
                if tok.startswith(("-", "(", "!", "\\(")):
                    break
                ops.append(tok)
            if any(operand.match(o) for o in ops):
                yield start + 1, buf
                break


# --------------------------------------------------------------------------


def violations(rel: str, text: str):
    out = []
    if rel.endswith(".py"):
        for ln, window in py_walks(rel, text):
            if not EXCLUDES.search(window):
                out.append((ln, "walk"))
    else:
        just = rel.endswith(".just") or Path(rel).name == "justfile"
        for ln, cmd in sh_finds(rel, text, just):
            if not EXCLUDES.search(cmd):
                out.append((ln, "find"))
    return out


def files():
    r = subprocess.run(
        ["git", "-C", str(REPO), "ls-files", "--", "*.py", "*.sh", "*.just", "justfile"],
        capture_output=True, text=True, check=True,
    )
    return [
        f for f in r.stdout.split()
        if not f.startswith("third-party/") and "/third-party/" not in f and f != SELF
    ]


def self_test() -> list[str]:
    """Planted violations go red, scoped walks stay green — over synthetic
    files at real depths, so the root derivation is what is under test."""
    cases = [
        # (rel, text, want_violation, label)
        ("justfile", "x:\n    find . -name 'nros_declared_qos_generated.h'\n", True,
         "just recipe: find . (cwd is the checkout root)"),
        ("justfile", "x:\n    find . -path ./.claude -prune -o -name '*.h' -print\n", False,
         "just recipe: find . that prunes .claude"),
        ("just/check/x.just", 'x:\n    find "{{justfile_directory()}}" -name "*.yaml"\n', True,
         "justfile_directory() as the root"),
        ("just/check/x.just", 'x:\n    find "{{justfile_directory()}}/build" -name "*.h"\n', False,
         "negative control: scoped to a build directory"),
        ("scripts/a.sh", 'repo_root="$(git rev-parse --show-toplevel)"\nfind "$repo_root" \\\n    -name "*.contract.yaml"\n', True,
         "sh: --show-toplevel variable, continuation-joined"),
        ("scripts/check/a.sh", 'ROOT="$(cd "$(dirname "$0")/../.." && pwd)"\nfind "$ROOT" -type f | wc -l\n', True,
         "sh: dirname/.. root derived from the script's own depth"),
        ("scripts/check/a.sh", 'ROOT="$(cd "$(dirname "$0")/.." && pwd)"\nfind "$ROOT" -type f\n', False,
         "sh: one `..` from scripts/check/ is scripts/, not the root"),
        ("scripts/a.sh", 'root="$1"\nfind "$root" -name "*.c"\n', False,
         "sh: a root this file does not bind to the checkout"),
        ("scripts/a.sh", 'repo_root="$(git rev-parse --show-toplevel)"\nfind "$repo_root/build" -name "*.o"\n', False,
         "negative control: sh walk under a build directory"),
        ("scripts/check/x.py", "from pathlib import Path\nREPO = Path(__file__).resolve().parents[2]\nn = len(list(REPO.rglob('*.contract.yaml')))\n", True,  # walk-ok: fixture TEXT for the self-test, never executed
         "py: REPO.rglob at parents[depth-1]"),
        ("scripts/x.py", "import os\nROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))\nfor d, dn, fn in os.walk(ROOT):\n    pass\n", True,  # walk-ok: fixture TEXT for the self-test, never executed
         "py: os.walk over a dirname-chain root"),
        ("scripts/x.py", "import os\nROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))\nfor d, dn, fn in os.walk(ROOT):\n    dn[:] = [x for x in dn if x != '.claude']\n", False,  # walk-ok: fixture TEXT for the self-test, never executed
         "py: the same walk pruning .claude"),
        ("scripts/x.py", "import os\nfrom repo_walk import walk\nROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))\nfor d, dn, fn in walk(ROOT):\n    pass\nfor d, dn, fn in os.walk(ROOT):  # repo_walk.prune below\n    pass\n", False,  # walk-ok: fixture TEXT for the self-test, never executed
         "py: through repo_walk"),
        ("scripts/x.py", "from pathlib import Path\nREPO = Path(__file__).resolve().parents[1]\nn = list((REPO / 'build').rglob('*.h'))\n", False,  # walk-ok: fixture TEXT for the self-test, never executed
         "negative control: py walk under a build directory"),
        ("scripts/x.py", "from pathlib import Path\nREPO = Path(__file__).resolve().parents[1]\n'''REPO.rglob('*') is what NOT to do'''\n", False,
         "py: a walk quoted in a docstring is prose"),
        ("scripts/x.py", "import glob\nfs = glob.glob('**/*.h', recursive=True)\n", True,  # walk-ok: fixture TEXT for the self-test, never executed
         "py: a cwd-relative recursive glob"),
    ]
    bad = []
    for rel, text, want, label in cases:
        got = bool(violations(rel, text))
        if got != want:
            bad.append(f"  self-test FAIL: {label} — got {'a violation' if got else 'none'}")
    return bad


def main() -> int:
    bad = self_test()
    if bad:
        print("check-repo-root-walk-scope: self-test failed:", file=sys.stderr)
        print("\n".join(bad), file=sys.stderr)
        return 2
    if "--self-test" in sys.argv:
        print("check-repo-root-walk-scope: self-test OK")
        return 0
    problems = []
    for rel in files():
        try:
            text = (REPO / rel).read_text(encoding="utf-8", errors="replace")
        except OSError:
            continue
        for ln, kind in violations(rel, text):
            problems.append(f"  {rel}:{ln}: {kind} rooted at the checkout root")
    if problems:
        print("check-repo-root-walk-scope FAILED (issue 1565):", file=sys.stderr)
        print("\n".join(problems), file=sys.stderr)
        print(
            "\n  A walk from the checkout root also counts every agent worktree under\n"
            "  .claude/worktrees/ and every submodule's tree — a different repository.\n"
            "  For tracked content use `git ls-files` / `git grep`. For untracked\n"
            "  artifacts, scope the walk to a build directory, or go through\n"
            "  scripts/lib/repo_walk.py (stops at every nested repository), or name\n"
            "  `.claude` in the command (`find . -path ./.claude -prune -o …`).",
            file=sys.stderr,
        )
        return 1
    print("check-repo-root-walk-scope OK — no committed walk from the checkout root "
          "crosses into .claude/worktrees")
    return 0


if __name__ == "__main__":
    sys.exit(main())

#!/usr/bin/env python3
"""A git read of OLD trees or blobs goes through the bounded history helper.

Issue 2442. CI provisions history as `git fetch --filter=tree:0 --unshallow`
(scripts/ci/provision-gate-lane.sh): every commit is whole and the clone is
not shallow, so the truncation rule (issue 1476) lets it through, but every
tree and blob below the fetched tips lives on the promisor remote. A git
command that needs one fetches it LAZILY, one object per round trip, from
inside the command. `check-toolchain-paths-persist`'s
`git log --diff-filter=DR --name-status -- cmake/toolchain/` ran into the
40-minute nextest cap in CI's `check / fast` that way.

Commit-only questions (`merge-base`, `rev-list`, `log --format` without a
path or a diff, `cat-file -e` on a commit) are fine in such a clone and are
not this gate's business.

# What is checked

Every git invocation in a tracked `scripts/**` `.py`/`.sh` file, a
`just/**/*.just` recipe or the root `justfile` whose arguments make it read an old tree or blob:

  * `log` / `whatchanged` with a pathspec (`--`) or a diff output
    (`--name-status`, `--name-only`, `--stat`, `-p`, `--follow`,
    `--diff-filter`, `-S`, `-G`, ...)
  * `ls-tree`, `diff-tree`, `archive` of a revision that is not `HEAD`
  * `show` of anything but `HEAD`/the index, unless `-s`/`--no-patch`
  * `diff` naming any revision but `HEAD`
  * `grep` over a revision that is not `HEAD`
  * `rev-parse <rev>:<path>` of a revision that is not `HEAD`
  * `cat-file -p` / `cat-file blob|tree`, and `blame`

must instead go through `scripts/lib/git_history.py` (`history_git`, after
`history_objects_unreadable` or `tree_present`) or its shell twin
`scripts/lib/git-history.sh` (`nros_git_history_run`, after
`nros_git_history_objects_unreadable` or `nros_git_tree_present`), or be in
EXEMPT with a reason that makes it provably safe. Python is read through
`ast`, so prose that NAMES a command is never an invocation; shell is read
per line, and a `git` counts only in command position.

Run:  python3 scripts/check/check-history-object-reads.py [--self-test]
"""

from __future__ import annotations

import ast
import re
import shlex
import subprocess
import sys
import warnings
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]

# The helper itself is where the raw reads live.
HELPERS = {"scripts/lib/git_history.py", "scripts/lib/git-history.sh"}

# (file, verb) -> why that read is provably safe. Keyed by verb, not line, so
# an edit nearby does not invalidate it; a NEW risky verb in the file is still
# refused.
EXEMPT = {
    ("scripts/ci/pin-baseline.sh", "cat-file"):
        "`cat-file -p <commit>` reads a COMMIT object (its first `parent` "
        "line), never a tree or blob; commits are whole in a treeless clone",
    ("scripts/audit-nets.py", "diff"):
        "not a gate: the /audit skill runs `--since <baseline>` by hand in a "
        "developer checkout, which is a full clone",
    ("scripts/ci/fixture-tree.sh", "diff"):
        "not a gate, and not this repository: it diffs two commits of the "
        "separate fixture tree, which `fixture-tree.sh sync` fetches whole",
    ("scripts/probe/run-bootstrap-probe.sh", "show"):
        "not a gate: `show <under_test>:<file>` reads the tip under test "
        "(HEAD or PROBE_BRANCH), and the daily host-tests.yml checkout track checks "
        "out with fetch-depth: 0 (a full clone)",
}

LOG_FLAGS = ("--name-status", "--name-only", "--stat", "--numstat",
             "--shortstat", "--dirstat", "-p", "-u", "--patch", "--follow",
             "--raw", "--summary", "--diff-filter", "-S", "-G", "--find-renames",
             "-M", "-C", "--full-diff", "--cc", "-m")
HEADISH = re.compile(r"^(HEAD|HEAD:.*|:.*|--cached|--staged)$")
EXPR = "<expr>"
REVLIKE = re.compile(r"^(origin/\S+|HEAD[~^]\S*|\S+@\{\S*\}|[0-9a-f]{7,40}|\S+\.\.\.?\S*)$")


def _strip_locators(args: list[str]) -> list[str]:
    """Drop git's global options so args[0] is the verb."""
    i = 0
    while i < len(args):
        a = args[i]
        if a in ("-C", "-c", "--git-dir", "--work-tree", "--namespace"):
            i += 2
        elif a.startswith(("--git-dir=", "--work-tree=", "--namespace=",
                           "--no-pager", "--literal-pathspecs", "--bare")):
            i += 1
        elif a == "--no-replace-objects" or a == "-P" or a == "--paginate":
            i += 1
        else:
            break
    return args[i:]


def _operands(rest: list[str]) -> list[str]:
    """Non-option arguments before `--`."""
    out = []
    for a in rest:
        if a == "--":
            break
        if a != EXPR and a.startswith("-"):
            continue
        out.append(a)
    return out


def classify(args: list[str]) -> tuple[str, str] | None:
    """(verb, why) when this git invocation reads old trees/blobs, else None."""
    args = _strip_locators(args)
    if not args:
        return None
    verb, rest = args[0], args[1:]
    if verb in ("log", "whatchanged"):
        if "--" in rest:
            return verb, "a path walk over history (pathspec)"
        for a in rest:
            if a == EXPR:
                continue
            if a in LOG_FLAGS or a.startswith(("--diff-filter", "--stat=", "-S", "-G")):
                return verb, f"a diff over history (`{a}`)"
        return None
    if verb == "rev-parse":
        # `<rev>:<path>` resolves the path through <rev>'s trees.
        bad = [a for a in rest if ":" in a and not a.startswith("-")
               and not HEADISH.match(a)]
        return (verb, f"resolves `{bad[0]}` through old trees") if bad else None
    if verb == "blame":
        return verb, "blame reads every revision of the file"
    if verb == "cat-file":
        if "-p" in rest or "blob" in rest or "tree" in rest:
            return verb, "prints an object that may be an old tree or blob"
        return None
    ops = _operands(rest)
    if verb in ("ls-tree", "archive"):
        if ops and not HEADISH.match(ops[0]):
            return verb, f"reads the tree of `{ops[0]}`"
        return None
    if verb == "diff-tree":
        bad = [o for o in ops[:2] if not HEADISH.match(o)]
        return (verb, f"reads the tree of `{bad[0]}`") if bad else None
    if verb == "show":
        if "-s" in rest or "--no-patch" in rest:
            bad = [o for o in ops if ":" in o and not HEADISH.match(o)]
        else:
            bad = [o for o in ops if not HEADISH.match(o)] or ([] if ops else ["HEAD"])
            bad = [b for b in bad if not HEADISH.match(b)]
        return (verb, f"reads the tree/blob of `{bad[0]}`") if bad else None
    if verb == "diff":
        bad = [o for o in ops if not HEADISH.match(o)]
        return (verb, f"diffs against the tree of `{bad[0]}`") if bad else None
    if verb == "grep":
        # Which operand is the pattern and which a pathspec is not decidable
        # from text, so only a REVISION-SHAPED literal is taken as a tree read.
        bad = [o for o in ops if REVLIKE.match(o) and not HEADISH.match(o)]
        return (verb, f"searches the tree of `{bad[0]}`") if bad else None
    return None


# --- Python: through ast -----------------------------------------------------

WRAPPERS = {"git", "_git"}


def _lit(node) -> str:
    if isinstance(node, ast.Constant) and isinstance(node.value, str):
        return node.value
    if isinstance(node, ast.JoinedStr):
        parts = []
        for v in node.values:
            parts.append(v.value if isinstance(v, ast.Constant) else EXPR)
        s = "".join(parts)
        return s if s != EXPR else EXPR
    return EXPR


def python_calls(text: str) -> list[tuple[int, list[str]]]:
    """(line, git args) for every git invocation in a Python source."""
    try:
        with warnings.catch_warnings():
            warnings.simplefilter("ignore")
            tree = ast.parse(text)
    except SyntaxError:
        return []
    out = []
    for node in ast.walk(tree):
        if not isinstance(node, ast.Call):
            continue
        f = node.func
        name = f.id if isinstance(f, ast.Name) else (f.attr if isinstance(f, ast.Attribute) else "")
        if node.args and isinstance(node.args[0], (ast.List, ast.Tuple)):
            elts = node.args[0].elts
            if elts and _lit(elts[0]) == "git":
                out.append((node.lineno, [_lit(e) for e in elts[1:]]))
                continue
        if name in WRAPPERS and isinstance(f, ast.Name):
            args = [_lit(a) for a in node.args]
            # A wrapper's leading non-literal arguments are its cwd.
            while args and args[0] == EXPR:
                args.pop(0)
            out.append((node.lineno, args))
    return out


# --- shell: per line, command position only -----------------------------------

SH_GIT = re.compile(r"(?:^|\$\(|<\(|\||;|&&|\|\||\bthen\b|\bdo\b|\bif\b|!|\()\s*"
                    r"(?:[A-Z_][A-Z0-9_]*=\S*\s+)*git\s+(.*)")


def _sh_tokens(rest: str) -> list[str]:
    rest = re.split(r"\s(?:\||;|&&|\|\|)\s|\)\s*$|\)\s*\|", rest)[0]
    try:
        toks = shlex.split(rest, comments=True, posix=True)
    except ValueError:
        toks = rest.split()
    out = []
    for t in toks:
        if t == "\\" or re.match(r"^\d*[<>]", t):
            break  # a redirection or a line continuation ends the arguments
        if ")" in t and "$(" not in t:
            head = t.split(")")[0]  # the end of the `$(...)` this call is in
            if head:
                out.append(EXPR if "$" in head else head)
            break
        out.append(EXPR if "$" in t else t)
    return out


def shell_calls(text: str) -> list[tuple[int, list[str]]]:
    out = []
    for n, line in enumerate(text.splitlines(), 1):
        if line.lstrip().startswith("#"):
            continue
        for m in SH_GIT.finditer(line):
            out.append((n, _sh_tokens(m.group(1))))
    return out


def offenders(rel: str, text: str) -> list[tuple[int, str, str]]:
    calls = python_calls(text) if rel.endswith(".py") else shell_calls(text)
    bad = []
    for line, args in calls:
        hit = classify(args)
        if hit and (rel, hit[0]) not in EXEMPT:
            bad.append((line, hit[0], hit[1]))
    return bad


def tracked() -> list[str]:
    r = subprocess.run(["git", "-C", str(ROOT), "ls-files", "--", "scripts/*.py",
                        "scripts/*.sh", "just/*.just", "justfile"],
                       capture_output=True, text=True, check=True)
    return [p for p in r.stdout.splitlines() if p and p not in HELPERS]


def self_test() -> int:
    py = [
        ("path walk", 'git("log", "--diff-filter=DR", "--name-status", "--format=", "--", D)', True),
        ("list form diff of a base", 'subprocess.run(["git", "diff", "--name-only", base, "--"])', True),
        ("show rev:path", 'git("show", f"{rev}:x.toml")', True),
        ("ls-tree rev", 'git("ls-tree", rev, "--", p)', True),
        ("wrapper with cwd", 'git(root, "log", "-p", sha)', True),
        ("commit subject only", 'git(root, "log", "-1", "--format=%s", sha)', False),
        ("ls-tree HEAD", 'subprocess.run(["git", "-C", r, "ls-tree", "HEAD", "--", p])', False),
        ("worktree diff", '["git", "--no-pager", "diff", "--stat", "--", DOC]', False),
        ("rev-list", 'git(root, "rev-list", "--all", "--not", "HEAD")', False),
        ("rev-parse a path at a rev", 'git("rev-parse", "--verify", f"{base}:{LIST}")', True),
        ("rev-parse a commit", 'git("rev-parse", "--verify", "--quiet", f"{base}^{{commit}}")', False),
        ("prose names it", 'x = f"restore it (`git log -p -- {rel}`)"', False),
        ("helper call", 'history_git(REPO, "log", "--name-status", "--", D)', False),
    ]
    sh = [
        ("bare path walk", 'git log --name-status -- cmake/', True),
        ("in a substitution", 'n="$(git -C "$t" diff --name-only "$a" "$b" | wc -l)"', True),
        ("ls-tree a baseline", '    git ls-tree -r "$1" 2>/dev/null | awk x', True),
        ("cat-file -p", 'git cat-file -p "$sha"', True),
        ("grep a rev", 'git grep -n pat origin/main -- x', True),
        ("ls-tree HEAD", 'pin="$(git ls-tree HEAD "$path" | awk x)"', False),
        ("diff HEAD", 'git diff --name-only --no-renames HEAD -- 2>/dev/null', False),
        ("grep HEAD", 'git -C "$r" grep -h -o -E "p" HEAD', False),
        ("echo prose", 'echo "    git diff -- \'*Cargo.toml\'"', False),
        ("comment", '# git log -p -- x', False),
        ("log subject", 'git --git-dir="$s" log -1 --format=%s "$old"', False),
        ("end of a substitution", 'echo "$(git -C "$s" rev-parse --short HEAD): x"', False),
        ("helper call", 'nros_git_history_run 60 log --name-status -- x', False),
    ]
    bad = 0
    for kind, cases in (("x.py", py), ("x.sh", sh)):
        for name, text, want in cases:
            got = bool(offenders(kind, text))
            if got != want:
                print(f"check-history-object-reads self-test: {kind} {name}: "
                      f"flagged={got}, wanted {want}", file=sys.stderr)
                bad += 1
    if bad:
        return 1
    print(f"check-history-object-reads self-test: OK ({len(py) + len(sh)} case(s))",
          file=sys.stderr)
    return 0


def helper_self_tests() -> int:
    """The helper this gate sends callers to must itself hold its rule."""
    sys.path.insert(0, str(ROOT / "scripts"))
    from lib import git_history
    if git_history.self_test() != 0:
        return 1
    r = subprocess.run(["bash", "-c", ". scripts/lib/git-history.sh && "
                        "nros_git_history_self_test"], cwd=ROOT,
                       capture_output=True, text=True, stdin=subprocess.DEVNULL)
    if r.returncode != 0:
        print(r.stdout + r.stderr, file=sys.stderr)
        return 1
    return 0


def main() -> int:
    if self_test() != 0 or helper_self_tests() != 0:
        return 1
    if "--self-test" in sys.argv:
        return 0
    files = tracked()
    if not files:
        print("check-history-object-reads: no tracked files matched; the file "
              "list is wrong and this gate would pass vacuously.", file=sys.stderr)
        return 1
    found, bad, used = 0, [], set()
    for rel in files:
        try:
            text = (ROOT / rel).read_text(encoding="utf-8", errors="replace")
        except OSError:
            continue
        calls = python_calls(text) if rel.endswith(".py") else shell_calls(text)
        found += len(calls)
        for line, args in calls:
            hit = classify(args)
            if not hit:
                continue
            if (rel, hit[0]) in EXEMPT:
                used.add((rel, hit[0]))
                continue
            bad.append(f"  {rel}:{line}: git {hit[0]}: {hit[1]}")
    if found == 0:
        print("check-history-object-reads: found NO git invocations at all; the "
              "parser is wrong and this gate would pass vacuously.", file=sys.stderr)
        return 1
    stale = sorted(set(EXEMPT) - used)
    if stale:
        print("check-history-object-reads FAILED: EXEMPT entries match nothing "
              "(drop them):")
        for rel, verb in stale:
            print(f"  {rel}: {verb}")
        return 1
    if bad:
        print(f"check-history-object-reads FAILED: {len(bad)} raw read(s) of old "
              "trees or blobs outside the bounded helper.\n")
        print("\n".join(bad))
        print("""
CI's history is `--filter=tree:0 --unshallow`: commits whole, trees and blobs on
the network. A read like these fetches them lazily, one object per round trip,
and can run for longer than the lane's cap (issue 2442). Use the one spelling:

    python:  from lib.git_history import history_objects_unreadable, tree_present, history_git
    shell:   . scripts/lib/git-history.sh
             nros_git_history_objects_unreadable / nros_git_tree_present <rev>
             nros_git_history_run <timeout-s> <git args...>

and report an unreadable clone or a timeout as NOT VERIFIED. If the read is
provably safe (HEAD's tree, a commit object), add it to EXEMPT WITH the reason.
""")
        return 1
    print(f"check-history-object-reads: OK ({len(files)} file(s), {found} git "
          f"invocation(s), {len(EXEMPT)} exempt)")
    return 0


if __name__ == "__main__":
    sys.exit(main())

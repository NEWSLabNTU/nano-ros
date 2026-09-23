#!/usr/bin/env python3
"""`PR #1234 (abc1234)` in a roadmap doc must name a commit main can reach.

Phase-464's protocol says status lives in the wave's footer and that the
program runs on evidence over projection. A footer naming a commit is
making a claim of evidence, so a reader who runs `git show <sha>` has to
get the change the footer describes.

They did not. On 2026-09-23 ten references across six phase docs named
PRE-SQUASH BRANCH commits. The merge queue rebases a pull request onto
main, so the branch's own commit ids do not survive it, and every one of
those footers pointed at an object unreachable from main. `phase-459-W0`
and `W5` were the expensive case: both footers read "PR #1195
(d4c4dae3b), in the queue" for a day after #1195 had merged, so the
waves looked unstarted from the doc alone and a session rebuilt work
that was already on main.

WHAT IS CHECKED, and why it is exactly this and nothing wider:

Only the form `PR #<number> (<sha>...)`. That spelling is this repository
citing its own pull request, and it is used nowhere else: the docs cite
ros-launch-manifest and play_launch commits in prose instead, and the
archived phase docs use a free-form `commit \\`abc1234\\`` that names
history rewritten years of phases ago. Narrowing to this one form keeps
the gate HOST-INDEPENDENT, which a wider rule cannot be. A rule of "any
hex token must resolve" reads differently in a long-lived clone that
still holds deleted branch objects than in a fresh CI clone that does
not, and a gate whose verdict depends on the reader's reflog is not a
gate.

Under this rule an unresolvable sha is a FAILURE, not a skip: a commit
of ours that this repository cannot even name is the worst case of the
defect, not an exemption from it.

The convention that keeps a footer green while its work is in flight:
write `PR #1234, in the queue` with NO commit id, and add the id once it
lands. A commit id is a claim that something landed; do not write one
before it has.
"""

from __future__ import annotations

import re
import subprocess
import sys
from pathlib import Path

# The footers spell it both `PR #1234 (abc1234)` and
# `PR #1234 (abc1234, merged 2026-09-23)`, so the sha ends at a word
# boundary rather than at the closing parenthesis.
CITATION = re.compile(r"PR #(\d+) \(([0-9a-f]{7,40})\b")

ROOT_DIRS = ("docs/roadmap",)

HINT = """
The merge queue rebases a pull request onto main, so a branch's own
commit ids do not survive the merge. Name the commit that is ON MAIN:

    gh pr view <N> --json mergeCommit --jq .mergeCommit.oid
    git log main --oneline --grep '<the wave>'

While the work is still in flight, write `PR #1234, in the queue` with
NO commit id, and add the id once it lands. A commit id is a claim that
something landed; do not write one before it has.
"""


def git(root: Path, *args: str) -> subprocess.CompletedProcess:
    return subprocess.run(
        ["git", *args], cwd=root, capture_output=True, text=True
    )


def repo_root() -> Path:
    out = subprocess.run(
        ["git", "rev-parse", "--show-toplevel"],
        capture_output=True, text=True, check=True,
    )
    return Path(out.stdout.strip())


def classify(root: Path, sha: str) -> str:
    """One of: ok, unreachable, unknown."""
    if git(root, "cat-file", "-e", f"{sha}^{{commit}}").returncode != 0:
        return "unknown"
    if git(root, "merge-base", "--is-ancestor", sha, "HEAD").returncode == 0:
        return "ok"
    return "unreachable"


def subject(root: Path, sha: str) -> str:
    return git(root, "log", "-1", "--format=%s", sha).stdout.strip()


def selftest(root: Path) -> None:
    """Negative controls, run on the normal path.

    A negative control nobody runs decays into a comment, so this is
    called from main() rather than hidden behind a flag.
    """
    cases_match = [
        ("Status: landed in PR #1166 (7ef585b6b)", [("1166", "7ef585b6b")]),
        ("Status: landed in PR #1215 (14f120795, merged 2026-09-23); the ring",
         [("1215", "14f120795")]),
        ("PR #1 (abcdef01) and PR #22 (0123456789abcdef)",
         [("1", "abcdef01"), ("22", "0123456789abcdef")]),
    ]
    for line, want in cases_match:
        got = CITATION.findall(line)
        assert got == want, f"selftest: {line!r} gave {got}, wanted {want}"

    cases_no_match = [
        # The in-flight convention: a PR with no commit id claims nothing.
        "Status: PR #1195, in the queue",
        # Too short to be a commit id; git's own floor is 7.
        "PR #1166 (7ef585)",
        # A foreign commit is cited in prose, never in this form.
        "rlm R4 landed as bfbe075, tagged v0.1.37",
        # Not a hex run.
        "PR #1166 (see the merge queue)",
    ]
    for line in cases_no_match:
        got = CITATION.findall(line)
        assert got == [], f"selftest: {line!r} should not match, gave {got}"

    # HEAD is reachable from HEAD, by definition.
    assert classify(root, "HEAD") == "ok", "selftest: HEAD did not classify ok"

    # A well-formed id that cannot exist: all f's is not a commit here.
    assert classify(root, "f" * 40) == "unknown", (
        "selftest: an impossible sha did not classify unknown"
    )

    # The unreachable case needs a commit outside HEAD's history, which this
    # script must not create. Use one if the clone happens to hold it, and
    # stay quiet if it does not, so the selftest never depends on the clone.
    out = git(root, "rev-list", "--all", "--not", "HEAD", "--max-count=1")
    stray = out.stdout.strip()
    if stray:
        assert classify(root, stray) == "unreachable", (
            f"selftest: {stray} should classify unreachable"
        )

    print(
        "check-roadmap-commit-refs selftest: OK "
        f"({len(cases_match) + len(cases_no_match)} pattern case(s), "
        f"{3 if stray else 2} classify case(s))"
    )


def main() -> int:
    root = repo_root()
    selftest(root)

    files: list[Path] = []
    for d in ROOT_DIRS:
        p = root / d
        if p.is_dir():
            files.extend(sorted(p.rglob("*.md")))

    checked = 0
    bad: list[tuple[str, int, int, str, str]] = []
    verdicts: dict[str, str] = {}

    for path in files:
        try:
            text = path.read_text(encoding="utf-8")
        except UnicodeDecodeError:
            continue
        rel = str(path.relative_to(root))
        for lineno, line in enumerate(text.splitlines(), 1):
            for pr, sha in CITATION.findall(line):
                if sha not in verdicts:
                    verdicts[sha] = classify(root, sha)
                checked += 1
                if verdicts[sha] != "ok":
                    bad.append((rel, lineno, int(pr), sha, verdicts[sha]))

    if bad:
        print(
            "check-roadmap-commit-refs FAILED: "
            f"{len(bad)} of {checked} pull-request citation(s) name a commit "
            "HEAD cannot reach.\n"
        )
        for rel, lineno, pr, sha, why in bad:
            reason = (
                "not an ancestor of HEAD"
                if why == "unreachable"
                else "not a commit in this repository at all"
            )
            print(f"  {rel}:{lineno}: PR #{pr} ({sha}) -- {reason}")
            if why == "unreachable":
                print(f"      subject: {subject(root, sha) or '(unknown)'}")
        print(HINT)
        return 1

    print(
        f"check-roadmap-commit-refs: {checked} pull-request citation(s) "
        f"reachable from HEAD across {len(files)} roadmap doc(s)."
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())

#!/usr/bin/env python3
"""A `merge-base --is-ancestor` call must go through the truncation rule.

Issue 1476. `git merge-base --is-ancestor <a> <b>` answers NO for two quite
different reasons, and only one of them is about the commits:

  * nothing in <b>'s history is <a>                       — a measurement
  * <b>'s history is CUT here, so there is nothing to walk — not a measurement

A shallow clone produces the second. So does a shallow clone that has been
fetched into repeatedly, which is what a self-hosted runner's persistent
workspace is: `git clean -ffdx` removes files and never objects, so every
earlier run's tip stays resolvable while the path between them does not. On
2026-09-24 that made `check-roadmap-commit-refs` call three commits "not an
ancestor of HEAD" — all three on main — and a documentation gate stopped both
tier 2 and the tier-2 nightly before either built anything.

# Why a gate and not a code review

The rule was already written down, correctly, in
`check-play-launch-parser-ref.py`: "truncation can only manufacture a FALSE
negative, never a false positive". A second gate wrote it again a fortnight
later and got half of it — it guarded the object-is-absent negative and not
the history-is-cut one — and nothing could notice, because both spellings
read as careful code. That is the repo's recurring shape: a rule with two
spellings drifts toward the one that is wrong.

So there is ONE spelling, `scripts/lib/git_history.py` and its shell twin
`scripts/lib/git-history.sh`, and this gate keeps a third from appearing.

# What is checked

A tracked file that INVOKES `merge-base --is-ancestor` must reach the helper
(import `lib.git_history`, or source `scripts/lib/git-history.sh`), or be
listed in EXEMPT with a reason. Comment lines do not count as invocations —
several files discuss the hazard without running the command, and a gate that
cannot tell those apart would push the discussion out of the comments.

Deliberately NOT checked: that the caller then reports the UNKNOWN case
honestly. That is what the three-outcome verdict and the skip ledger are for,
and asserting it from a regex would either miss callers or forbid a legitimate
shape. Reaching the rule is what can be checked from a line.

Run:  python3 scripts/check/check-ancestry-truncation.py [--self-test]
"""

from __future__ import annotations

import re
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]

# The invocation, not the words. `git` and the subcommand may be separated by
# locator arguments (`-C <path>`, `--git-dir=<store>`), and a Python caller
# spells them as list elements, so the two tokens are matched with the noise
# between them rather than as a phrase.
CALL = re.compile(r"merge-base[^\n]{0,80}?--is-ancestor|--is-ancestor[^\n]{0,80}?merge-base")

# Reaching the one spelling of the rule. Either language, either direction.
REACHES = re.compile(
    r"lib\.git_history|lib/git_history|from lib import git_history"
    r"|scripts/lib/git-history\.sh|nros_git_ancestry|nros_git_history_truncated"
)

# A comment line in the languages that have call sites here. A file may
# discuss the hazard freely; what it may not do is run the command.
COMMENT = re.compile(r"^\s*(#|//|///|\*|/\*)")

SUFFIXES = (".py", ".sh", ".bash", ".just", ".rs", ".yml", ".yaml", ".cmake")

# path -> why it may name the call without reaching the helper.
EXEMPT = {
    "scripts/lib/git_history.py": "this IS the rule",
    "scripts/lib/git-history.sh": "this IS the rule, shell twin",
    "scripts/check/check-ancestry-truncation.py": "this gate, which quotes the call",
}


def tracked() -> list[str]:
    out = subprocess.run(
        ["git", "-C", str(ROOT), "ls-files"],
        capture_output=True, text=True, check=False,
    )
    return [p for p in out.stdout.splitlines() if p.endswith(SUFFIXES)]


def offenders(text: str) -> bool:
    """True when this text INVOKES the call without reaching the rule."""
    invokes = any(
        CALL.search(line) and not COMMENT.match(line)
        for line in text.splitlines()
    )
    return invokes and not REACHES.search(text)


def self_test() -> int:
    """Negative controls on the RULE, not on the tree.

    A gate whose only evidence is "the tree is clean" cannot tell a rule that
    holds from a predicate that never fires — which is how a green gate
    measured nothing for a fortnight elsewhere in this repo.
    """
    cases = [
        # (name, text, want_offender)
        ("bare shell call", 'git -C "$p" merge-base --is-ancestor "$a" "$b"', True),
        ("bare python call",
         '["git", "merge-base", "--is-ancestor", ref, tip]', True),
        ("shell call that sources the rule",
         '. scripts/lib/git-history.sh\nnros_git_ancestry "$a" "$b" -C "$p"', False),
        ("python call that imports the rule",
         'from lib.git_history import ancestry\nancestry(root, sha)', False),
        # The hazard is discussed in several files that do not run it; a gate
        # that read those would push the discussion out of the comments.
        ("comment only", '# `git merge-base --is-ancestor` cannot see past a graft', False),
        ("rustdoc comment only",
         '/// (`git merge-base --is-ancestor <pin> <branch>`) fails with', False),
        # A file that reaches the rule AND still has a raw call is accepted:
        # this gate checks reach, not every line (see the module docstring).
        ("mixed", 'from lib.git_history import ancestry\n'
                  'subprocess.run(["git", "merge-base", "--is-ancestor", a, b])', False),
    ]
    bad = 0
    for name, text, want in cases:
        got = offenders(text)
        if got != want:
            print(f"check-ancestry-truncation self-test: {name}: got {got}, "
                  f"wanted {want}", file=sys.stderr)
            bad += 1
    if bad:
        return 1
    print(f"check-ancestry-truncation self-test: OK ({len(cases)} case(s))",
          file=sys.stderr)
    return 0


def main() -> int:
    if "--self-test" in sys.argv:
        return self_test()
    if self_test() != 0:
        return 1

    files = tracked()
    if not files:
        print("check-ancestry-truncation: no tracked files matched — the file "
              "list is wrong and this gate would pass vacuously.", file=sys.stderr)
        return 1

    bad = []
    seen_any_call = False
    for rel in files:
        try:
            text = (ROOT / rel).read_text(encoding="utf-8", errors="replace")
        except OSError:
            continue
        if not CALL.search(text):
            continue
        seen_any_call = True
        if rel in EXEMPT:
            continue
        if offenders(text):
            bad.append(rel)

    if not seen_any_call:
        print("check-ancestry-truncation: found NO ancestry call sites at all — "
              "the pattern is wrong, and this gate would pass vacuously.",
              file=sys.stderr)
        return 1

    if bad:
        print("check-ancestry-truncation FAILED: "
              f"{len(bad)} file(s) run `git merge-base --is-ancestor` without "
              "the truncation rule.\n")
        for rel in bad:
            print(f"  {rel}")
        print("""
A shallow clone grafts its tip parentless, so `--is-ancestor` answers NO for a
pair the full history relates. Truncation can only manufacture a FALSE
negative, so a YES counts everywhere and a NO from a truncated checkout is
"cannot tell" — a third outcome the caller must report, never read as a
verdict. Issue 1476 is what happened the last time a caller read it as one:
three commits on main were reported unreachable and tier 2 built nothing.

Use the one spelling:

    python:  sys.path.insert(0, "<scripts>"); from lib.git_history import ancestry
             ancestry(cwd, sha, tip)            -> True / False / None
    shell:   . scripts/lib/git-history.sh
             nros_git_ancestry <a> <b> [-C <p>] -> yes / no / unknown

If a site genuinely cannot, add it to EXEMPT in this file WITH the reason.
""")
        return 1

    print(f"check-ancestry-truncation: OK ({len(files)} tracked file(s); every "
          f"ancestry call site reaches the rule, {len(EXEMPT)} exempt)")
    return 0


if __name__ == "__main__":
    sys.exit(main())

#!/usr/bin/env python3
"""Generate the OPEN-issues list in `docs/issues/README.md` — phase 395 W1.

The list is derivable: every issue file already carries `id`, `title`, `type`,
`area` and `status` in its frontmatter. Maintaining a second hand-written copy
buys nothing and costs a great deal, because that copy is a SHARED REGISTRY that
every agent touches.

Why this is a batching blocker and not a tidy-up
------------------------------------------------
A merge queue tests a batch only if the batch MERGES. `docs/issues/README.md` is
4,170 lines with 31 open rows scattered from line 68 to line 4,154, interleaved
with 291 "Recently resolved" entries — so two agents filing unrelated issues
collide in the same dense region and the batch cannot form at all. That is not a
prediction: this session hit it on a rebase, and separately found `main` carrying
**two open `#0824` rows** while `check-issue-index` reported OK (it compares
distinct ids to files, so a duplicate is invisible to it).

Why the generated rows are one line
-----------------------------------
The hand-written rows grew into paragraphs — a second copy of each issue's own
prose. An index of 31 open issues does not need 4,000 lines; the detail belongs
in the issue file, which already has it. So a row is `**#NNNN** (area, opened) —
title. See NNNN-*.` and nothing more.

`Recently resolved` is NOT generated. It is genuinely hand-written history and
the convention says it is pruned per cycle, which is a judgement.

Usage
-----
    scripts/gen-issue-index.py           # rewrite the block
    scripts/gen-issue-index.py --check   # fail if it would change
"""

import argparse
import os
import re
import subprocess
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
INDEX = os.path.join(ROOT, "docs", "issues", "open.md")

BEGIN = "<!-- BEGIN GENERATED open-issue list — scripts/gen-issue-index.py -->"
END = "<!-- END GENERATED open-issue list -->"

FM = re.compile(r"^---\n(.*?)\n---", re.S)


def field(fm, name):
    """A frontmatter scalar, tolerating the folded multi-line form YAML allows."""
    m = re.search(rf"^{name}:\s*(.*?)(?=^\S+:|\Z)", fm, re.M | re.S)
    if not m:
        return ""
    val = " ".join(line.strip() for line in m.group(1).strip().split("\n"))
    return val.strip().strip('"').strip("'")


def open_issues():
    # `--others --exclude-standard` as well as the cached set: a just-filed
    # issue is UNTRACKED until it is staged, and that is precisely the moment
    # this runs. Listing only tracked files made the generated index stale at
    # the one moment it is regenerated, so `check-issue-index` failed on the
    # new issue while the generator reported it had written the list.
    out = subprocess.run(
        ["git", "ls-files", "--cached", "--others", "--exclude-standard",
         ISSUE_PATHSPEC],
        cwd=ROOT, capture_output=True, text=True, check=True,
    ).stdout.split()
    out = sorted(set(out))
    rows = []
    for rel in sorted(out):
        with open(os.path.join(ROOT, rel), encoding="utf8") as fh:
            text = fh.read()
        m = FM.match(text)
        if not m:
            continue
        fm = m.group(1)
        if field(fm, "status") != "open":
            continue
        num = re.match(r"\d+", os.path.basename(rel)).group(0)
        rows.append(
            {
                "id": num,
                "title": field(fm, "title"),
                "area": field(fm, "area") or "—",
                "type": field(fm, "type"),
            }
        )
    return rows


def render(rows):
    # issue 0884 — NO COUNT LINE, deliberately. A running total is one line that
    # EVERY issue-touching pull request rewrites at the same position, which is
    # the worst possible shape for concurrent edits: differing counts make git
    # keep both lines under `merge=union`, and matching counts merge silently to
    # a WRONG total (measured: two agents each taking 3 -> 4 produced "4" for
    # five issues). The number is derivable by counting the rows below it and
    # buys a reader nothing the list does not already show.
    lines = [
        BEGIN,
        "",
        "One line each — the detail lives in the issue file, which already has",
        "it. Regenerate with `scripts/gen-issue-index.py`; `check-issue-index`",
        "fails if this block drifts.",
        "",
    ]
    for r in rows:
        lines.append(f"- **#{r['id']}** ({r['area']}) — {r['title']} See `{r['id']}-*`.")
    lines += ["", END]
    return "\n".join(lines)


# The pathspec BOTH this generator and `scripts/check-issue-index.sh` enumerate
# with. It must stay in step with the copy there — `--self-test` asserts that.
#
# `[0-9]*`, NOT `0*`. The old spelling assumed every id begins with a zero, which
# is true only below 1000. At id 1000 the glob silently stops matching: the
# generator drops the issue from the list, the checker cannot see the file
# either, so the two agree about a set that is missing it and the gate passes
# GREEN. A ceiling that reports success is worse than one that errors — nothing
# would have pointed at this file.
ISSUE_PATHSPEC = "docs/issues/[0-9]*.md"


def self_test():
    """Prove the pathspec matches ids at and beyond the old 1000 ceiling.

    Runs `git ls-files` against a throwaway repo rather than asserting on a
    regex: what broke was GIT PATHSPEC matching, so testing anything else would
    pass while the real enumeration still dropped files.
    """
    import shutil, tempfile

    names = ["0001-a.md", "0999-b.md", "1000-c.md", "9999-d.md", "10000-e.md",
             "README.md", "open.md"]
    want = {"0001-a.md", "0999-b.md", "1000-c.md", "9999-d.md", "10000-e.md"}
    tmp = tempfile.mkdtemp()
    try:
        d = os.path.join(tmp, "docs", "issues")
        os.makedirs(d)
        for n in names:
            with open(os.path.join(d, n), "w", encoding="utf8") as fh:
                fh.write("---\nid: 1\nstatus: open\n---\n")
        subprocess.run(["git", "init", "-q"], cwd=tmp, check=True)
        out = subprocess.run(
            ["git", "ls-files", "--cached", "--others", "--exclude-standard",
             ISSUE_PATHSPEC],
            cwd=tmp, capture_output=True, text=True, check=True,
        ).stdout.split()
        got = {os.path.basename(x) for x in out}
        if got != want:
            print("gen-issue-index --self-test FAILED")
            print(f"  pathspec {ISSUE_PATHSPEC!r}")
            print(f"  missed:  {sorted(want - got)}")
            print(f"  extra:   {sorted(got - want)}")
            return 1

        # The id must be read from the WHOLE leading run of digits. A fixed
        # 4-char slice returns '1000' for '10000-e.md' — a different issue.
        for n in names:
            if n in want:
                m = re.match(r"\d+", n)
                assert m and n.startswith(m.group(0) + "-"), n
        if re.match(r"\d+", "10000-e.md").group(0) != "10000":
            print("gen-issue-index --self-test FAILED: id truncated")
            return 1

        # The shell checker must enumerate the SAME set, or the two disagree
        # about which files exist and the gate compares mismatched sets.
        sh = os.path.join(ROOT, "scripts", "check-issue-index.sh")
        if os.path.exists(sh):
            with open(sh, encoding="utf8") as fh:
                sh_text = fh.read()
            if f"'{ISSUE_PATHSPEC}'" not in sh_text:
                print("gen-issue-index --self-test FAILED: "
                      f"check-issue-index.sh does not enumerate {ISSUE_PATHSPEC!r}")
                return 1
        print("gen-issue-index self-test: OK "
              "(pathspec matches ids >= 1000; checker agrees)")
        return 0
    finally:
        shutil.rmtree(tmp, ignore_errors=True)


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--check", action="store_true")
    ap.add_argument("--self-test", action="store_true")
    args = ap.parse_args()

    if args.self_test:
        return self_test()

    rows = open_issues()
    block = render(rows)

    with open(INDEX, encoding="utf8") as fh:
        text = fh.read()

    if BEGIN in text and END in text:
        start = text.index(BEGIN)
        stop = text.index(END) + len(END)
        new = text[:start] + block + text[stop:]
    else:
        if args.check:
            print(
                "gen-issue-index: no generated block in docs/issues/open.md.\n"
                "Run scripts/gen-issue-index.py to create it."
            )
            return 1
        print("gen-issue-index: no block found; printing it for placement:\n")
        print(block)
        return 0

    if args.check:
        if new != text:
            print(
                "gen-issue-index: the open-issue list is STALE.\n"
                "  Run: python3 scripts/gen-issue-index.py"
            )
            return 1
        print(f"gen-issue-index OK — {len(rows)} open issue(s), list matches the files.")
        return 0

    with open(INDEX, "w", encoding="utf8") as fh:
        fh.write(new)
    print(f"wrote {INDEX} — {len(rows)} open issue(s)")
    return 0


if __name__ == "__main__":
    sys.exit(main())

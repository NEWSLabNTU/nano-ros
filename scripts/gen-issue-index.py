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
         "docs/issues/0*.md"],
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
        num = os.path.basename(rel)[:4]
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


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--check", action="store_true")
    args = ap.parse_args()

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

#!/usr/bin/env python3
"""Run the codebase audit's detection nets, and show a reader only what changed.

The nets live in ONE place, `docs/development/codebase-audit-checklist.md`:
each is a line holding a single backticked `git grep ...` command under a
checklist item (`- **G2 ...**`). This script extracts them, so adding a net to
the checklist is all it takes; it never carries a copy.

    python3 scripts/audit-nets.py                       # every net's count
    python3 scripts/audit-nets.py --since <sha>         # + hits in changed files
    python3 scripts/audit-nets.py --since <sha> --hits  # print those hit lines
    python3 scripts/audit-nets.py --only G2,C8          # restrict to items
    python3 scripts/audit-nets.py --self-test
    python3 scripts/audit-nets.py --check               # the gate: every net runs

`--since` takes the `Baseline commit:` of the previous findings log. A hit
counts as CHANGED when its file differs between that commit and the worktree
(committed or not), which is the only part of a net worth a reader's tokens;
the totals are printed so a count that moved with no changed file is visible
too. Only commands starting `git grep ` are run: the checklist is prose, and
this keeps a stray backticked span from becoming a shell command.
"""

import argparse
import os
import re
import shlex
import subprocess
import sys

CHECKLIST = "docs/development/codebase-audit-checklist.md"
ITEM = re.compile(r"^- \*\*([A-K][0-9]+)\b")
NET = re.compile(r"^\s+`(git grep [^`]+)`")


def extract(text):
    """`[(item, command)]` in file order; a net belongs to the nearest item
    heading above it."""
    item = None
    nets = []
    for line in text.splitlines():
        m = ITEM.match(line)
        if m:
            item = m.group(1)
            continue
        m = NET.match(line)
        if m and item:
            nets.append((item, m.group(1)))
    return nets


def run_net(cmd, root):
    """Hit lines `path:line:text` of one net (git grep exits 1 on no match)."""
    proc = subprocess.run(
        shlex.split(cmd), cwd=root, capture_output=True, text=True, check=False
    )
    if proc.returncode not in (0, 1):
        raise RuntimeError(f"net failed (rc={proc.returncode}): {cmd}\n{proc.stderr}")
    return [line for line in proc.stdout.splitlines() if line]


def changed_files(since, root):
    """Paths that differ between `since` and the worktree, plus untracked."""
    diff = subprocess.run(
        ["git", "diff", "--name-only", since],
        cwd=root,
        capture_output=True,
        text=True,
        check=True,
    ).stdout.split()
    untracked = subprocess.run(
        ["git", "ls-files", "--others", "--exclude-standard"],
        cwd=root,
        capture_output=True,
        text=True,
        check=True,
    ).stdout.split()
    return set(diff) | set(untracked)


def self_test():
    """The extractor binds nets to items and refuses non-`git grep` spans."""
    sample = (
        "- **A2 thing.** Detect:\n"
        "  `git grep -n 'x' -- cmake`\n"
        "  `rm -rf /` is prose, not a net\n"
        "- **G2 other.**\n"
        "  text `git grep inline` mid-line is not a net\n"
        "  `git grep -nE 'a|b' -- examples`\n"
    )
    got = extract(sample)
    want = [
        ("A2", "git grep -n 'x' -- cmake"),
        ("G2", "git grep -nE 'a|b' -- examples"),
    ]
    if got != want:
        raise SystemExit(f"audit-nets self-test: extracted {got!r}, expected {want!r}")
    root = repo_root()
    with open(os.path.join(root, CHECKLIST), encoding="utf-8") as f:
        real = extract(f.read())
    if not real:
        raise SystemExit(f"audit-nets self-test: no nets found in {CHECKLIST}")
    print(f"audit-nets self-test: OK ({len(real)} nets in the checklist)")


def repo_root():
    return subprocess.run(
        ["git", "rev-parse", "--show-toplevel"],
        capture_output=True,
        text=True,
        check=True,
    ).stdout.strip()


def main():
    ap = argparse.ArgumentParser(description="Run the codebase audit's detection nets.")
    ap.add_argument("--since", help="baseline commit (the last findings log's)")
    ap.add_argument("--hits", action="store_true", help="print changed-file hit lines")
    ap.add_argument("--only", help="comma-separated item ids, e.g. G2,C8")
    ap.add_argument("--self-test", action="store_true")
    ap.add_argument("--check", action="store_true", help="gate: every net runs")
    args = ap.parse_args()
    # The extractor's negative control runs on every invocation, so no mode
    # ships it unexercised.
    self_test()
    if args.self_test:
        return 0
    if args.check:
        # The gate: every net in the checklist parses and runs. A net with a
        # typo would otherwise read as "0 hits" — the vacuous-pass shape.
        root = repo_root()
        with open(os.path.join(root, CHECKLIST), encoding="utf-8") as f:
            nets = extract(f.read())
        for _item, cmd in nets:
            run_net(cmd, root)
        print(f"audit-nets: OK ({len(nets)} nets run)")
        return 0

    root = repo_root()
    with open(os.path.join(root, CHECKLIST), encoding="utf-8") as f:
        nets = extract(f.read())
    if args.only:
        keep = {i.strip() for i in args.only.split(",")}
        nets = [n for n in nets if n[0] in keep]
    changed = changed_files(args.since, root) if args.since else None

    print(f"{'item':5} {'total':>6} {'changed':>8}  net")
    for item, cmd in nets:
        hits = run_net(cmd, root)
        fresh = [
            h for h in hits if changed is not None and h.split(":", 1)[0] in changed
        ]
        col = f"{len(fresh):>8}" if changed is not None else f"{'-':>8}"
        print(f"{item:5} {len(hits):>6} {col}  {cmd}")
        if args.hits:
            for h in fresh:
                print(f"      {h}")
    if changed is not None:
        print(f"\n{len(changed)} file(s) changed since {args.since}")
    return 0


if __name__ == "__main__":
    sys.exit(main())

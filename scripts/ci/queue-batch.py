#!/usr/bin/env python3
"""Which pull requests were in the merge group a run built? Phase-567 W5b.

`queue.yml`'s L3 (cross build + link) is advisory: it carries no required
context, so a red L3 ejects nothing and the ejection notifier rightly posts
nothing (issue 1843). That left a link break heard about only at the next
nightly. W5b reports it to every pull request whose change the red build
contained, which needs this question answered.

A merge group's ref is `gh-readonly-queue/main/pr-<N>-<base>`, where `<base>` is
the commit the group was built ON. When the queue speculates several entries
deep, entry k's base is entry k-1's group head, so the group holds every entry
down that chain. The chain stops at a base that is no other entry's head (the
plain `main` tip), or at an entry whose pull request had already MERGED when
this run started: its change was `main` by then, not part of this batch.

Heads are read from `gate`'s merge-group runs, because `gate` runs for every
entry; `queue`'s L3 may not have started for all of them.

    queue-batch.py --run <run-id>       # one PR number per line, this run's first
    queue-batch.py --selftest

Never fails the caller: a run it cannot read prints nothing and exits 0, and the
caller then posts to the run's own pull request alone.
"""

import argparse
import json
import os
import re
import subprocess
import sys

REPO = os.environ.get("NROS_QUEUE_REPO", "NEWSLabNTU/nano-ros")
REF = re.compile(r"/pr-(\d+)-([0-9a-f]{40})$")
# How many recent merge-group runs to read heads from. The queue batches at most
# five, so a deep chain is far shorter than this.
HEAD_WINDOW = 60
MAX_DEPTH = 10


def gh_json(args):
    try:
        out = subprocess.run(["gh"] + args, capture_output=True, text=True, timeout=120)
    except (OSError, subprocess.TimeoutExpired):
        return None
    if out.returncode != 0:
        return None
    try:
        return json.loads(out.stdout)
    except json.JSONDecodeError:
        return None


def parse_ref(head_branch):
    """(pr, base sha) from a merge-group ref, or None."""
    m = REF.search(head_branch or "")
    return (int(m.group(1)), m.group(2)) if m else None


def batch(head_branch, started_at, group_runs, merged_at):
    """The PRs in the group `head_branch` names, its own first.

    `group_runs`: merge-group runs as {headBranch, headSha}; `merged_at(pr)`:
    the PR's merge time (ISO string) or None. An entry merged BEFORE
    `started_at` is `main`, not batch, and ends the chain.
    """
    me = parse_ref(head_branch)
    if not me:
        return []
    by_head = {}
    for r in group_runs:
        ref = parse_ref(r.get("headBranch"))
        if ref and r.get("headSha"):
            by_head.setdefault(r["headSha"], ref)
    out, (pr, base) = [me[0]], me
    for _ in range(MAX_DEPTH):
        prev = by_head.get(base)
        if not prev or prev[0] in out:
            break
        m = merged_at(prev[0])
        if m and m < started_at:
            break
        out.append(prev[0])
        base = prev[1]
    return out


def selftest(verbose=False):
    bad = 0

    def case(desc, ok):
        nonlocal bad
        if verbose or not ok:
            print(f"  {'ok  ' if ok else 'FAIL'} {desc}", file=sys.stdout if ok else sys.stderr)
        bad += not ok

    s = lambda c: c * 40
    # Recorded shape (2026-11-16): each entry built on the previous one's head.
    runs = [
        {"headBranch": f"gh-readonly-queue/main/pr-4047-{s('c')}", "headSha": s("d")},
        {"headBranch": f"gh-readonly-queue/main/pr-4045-{s('b')}", "headSha": s("c")},
        {"headBranch": f"gh-readonly-queue/main/pr-4044-{s('a')}", "headSha": s("b")},
        {"headBranch": f"gh-readonly-queue/main/pr-4042-{s('9')}", "headSha": s("a")},
    ]
    merged = {4042: "2026-11-16T09:45:00Z"}
    got = batch(runs[0]["headBranch"], "2026-11-16T10:09:00Z", runs, merged.get)
    case("the chain of unmerged entries is the batch, this run's PR first",
         got == [4047, 4045, 4044])
    case("an entry merged before the run started is main, and ends the chain",
         4042 not in got)
    case("an entry merged AFTER the run started was still in the batch",
         batch(runs[0]["headBranch"], "2026-11-16T09:30:00Z", runs, merged.get)
         == [4047, 4045, 4044, 4042])
    case("a group built on plain main is its own PR alone",
         batch(f"gh-readonly-queue/main/pr-7-{s('e')}", "x", runs, merged.get) == [7])
    case("a ref that is no merge group names nobody", batch("main", "x", runs, merged.get) == [])
    loop = [{"headBranch": f"gh-readonly-queue/main/pr-1-{s('f')}", "headSha": s("f")}]
    case("a self-referencing ref cannot loop",
         batch(loop[0]["headBranch"], "x", loop, lambda _p: None) == [1])
    if bad:
        print("queue-batch self-test: FAILED", file=sys.stderr)
    return 1 if bad else 0


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--run", help="the merge-group run whose batch to print")
    ap.add_argument("--selftest", action="store_true")
    args = ap.parse_args()
    if args.selftest:
        return selftest(verbose=True)
    if selftest():
        return 1
    if not args.run:
        ap.error("--run <id> is required")
    run = gh_json(["run", "view", args.run, "--repo", REPO,
                   "--json", "headBranch,createdAt"]) or {}
    if not parse_ref(run.get("headBranch")):
        return 0
    group_runs = gh_json(["run", "list", "--repo", REPO, "--workflow", "gate.yml",
                          "--event", "merge_group", "--limit", str(HEAD_WINDOW),
                          "--json", "headBranch,headSha"]) or []
    cache = {}

    def merged_at(pr):
        if pr not in cache:
            cache[pr] = (gh_json(["pr", "view", str(pr), "--repo", REPO,
                                  "--json", "mergedAt"]) or {}).get("mergedAt")
        return cache[pr]

    for pr in batch(run["headBranch"], run.get("createdAt", ""), group_runs, merged_at):
        print(pr)
    return 0


if __name__ == "__main__":
    sys.exit(main())

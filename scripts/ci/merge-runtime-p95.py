#!/usr/bin/env python3
"""Per-shard runtime of the dispatch-only `merge-runtime.yml` measurement tool.

The number a per-merge cadence for the host witness boards would be decided on
(phase-535 W8, RFC-0122 D9: p95 at most 20 minutes per shard). Since the
2026-11-12 ruling those boards are `daily` and the workflow runs only on
`workflow_dispatch`; this reads its job durations from the GitHub API (`gh`),
so a re-measurement is a number, not a recollection.

What is measured: PER-JOB RUNTIME, each shard job's `started_at` to its
`completed_at`. It excludes the time a job queued for a runner, and it is not
batch latency.

Only a SUCCESSFUL job is a sample: a job that failed stopped early, and a p95
over failures would report how fast the lane breaks. Failed jobs are counted
(n_fail) and so are jobs with no verdict (cancelled, skipped), never as samples.
A job that hit its `timeout-minutes` (conclusion `timed_out`) is counted apart
(n_timed_out) and makes its shard OVER budget: it is the most over-budget a
shard can be, never "no verdict". A shard with no successful job prints
"no samples".

Usage::

    merge-runtime-p95.py [--days 7] [--repo OWNER/NAME] [--budget-min 20] [--event E]
    merge-runtime-p95.py --selftest

Exit status: 0 when every shard has samples and its p95 is within the budget,
1 otherwise (including "no samples"), 2 on a usage or API error.
"""

import argparse
import datetime as dt
import json
import math
import re
import subprocess
import sys

WORKFLOW = "merge-runtime.yml"
# phase-559 W5b: the job name carries the dispatched lane (`merge-runtime
# tier1 (linux)`); the old `merge-runtime (linux)` is the merge lane. Only
# merge-lane jobs are samples here: a tier-1 shard is a different measurement.
JOB_NAME = re.compile(r"^merge-runtime (?:(?P<lane>[a-z0-9_-]+) )?\((?P<board>[^)]+)\)$")


def percentile(values, q):
    """Nearest-rank percentile (q in 0..100) of a non-empty list."""
    s = sorted(values)
    rank = max(1, math.ceil(q / 100 * len(s)))
    return s[rank - 1]


def _ts(s):
    return dt.datetime.fromisoformat(s.replace("Z", "+00:00"))


def summarise(jobs):
    """jobs: [{name, conclusion, started_at, completed_at}] -> {board: stats}."""
    out = {}
    for j in jobs:
        m = JOB_NAME.match(j.get("name") or "")
        if not m or (m["lane"] or "merge") != "merge":
            continue
        b = out.setdefault(m["board"], {"minutes": [], "failed": 0, "timed_out": 0, "no_verdict": 0})
        if j.get("conclusion") == "timed_out":
            b["timed_out"] += 1
            continue
        if j.get("conclusion") == "failure":
            b["failed"] += 1
            continue
        if j.get("conclusion") != "success":
            b["no_verdict"] += 1
            continue
        if not j.get("started_at") or not j.get("completed_at"):
            b["no_verdict"] += 1
            continue
        mins = (_ts(j["completed_at"]) - _ts(j["started_at"])).total_seconds() / 60
        b["minutes"].append(mins)
    return out


def _gh(args):
    r = subprocess.run(["gh", "api", *args], capture_output=True, text=True)
    if r.returncode != 0:
        print(f"gh api {' '.join(args)} failed:\n{r.stderr}", file=sys.stderr)
        sys.exit(2)
    return json.loads(r.stdout)


def fetch(repo, days, event):
    since = (dt.datetime.now(dt.timezone.utc) - dt.timedelta(days=days)).date()
    ids, page = [], 1
    while True:
        data = _gh([
            f"repos/{repo}/actions/workflows/{WORKFLOW}/runs?event={event}"
            f"&created=%3E%3D{since.isoformat()}&per_page=100&page={page}"
        ])
        batch = [r["id"] for r in data.get("workflow_runs", [])]
        ids += batch
        if len(batch) < 100:
            break
        page += 1
    jobs = []
    for rid in ids:
        jobs += _gh([f"repos/{repo}/actions/runs/{rid}/jobs?per_page=100"]).get("jobs", [])
    return len(ids), jobs


def report(stats, budget):
    ok = bool(stats)
    for board in sorted(stats):
        m = stats[board]["minutes"]
        nf = stats[board]["failed"]
        nt = stats[board]["timed_out"]
        nv = stats[board]["no_verdict"]
        counts = f"n_ok={len(m)} n_fail={nf} n_timed_out={nt} n_no_verdict={nv}"
        if nt:
            print(f"{board:16} {counts}  OVER {budget:g} min (a job timed out)")
            ok = False
            continue
        if not m:
            print(f"{board:16} {counts}  no samples")
            ok = False
            continue
        p50, p95 = percentile(m, 50), percentile(m, 95)
        verdict = "within" if p95 <= budget else "OVER"
        ok = ok and p95 <= budget
        print(f"{board:16} {counts}  p50={p50:6.1f} min  p95={p95:6.1f} min  "
              f"({verdict} {budget:g} min)")
    return ok


def selftest():
    assert percentile([1, 2, 3, 4], 50) == 2
    assert percentile([1, 2, 3, 4], 95) == 4
    assert percentile([5], 95) == 5
    jobs = [
        {"name": "merge-runtime (linux)", "conclusion": "success",
         "started_at": "2026-11-12T00:00:00Z", "completed_at": "2026-11-12T00:10:00Z"},
        {"name": "merge-runtime (linux)", "conclusion": "cancelled",
         "started_at": "2026-11-12T00:00:00Z", "completed_at": "2026-11-12T00:01:00Z"},
        {"name": "merge-runtime (zephyr)", "conclusion": "failure",
         "started_at": "2026-11-12T00:00:00Z", "completed_at": "2026-11-12T00:30:00Z"},
        {"name": "coverage report", "conclusion": "success",
         "started_at": "2026-11-12T00:00:00Z", "completed_at": "2026-11-12T00:01:00Z"},
    ]
    s = summarise(jobs)
    assert s["linux"] == {"minutes": [10.0], "failed": 0, "timed_out": 0, "no_verdict": 1}, s
    # A failed job is counted, never a sample.
    assert s["zephyr"] == {"minutes": [], "failed": 1, "timed_out": 0, "no_verdict": 0}, s
    assert "coverage report" not in s
    # A shard whose only job failed has no samples, so the report is not OK.
    assert report(s, 20) is False
    # Negative control: a 30-minute SUCCESS is over a 20-minute budget.
    slow = summarise([{"name": "merge-runtime (zephyr)", "conclusion": "success",
                       "started_at": "2026-11-12T00:00:00Z",
                       "completed_at": "2026-11-12T00:30:00Z"}])
    assert slow["zephyr"]["minutes"] == [30.0], slow
    assert report(slow, 20) is False
    assert report({"linux": s["linux"]}, 20) is True
    assert report({}, 20) is False
    # A timed-out job is counted apart, never no-verdict, and fails the
    # budget even beside fast successes.
    to = summarise([
        {"name": "merge-runtime (linux)", "conclusion": "success",
         "started_at": "2026-11-12T00:00:00Z", "completed_at": "2026-11-12T00:05:00Z"},
        {"name": "merge-runtime (linux)", "conclusion": "timed_out",
         "started_at": "2026-11-12T00:00:00Z", "completed_at": "2026-11-12T02:30:00Z"},
    ])
    assert to["linux"] == {"minutes": [5.0], "failed": 0, "timed_out": 1,
                           "no_verdict": 0}, to
    assert report(to, 20) is False
    lanes = summarise([
        {"name": "merge-runtime merge (linux)", "conclusion": "success",
         "started_at": "2026-11-12T00:00:00Z", "completed_at": "2026-11-12T00:10:00Z"},
        {"name": "merge-runtime tier1 (linux)", "conclusion": "success",
         "started_at": "2026-11-12T00:00:00Z", "completed_at": "2026-11-12T03:00:00Z"},
    ])
    assert lanes["linux"]["minutes"] == [10.0], lanes  # the tier1 shard is not a sample
    print("merge-runtime-p95 selftest: OK")


def main():
    ap = argparse.ArgumentParser(
        description="Per-shard runtime of the merge-runtime measurement workflow.")
    ap.add_argument("--days", type=int, default=7)
    ap.add_argument("--repo", default="NEWSLabNTU/nano-ros")
    ap.add_argument("--budget-min", type=float, default=20.0)
    ap.add_argument("--event", default="workflow_dispatch",
                    help="the run event to read (the workflow is dispatch-only)")
    ap.add_argument("--selftest", action="store_true")
    a = ap.parse_args()
    if a.selftest:
        selftest()
        return 0
    n, jobs = fetch(a.repo, a.days, a.event)
    print(f"{WORKFLOW}: {n} {a.event} run(s) in the last {a.days} day(s)")
    return 0 if report(summarise(jobs), a.budget_min) else 1


if __name__ == "__main__":
    sys.exit(main())

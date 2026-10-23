#!/usr/bin/env python3
"""Why is this self-hosted job still queued? -- issue 1365, clause 2.

A job that never starts reports nothing. Every lane on the self-hosted
runners has stalled in one of three ways, and from job state alone they look
the same: a job `queued` for six hours.

  * NO RUNNER   -- nothing registered carries the job's labels (2026-09-19:
                   the runner was gone and three jobs waited 24 h, then
                   cancelled, and the lane read GREEN).
  * BUSY        -- every matching runner is online and running another job.
                   That is contention, and it ends by itself (2026-09-21,
                   2026-10-19/20).
  * IDLE        -- a matching runner is online, NOT busy, and the job still
                   waits. This was the "third shape": 2026-09-29 to 10-17,
                   up to 26 h. It matched a runner on 2.329.0 that GitHub
                   offered work only after a self-update. Any recurrence is
                   the runner side's fault and needs an operator.

OFFLINE (registered, matching, every one offline) is NO RUNNER's twin, kept
apart because the remedy differs: restart the container instead of
registering a runner.

The runner list needs repository ADMIN (`GET /repos/{r}/actions/runners`).
That is why this is a maintainer tool and not a workflow step: a workflow's
`GITHUB_TOKEN` has no `administration` permission, so a workflow could only
say "queued" and could never name the shape. Without the permission the jobs
are still listed with how long they have waited, and the report says the
shape could not be named.

This never gates. It reports.

Usage::

    runner-wait-triage.py                  # every queued self-hosted job now
    runner-wait-triage.py --grace 5        # minutes before IDLE counts
    runner-wait-triage.py --selftest
"""

from __future__ import annotations

import argparse
import datetime as dt
import json
import os
import subprocess
import sys

REPO = os.environ.get("NROS_QUEUE_REPO", "NEWSLabNTU/nano-ros")

NO_RUNNER = "NO RUNNER"
OFFLINE = "OFFLINE"
BUSY = "BUSY"
IDLE = "IDLE"
STARTING = "STARTING"  # IDLE runner, but the job is younger than the grace

REMEDY = {
    NO_RUNNER: "no registered runner carries these labels: `just runner-up <labels>` on a host",
    OFFLINE: "every matching runner is offline: restart its container (`runner-container.sh <labels> --run`)",
    BUSY: "contention: every matching runner is running other work, so this ends by itself",
    IDLE: "a matching runner is idle beside this job (the issue-1365 third shape): check its "
          "version and listener log on the host, then restart it",
    STARTING: "a matching runner is idle and the job is still inside the grace window",
}


def gh_json(path: str):
    out = subprocess.run(
        ["gh", "api", "-H", "Accept: application/vnd.github+json", path],
        capture_output=True, text=True,
    )
    if out.returncode != 0:
        return None, out.stderr.strip() or out.stdout.strip()
    return json.loads(out.stdout), None


def matching(job_labels: list[str], runners: list[dict]) -> list[dict]:
    want = {label.lower() for label in job_labels}
    return [
        r for r in runners
        if want <= {lab["name"].lower() for lab in r.get("labels", [])}
    ]


def classify(job_labels: list[str], runners: list[dict], waited_min: float,
             grace_min: float) -> tuple[str, list[dict]]:
    found = matching(job_labels, runners)
    if not found:
        return NO_RUNNER, found
    online = [r for r in found if r.get("status") == "online"]
    if not online:
        return OFFLINE, found
    idle = [r for r in online if not r.get("busy")]
    if not idle:
        return BUSY, online
    return (IDLE if waited_min >= grace_min else STARTING), idle


def queued_self_hosted_jobs(repo: str) -> list[dict]:
    jobs: list[dict] = []
    seen: set[int] = set()
    for status in ("queued", "in_progress"):
        runs, err = gh_json(f"repos/{repo}/actions/runs?status={status}&per_page=100")
        if runs is None:
            raise SystemExit(f"runner-wait-triage: cannot list {status} runs: {err}")
        for run in runs.get("workflow_runs", []):
            if run["id"] in seen:
                continue
            seen.add(run["id"])
            page, err = gh_json(f"repos/{repo}/actions/runs/{run['id']}/jobs?filter=latest&per_page=100")
            if page is None:
                continue
            for job in page.get("jobs", []):
                labels = job.get("labels") or []
                if job.get("status") == "queued" and "self-hosted" in [x.lower() for x in labels]:
                    job["_workflow"] = run.get("path", "").rsplit("/", 1)[-1]
                    jobs.append(job)
    return jobs


def minutes_since(stamp: str, now: dt.datetime) -> float:
    t = dt.datetime.fromisoformat(stamp.replace("Z", "+00:00"))
    return (now - t).total_seconds() / 60.0


def report(repo: str, grace: float) -> int:
    now = dt.datetime.now(dt.timezone.utc)
    jobs = queued_self_hosted_jobs(repo)
    data, err = gh_json(f"repos/{repo}/actions/runners?per_page=100")
    runners = None if data is None else data.get("runners", [])

    if runners is not None:
        print(f"runners ({len(runners)} registered):")
        for r in runners:
            labels = ",".join(lab["name"] for lab in r.get("labels", [])
                              if lab.get("type") != "read-only")
            state = r.get("status", "?") + (" busy" if r.get("busy") else " idle")
            print(f"  {r['name']:<44} {state:<12} {labels}")
    else:
        print("runners: NOT VISIBLE with this token -- listing runners needs repository admin")
        print(f"  ({err})")
        print("  so the jobs below are reported with their wait, and the shape cannot be named.")

    if not jobs:
        print("\nqueued self-hosted jobs: none")
        return 0

    print(f"\nqueued self-hosted jobs ({len(jobs)}):")
    counts: dict[str, int] = {}
    for job in sorted(jobs, key=lambda j: j["created_at"]):
        waited = minutes_since(job["created_at"], now)
        head = f"  {waited:7.0f} min  {job['_workflow']} / {job['name']}  (job {job['id']})"
        if runners is None:
            print(head)
            continue
        shape, who = classify(job.get("labels") or [], runners, waited, grace)
        counts[shape] = counts.get(shape, 0) + 1
        print(f"{head}\n      {shape}: {REMEDY[shape]}")
        if who:
            print("      runners: " + ", ".join(r["name"] for r in who))
    if counts:
        print("\nsummary: " + ", ".join(f"{k} {v}" for k, v in sorted(counts.items())))
    return 0


def selftest() -> int:
    fail = 0

    def runner(name, status="online", busy=False, labels=("self-hosted", "nros-big")):
        return {"name": name, "status": status, "busy": busy,
                "labels": [{"name": x} for x in labels]}

    def want(label, got, exp):
        nonlocal fail
        if got != exp:
            print(f"  [FAIL] {label}: want {exp!r}, got {got!r}")
            fail += 1

    job = ["self-hosted", "nros-big"]
    want("nothing registered", classify(job, [], 30, 2)[0], NO_RUNNER)
    want("a runner without the job's label does not match",
         classify(job, [runner("a", labels=("self-hosted",))], 30, 2)[0], NO_RUNNER)
    want("labels match case-insensitively",
         classify(["Self-Hosted", "NROS-BIG"], [runner("a", busy=True)], 30, 2)[0], BUSY)
    want("registered but offline", classify(job, [runner("a", status="offline")], 30, 2)[0], OFFLINE)
    want("every online match busy",
         classify(job, [runner("a", busy=True), runner("b", status="offline")], 30, 2)[0], BUSY)
    want("an idle match beside a waiter",
         classify(job, [runner("a", busy=True), runner("b")], 30, 2), (IDLE, [runner("b")]))
    want("an idle match inside the grace window", classify(job, [runner("b")], 1, 2)[0], STARTING)
    if fail:
        return 1
    print("runner-wait-triage --selftest: OK (7 cases)")
    return 0


def main() -> int:
    ap = argparse.ArgumentParser(description=(__doc__ or "").splitlines()[0])
    ap.add_argument("--grace", type=float, default=2.0,
                    help="minutes a job may wait beside an idle runner before it counts (default 2)")
    ap.add_argument("--selftest", action="store_true")
    args = ap.parse_args()
    if args.selftest:
        return selftest()
    return report(REPO, args.grace)


if __name__ == "__main__":
    sys.exit(main())

#!/usr/bin/env python3
"""Bound how long a job may wait for a self-hosted runner, and make it visible.

A self-hosted job that no online runner can take is not a failure on GitHub.
It sits `queued` for up to 24 hours, and nothing turns red. Two things made
that expensive here:

  - The merge queue's L3 job shares the constant concurrency group
    `queue-l3`, so one L3 waiting for a runner holds every later queue run
    behind it. On 2026-11-12 that lasted 647 minutes.
  - A runner's supervision loop stops on purpose when the runner's labels are
    not true on its host (exit 78). Nothing outside the host notices
    (issue 2497).

Each run of this script looks at every queued or in-progress workflow run. A
job counts as waiting when its status is `queued` and its labels include
`self-hosted`. For each waiting job:

  - a MERGE-QUEUE job (labels include `nros-queue`, or the run's event is
    `merge_group`) waiting longer than --queue-budget minutes has its RUN
    cancelled. That releases the concurrency group. No queue.yml job is a
    required check, so cancelling ejects nothing.
  - any other self-hosted job waiting longer than --budget minutes is
    reported and left alone, because the scheduled lanes wait behind each
    other by design.

The script exits 1 when it found anything, so the workflow that runs it goes
red, and the report names the labels nobody serves. `--dry-run` reports
without cancelling.

Needs `gh`, authenticated with `actions: write` for the cancel. A self-test
runs on every invocation.
"""

import argparse
import datetime as dt
import json
import os
import subprocess
import sys

QUEUE_LABEL = "nros-queue"


def parse_ts(s):
    if not s:
        return None
    return dt.datetime.fromisoformat(s.replace("Z", "+00:00"))


def classify(runs, now, queue_budget, budget):
    """runs: [{id, name, event, html_url, jobs: [job dicts]}].

    Returns (cancel, report): `cancel` is a list of run ids to cancel, and
    `report` is a list of human lines.
    """
    cancel, report = [], []
    for run in runs:
        for job in run.get("jobs", []):
            labels = job.get("labels") or []
            if job.get("status") != "queued" or "self-hosted" not in labels:
                continue
            since = parse_ts(job.get("created_at")) or parse_ts(job.get("started_at"))
            if since is None:
                continue
            waited = (now - since).total_seconds() / 60
            is_queue = QUEUE_LABEL in labels or run.get("event") == "merge_group"
            limit = queue_budget if is_queue else budget
            if waited <= limit:
                continue
            what = (f"{run.get('name')} / {job.get('name')}: queued {waited:.0f} min "
                    f"(budget {limit}) waiting for a runner with "
                    f"[{', '.join(labels)}] -- {run.get('html_url', '')}")
            if is_queue:
                if run["id"] not in cancel:
                    cancel.append(run["id"])
                report.append("CANCELLED " + what)
            else:
                report.append("WAITING   " + what)
    return cancel, report


def gh_json(*args):
    out = subprocess.run(["gh", "api", *args], check=True, capture_output=True, text=True).stdout
    return json.loads(out) if out.strip() else {}


def fetch(repo):
    runs = []
    for status in ("queued", "in_progress"):
        page = gh_json(f"repos/{repo}/actions/runs?status={status}&per_page=100")
        runs += page.get("workflow_runs", [])
    seen, out = set(), []
    for r in runs:
        if r["id"] in seen:
            continue
        seen.add(r["id"])
        jobs = gh_json(f"repos/{repo}/actions/runs/{r['id']}/jobs?per_page=100").get("jobs", [])
        out.append({"id": r["id"], "name": r.get("name"), "event": r.get("event"),
                    "html_url": r.get("html_url"), "jobs": jobs})
    return out


def self_test():
    now = dt.datetime(2026, 11, 13, 12, 0, tzinfo=dt.timezone.utc)

    def job(mins, labels, status="queued", name="j"):
        t = (now - dt.timedelta(minutes=mins)).isoformat().replace("+00:00", "Z")
        return {"name": name, "status": status, "labels": labels, "created_at": t}

    q = ["self-hosted", "linux", QUEUE_LABEL]
    s = ["self-hosted", "linux", "nros-qemu"]
    cases = [
        # (runs, want_cancel, want_report_lines)
        ([{"id": 1, "event": "merge_group", "jobs": [job(31, q)]}], [1], 1),
        ([{"id": 1, "event": "merge_group", "jobs": [job(29, q)]}], [], 0),
        ([{"id": 2, "event": "merge_group", "jobs": [job(90, ["self-hosted", "nros-big"])]}], [2], 1),
        ([{"id": 3, "event": "schedule", "jobs": [job(90, s)]}], [], 0),
        ([{"id": 3, "event": "schedule", "jobs": [job(400, s)]}], [], 1),
        ([{"id": 4, "event": "merge_group", "jobs": [job(90, ["ubuntu-22.04"])]}], [], 0),
        ([{"id": 5, "event": "merge_group", "jobs": [job(90, q, status="in_progress")]}], [], 0),
        ([{"id": 6, "event": "merge_group", "jobs": [job(40, q, name="a"), job(50, q, name="b")]}], [6], 2),
    ]
    bad = 0
    for runs, want_cancel, want_lines in cases:
        c, r = classify(runs, now, queue_budget=30, budget=360)
        if c != want_cancel or len(r) != want_lines:
            print(f"  self-test FAIL: {runs} -> cancel={c} report={r}")
            bad += 1
    if bad:
        print(f"self-hosted-wait-watchdog self-test: {bad} of {len(cases)} case(s) FAILED")
        return 1
    print(f"self-hosted-wait-watchdog self-test: OK ({len(cases)} cases)")
    return 0


def main():
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    ap.add_argument("--repo", default=os.environ.get("GITHUB_REPOSITORY"))
    ap.add_argument("--queue-budget", type=int, default=30,
                    help="minutes a merge-queue job may wait for a runner (default 30)")
    ap.add_argument("--budget", type=int, default=360,
                    help="minutes any other self-hosted job may wait (default 360)")
    ap.add_argument("--dry-run", action="store_true", help="report, never cancel")
    ap.add_argument("--self-test", action="store_true", help="run the self-test only")
    a = ap.parse_args()
    if self_test() != 0:
        return 1
    if a.self_test:
        return 0
    if not a.repo:
        print("self-hosted-wait-watchdog: no --repo and no GITHUB_REPOSITORY", file=sys.stderr)
        return 2
    now = dt.datetime.now(dt.timezone.utc)
    cancel, report = classify(fetch(a.repo), now, a.queue_budget, a.budget)
    for run_id in cancel:
        if a.dry_run:
            continue
        subprocess.run(["gh", "api", "-X", "POST", f"repos/{a.repo}/actions/runs/{run_id}/cancel"],
                       check=False)
    lines = report or ["no self-hosted job is waiting past its budget"]
    summary = os.environ.get("GITHUB_STEP_SUMMARY")
    if summary:
        with open(summary, "a", encoding="utf8") as fh:
            fh.write("## Self-hosted runner wait\n\n" + "\n".join(f"- {x}" for x in lines) + "\n")
    for x in lines:
        print(x)
    if report:
        print("\nA job above has waited past its budget for a self-hosted runner. Either no "
              "runner carrying those labels is online, or every runner that carries them is busy. "
              "On the runner host: `just runner-doctor <labels>`, then restart its loop "
              "(scripts/ci/runner-loop-container.sh).", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())

#!/usr/bin/env python3
"""Report an expected self-hosted runner that has stopped taking jobs.

`self-hosted-wait-watchdog.py` catches a job that waits too long for a runner.
It cannot see a runner that stopped while another runner with the same labels
kept up: no job waits, so there is nothing to report, and the loss is
capacity rather than a stuck queue. On 2026-11-13 host 118's runner loop
stopped after three refused registrations, host 241 took every job, and the
stop was found only because someone read the runner list.

Listing runners needs admin, and the workflow token is not admin. A finished
or running job does carry `runner_name`, which `actions: read` can see. So
the runners we EXPECT are declared in `.config/self-hosted-runners.json`, and
for each one this script asks, over the last --window minutes:

  - did it take any job (finished, or running now)?
  - how many jobs that it COULD have taken ran on another runner instead?

A runner that took nothing while at least --min-missed jobs it serves ran
elsewhere is reported, and the script exits 1 so the workflow goes red. A
runner that took nothing while nothing it serves ran at all is idle, not
missing, and is not reported.

The second question is about LABELS, not runners: is there a label set that
workflows ask for, and that no runner took a single job for? When the only
runner carrying a label stops, no job runs elsewhere either, so the first
question sees nothing. On 2026-11-14 both host-241 loops stopped (three
refused `setfacl` starts each), the `nros-queue` runner deregistered, and from
05:08Z every merge-queue L3 job sat unassigned until its group merged and
GitHub cancelled it. So a label set with at least --min-missed jobs in the
window that ended or still wait WITHOUT a runner, and none that got one, is
reported, naming the expected runners that carry it.

A job is one the runner serves when every label the job asks for is one the
runner carries. GitHub compares labels without regard to case, and adds
`self-hosted`, `Linux` and `X64` to every Linux runner itself.

Only workflows that mention `self-hosted` are fetched, read from the checked
out `.github/workflows/`, so the API cost follows the self-hosted lanes rather
than every hosted run. A self-test runs on every invocation.
"""

import argparse
import datetime as dt
import json
import os
import pathlib
import subprocess
import sys

IMPLICIT = {"self-hosted", "linux", "x64"}


def parse_ts(s):
    if not s:
        return None
    return dt.datetime.fromisoformat(s.replace("Z", "+00:00"))


def serves(runner_labels, job_labels):
    want = {x.lower() for x in job_labels}
    if "self-hosted" not in want:
        return False
    have = IMPLICIT | {x.lower() for x in runner_labels}
    return want <= have


def classify(runners, jobs, now, window, min_missed):
    """runners: [{name, labels}]; jobs: [{name, labels, runner_name, status,
    started_at, run_url}]. Returns the report lines."""
    since = now - dt.timedelta(minutes=window)
    recent = []
    for j in jobs:
        start = parse_ts(j.get("started_at"))
        if not j.get("runner_name"):
            continue
        if j.get("status") == "in_progress" or (start and start >= since):
            recent.append(j)
    report = []
    # Label sets nobody served: jobs that ended or still wait with no runner,
    # and not one job of the same set that got a runner.
    sets = {}
    for j in jobs:
        labels = j.get("labels") or []
        if "self-hosted" not in {x.lower() for x in labels}:
            continue
        start = parse_ts(j.get("started_at"))
        if not (j.get("status") in ("queued", "in_progress") or (start and start >= since)):
            continue
        key = tuple(sorted({x.lower() for x in labels}))
        took, missed = sets.setdefault(key, ([], []))
        (took if j.get("runner_name") else missed).append(j)
    for key, (took, missed) in sorted(sets.items()):
        if took or len(missed) < min_missed:
            continue
        carriers = [r["name"] for r in runners if serves(r["labels"], list(key))]
        report.append(
            f"no runner took any of {len(missed)} job(s) asking for [{', '.join(key)}] in the "
            f"last {window} min (latest: {missed[-1].get('name')} {missed[-1].get('run_url', '')}); "
            + (f"expected to serve it: {', '.join(carriers)}" if carriers
               else "no runner in .config/self-hosted-runners.json carries it"))
    for r in runners:
        took = [j for j in recent if j["runner_name"] == r["name"]]
        if took:
            continue
        missed = [j for j in recent
                  if j["runner_name"] != r["name"] and serves(r["labels"], j.get("labels") or [])]
        if len(missed) < min_missed:
            continue
        others = sorted({j["runner_name"] for j in missed})
        report.append(
            f"{r['name']} took no job in the last {window} min, while {len(missed)} job(s) "
            f"it serves ran on {', '.join(others)} (latest: {missed[-1].get('name')} "
            f"{missed[-1].get('run_url', '')})")
    return report


def self_hosted_workflows(root):
    """Workflows with a `runs-on:` line that can name `self-hosted` (a literal
    or an expression). A file that merely MENTIONS it, in a comment or a
    script, puts every one of its runs through a jobs call for nothing: the
    first version read ten files and fetched 2124 jobs over 8 h."""
    out = set()
    for p in sorted((root / ".github" / "workflows").glob("*.y*ml")):
        for line in p.read_text(encoding="utf8").splitlines():
            s = line.strip()
            if s.startswith("runs-on:") and "self-hosted" in s:
                out.add(f".github/workflows/{p.name}")
                break
    return out


def gh_json(*args):
    out = subprocess.run(["gh", "api", *args], check=True, capture_output=True, text=True).stdout
    return json.loads(out) if out.strip() else {}


def fetch(repo, paths, now, window):
    """Jobs of the self-hosted workflows' runs created in the window, plus runs
    still running or waiting. Listed PER WORKFLOW and paged: a repository-wide
    listing of 100 runs is mostly gate and automation runs, and dropped the
    merge-queue runs this check exists for."""
    since = (now - dt.timedelta(minutes=window)).strftime("%Y-%m-%dT%H:%M:%SZ")
    runs = []
    for path in sorted(paths):
        wf = path.rsplit("/", 1)[-1]
        queries = [f"created=>={since}"] + [f"status={s}" for s in ("in_progress", "queued", "pending", "waiting")]
        for q in queries:
            page = 1
            while True:
                got = gh_json(f"repos/{repo}/actions/workflows/{wf}/runs?{q}&per_page=100&page={page}").get("workflow_runs", [])
                runs += got
                if len(got) < 100:
                    break
                page += 1
    seen, jobs = set(), []
    for run in runs:
        if run["id"] in seen:
            continue
        seen.add(run["id"])
        # A pull request never reaches a self-hosted runner here (H6; gate.yml
        # chooses self-hosted only for merge_group, schedule and dispatch), and
        # PR runs are most of gate.yml's: skip them before the jobs call.
        if run.get("event") in ("pull_request", "pull_request_target"):
            continue
        for j in gh_json(f"repos/{repo}/actions/runs/{run['id']}/jobs?per_page=100").get("jobs", []):
            jobs.append({"name": f"{run.get('name')} / {j.get('name')}",
                         "labels": j.get("labels") or [],
                         "runner_name": j.get("runner_name"),
                         "status": j.get("status"),
                         "started_at": j.get("started_at"),
                         "run_url": run.get("html_url")})
    jobs.sort(key=lambda j: j.get("started_at") or "")
    return jobs


def self_test():
    now = dt.datetime(2026, 11, 13, 12, 0, tzinfo=dt.timezone.utc)

    def job(runner, labels, mins_ago, status="completed"):
        t = (now - dt.timedelta(minutes=mins_ago)).isoformat().replace("+00:00", "Z")
        return {"name": "L3", "labels": labels, "runner_name": runner,
                "status": status, "started_at": t}

    a = {"name": "A", "labels": ["nros-sdk-zephyr", "nros-big"]}
    b = {"name": "B", "labels": ["nros-qemu", "nros-sdk-zephyr", "nros-big"]}
    l3 = ["self-hosted", "linux", "nros-sdk-zephyr", "nros-big"]
    qemu = ["self-hosted", "Linux", "nros-qemu"]
    qr = {"name": "Q", "labels": ["nros-queue", "nros-sdk-zephyr", "nros-big"]}
    q3 = ["self-hosted", "linux", "nros-queue"]
    cases = [
        # (name, runners, jobs, want_report_lines)
        ("A stopped, B took two jobs A serves", [a, b], [job("B", l3, 30), job("B", l3, 90)], 1),
        ("A took one in the window", [a, b], [job("A", l3, 100), job("B", l3, 30), job("B", l3, 90)], 0),
        ("A's only job is before the window", [a, b], [job("A", l3, 400), job("B", l3, 30), job("B", l3, 90)], 1),
        ("A is running a long job now", [a, b], [job("A", l3, 600, "in_progress"), job("B", l3, 30), job("B", l3, 90)], 0),
        ("one missed job is under the floor", [a, b], [job("B", l3, 30)], 0),
        ("jobs A cannot serve do not count", [a, b], [job("B", qemu, 30), job("B", qemu, 90)], 0),
        ("hosted jobs do not count", [a], [job("GitHub Actions 1", ["ubuntu-22.04"], 30)] * 3, 0),
        ("idle: nothing ran at all", [a, b], [], 0),
        ("label case is ignored", [a, b], [job("B", ["Self-Hosted", "NROS-BIG"], 30)] * 2, 1),
        # 2026-11-14: the only nros-queue runner gone; its jobs got no runner.
        ("a label nobody serves: two jobs with no runner", [a, b, qr],
         [job(None, q3, 30, "queued"), job(None, q3, 90)], 1),
        ("one unassigned job is under the floor", [a, b, qr], [job(None, q3, 30, "queued")], 0),
        ("a label set some runner served is not unserved", [a, b, qr],
         [job(None, q3, 30, "queued"), job(None, q3, 90), job("Q", q3, 60)], 0),
    ]
    bad = 0
    for name, runners, jobs, want in cases:
        got = classify(runners, jobs, now, window=180, min_missed=2)
        if len(got) != want:
            print(f"  self-test FAIL: {name}: {got}")
            bad += 1
    if bad:
        print(f"self-hosted-presence-watchdog self-test: {bad} of {len(cases)} case(s) FAILED")
        return 1
    print(f"self-hosted-presence-watchdog self-test: OK ({len(cases)} cases)")
    return 0


def main():
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    ap.add_argument("--repo", default=os.environ.get("GITHUB_REPOSITORY"))
    ap.add_argument("--root", default=".", help="checkout root (default .)")
    ap.add_argument("--window", type=int, default=180,
                    help="minutes to look back (default 180)")
    ap.add_argument("--min-missed", type=int, default=2,
                    help="jobs a runner must have missed before it is reported (default 2)")
    ap.add_argument("--self-test", action="store_true", help="run the self-test only")
    a = ap.parse_args()
    if self_test() != 0:
        return 1
    if a.self_test:
        return 0
    if not a.repo:
        print("self-hosted-presence-watchdog: no --repo and no GITHUB_REPOSITORY", file=sys.stderr)
        return 2
    root = pathlib.Path(a.root)
    runners = json.loads((root / ".config" / "self-hosted-runners.json").read_text(encoding="utf8"))["runners"]
    now = dt.datetime.now(dt.timezone.utc)
    jobs = fetch(a.repo, self_hosted_workflows(root), now, a.window)
    report = classify(runners, jobs, now, a.window, a.min_missed)
    lines = report or [f"every expected runner took a job in the last {a.window} min, or had none "
                       f"to take, and every self-hosted label set asked for was served"]
    summary = os.environ.get("GITHUB_STEP_SUMMARY")
    if summary:
        with open(summary, "a", encoding="utf8") as fh:
            fh.write("## Self-hosted runner presence\n\n" + "\n".join(f"- {x}" for x in lines) + "\n")
    for x in lines:
        print(x)
    if report:
        print("\nAn expected runner above is not taking jobs. On its host, check the runner "
              "loop (scripts/ci/runner-loop-container.sh) and its log; restart it once "
              "`just runner-doctor <labels>` passes. If the host was retired, remove it from "
              ".config/self-hosted-runners.json.", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())

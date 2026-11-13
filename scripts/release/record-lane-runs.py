#!/usr/bin/env python3
"""Record the last run of each board-verifying lane, when a release is cut
(issue 2325).

A board row says HOW OFTEN it is verified (`cadence`); this records WHEN that
last happened and WHAT it said, so `nros board show` and `nros doctor --report`
can answer "when was it last run, and did it pass?" from an installed release
with no network and no GitHub account. The cadence -> workflow map is the one
`scripts/check-board-tiers.py` enforces (packages/boards/board-support.toml's
header): daily = the nightly sweep, weekly = the platform probe, merge = the
merge queue.

A lane's run is its newest COMPLETED run on `main` from the event that IS the
cadence (a schedule, or the merge queue), never a manual dispatch. Each run
records its per-job conclusions, because the nightly sweep runs one job per
board, named after the row's `matrix_platform` (phase-558; before it, after
the row's `nightly_token`), and a sweep red on one board says nothing about
another.

Usage:
    record-lane-runs.py --repo OWNER/NAME --out <file>     (needs `gh`, GH_TOKEN)
    record-lane-runs.py --self-test
"""

from __future__ import annotations

import argparse
import json
import subprocess
import sys
from datetime import datetime, timezone

LANES = {
    "daily": ("nightly.yml", "schedule"),
    "weekly": ("weekly.yml", "schedule"),
    "merge": ("queue.yml", "merge_group"),
}


def gh_json(args: list[str]):
    r = subprocess.run(["gh", *args], capture_output=True, text=True)
    if r.returncode != 0:
        raise RuntimeError(f"gh {' '.join(args)}: {r.stderr.strip()}")
    return json.loads(r.stdout)


def lane_record(repo: str, workflow: str, event: str, fetch=gh_json) -> dict:
    # A merge-queue run is on `gh-readonly-queue/main/...`, never on `main`, so
    # only a scheduled lane can be filtered by branch.
    branch = ["--branch", "main"] if event == "schedule" else []
    runs = fetch([
        "run", "list", "--repo", repo, "--workflow", workflow, *branch,
        "--event", event, "--status", "completed", "--limit", "30",
        "--json", "databaseId,conclusion,createdAt,headSha,url",
    ])
    # The newest run that reached a VERDICT: the queue cancels a superseded
    # entry, and a cancelled or skipped run verified nothing.
    runs = [r for r in runs if r.get("conclusion") in ("success", "failure")]
    if not runs:
        return {"workflow": workflow, "event": event, "run": None}
    run = runs[0]
    jobs = fetch(["run", "view", str(run["databaseId"]), "--repo", repo, "--json", "jobs"])
    return {
        "workflow": workflow,
        "event": event,
        "run": {
            "url": run["url"],
            "date": run["createdAt"][:10],
            "conclusion": run["conclusion"] or "unknown",
            "commit": run["headSha"],
        },
        "jobs": {j["name"]: (j.get("conclusion") or "unknown") for j in jobs.get("jobs", [])},
    }


def record(repo: str, fetch=gh_json) -> dict:
    return {
        "recorded": datetime.now(timezone.utc).strftime("%Y-%m-%d"),
        "lanes": {cad: lane_record(repo, wf, ev, fetch) for cad, (wf, ev) in LANES.items()},
    }


def self_test() -> None:
    def fake(args):
        if args[:2] == ["run", "list"]:
            wf = args[args.index("--workflow") + 1]
            if wf == "weekly.yml":
                return []
            return [{"databaseId": 7, "conclusion": "failure", "createdAt": "2026-11-07T05:07:15Z",
                     "headSha": "abc", "url": "https://x/runs/7"}]
        return {"jobs": [{"name": "qemu", "conclusion": "success"},
                         {"name": "nuttx", "conclusion": "failure"}]}
    doc = record("o/r", fake)
    daily = doc["lanes"]["daily"]
    assert daily["run"]["date"] == "2026-11-07" and daily["run"]["conclusion"] == "failure", daily
    assert daily["jobs"] == {"qemu": "success", "nuttx": "failure"}, daily
    assert doc["lanes"]["weekly"]["run"] is None, doc
    assert set(doc["lanes"]) == {"daily", "weekly", "merge"}
    json.dumps(doc)
    print("record-lane-runs.py self-test OK")


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--self-test", action="store_true")
    ap.add_argument("--repo")
    ap.add_argument("--out")
    a = ap.parse_args()
    if a.self_test:
        self_test()
        return 0
    if not (a.repo and a.out):
        ap.error("--repo and --out are required")
    doc = record(a.repo)
    with open(a.out, "w", encoding="utf-8") as fh:
        json.dump(doc, fh, indent=1, sort_keys=True)
        fh.write("\n")
    for cad, lane in doc["lanes"].items():
        run = lane["run"]
        print(f"record-lane-runs: {cad} {lane['workflow']}: "
              + (f"{run['date']} {run['conclusion']}" if run else "no completed run"))
    return 0


if __name__ == "__main__":
    sys.exit(main())

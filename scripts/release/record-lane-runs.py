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

It also records each board's RESULTS HISTORY (RFC-0114 D13, phase-585 W5):
the `board-results.json` every board-verifying lane uploads per run
(`scripts/ci/board-results.py`, W2/W8), merged per registry board key, newest
first, at most `HISTORY_RUNS`. `nros board list --status` / `show` derive the
board's status word from it offline, by the rule the book uses
(`board_registry.status_from_evidence` and its Rust twin), and draw the same
marks.

Usage:
    record-lane-runs.py --repo OWNER/NAME --out <file>     (needs `gh`, GH_TOKEN)
    record-lane-runs.py --self-test
"""

from __future__ import annotations

import argparse
import json
import subprocess
import sys
import tempfile
from datetime import datetime, timezone
from pathlib import Path

LANES = {
    "daily": ("nightly.yml", "schedule"),
    "weekly": ("weekly.yml", "schedule"),
    "merge": ("queue.yml", "merge_group"),
}


# RFC-0114 D13 (phase-585 W5) -- the lanes that upload a `board-results`
# artifact, and the events whose runs count. A dispatch on `main` is evidence
# too (user ruling 2026-11-24); the derivation gives it no cadence.
HISTORY_LANES = (
    ("nightly.yml", ("schedule", "workflow_dispatch")),
    ("weekly.yml", ("schedule", "workflow_dispatch")),
    ("live-peer.yml", ("schedule", "workflow_dispatch")),
    ("host-tests.yml", ("schedule", "workflow_dispatch")),
)
# D13's N: the runs a board's history keeps, the marks the book and the CLI draw.
HISTORY_RUNS = 6
# How many completed runs per lane are read for their artifact.
HISTORY_SCAN = 12


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


def gh_board_results(repo: str, run_id: int) -> dict | None:
    """The run's `board-results.json`, or None when it uploaded none."""
    with tempfile.TemporaryDirectory() as d:
        r = subprocess.run(["gh", "run", "download", str(run_id), "--repo", repo,
                            "--name", "board-results", "--dir", d],
                           capture_output=True, text=True)
        f = Path(d) / "board-results.json"
        if r.returncode != 0 or not f.is_file():
            return None
        return json.loads(f.read_text())


def board_histories(repo: str, fetch=gh_json, download=gh_board_results) -> dict:
    """registry board key -> its newest `HISTORY_RUNS` results, newest first."""
    entries: list[tuple[str, int, str, dict]] = []
    for workflow, events in HISTORY_LANES:
        runs = fetch([
            "run", "list", "--repo", repo, "--workflow", workflow, "--branch", "main",
            "--status", "completed", "--limit", str(HISTORY_SCAN),
            "--json", "databaseId,event,createdAt",
        ])
        for run in runs:
            if run.get("event") not in events:
                continue
            doc = download(repo, run["databaseId"])
            if not doc:
                continue
            for key, b in (doc.get("boards") or {}).items():
                outcome = b.get("outcome")
                if not outcome:
                    continue
                entries.append((doc.get("date") or run["createdAt"][:10], run["databaseId"],
                                key, {"date": doc.get("date") or run["createdAt"][:10],
                                      "workflow": workflow, "event": run.get("event"),
                                      "outcome": outcome,
                                      "url": doc.get("url", "")}))
    # Newest first; a run id breaks a same-day tie (ids grow with time).
    entries.sort(key=lambda e: (e[0], e[1]), reverse=True)
    out: dict[str, list[dict]] = {}
    for _date, _id, key, rec in entries:
        hist = out.setdefault(key, [])
        if len(hist) < HISTORY_RUNS:
            hist.append(rec)
    return out


def record(repo: str, fetch=gh_json, download=gh_board_results) -> dict:
    return {
        "recorded": datetime.now(timezone.utc).strftime("%Y-%m-%d"),
        "lanes": {cad: lane_record(repo, wf, ev, fetch) for cad, (wf, ev) in LANES.items()},
        "boards": board_histories(repo, fetch, download),
    }


def self_test() -> None:
    def fake(args):
        if args[:2] == ["run", "list"]:
            wf = args[args.index("--workflow") + 1]
            if "--event" not in args:
                # The history scan: nightly has three runs, one a push (not
                # evidence) and one without an artifact; host-tests one.
                return {
                    "nightly.yml": [
                        {"databaseId": 30, "event": "schedule", "createdAt": "2026-11-09T05:00:00Z"},
                        {"databaseId": 20, "event": "push", "createdAt": "2026-11-08T05:00:00Z"},
                        {"databaseId": 10, "event": "workflow_dispatch", "createdAt": "2026-11-07T05:00:00Z"},
                        {"databaseId": 5, "event": "schedule", "createdAt": "2026-11-06T05:00:00Z"},
                    ],
                    "host-tests.yml": [
                        {"databaseId": 31, "event": "schedule", "createdAt": "2026-11-09T06:00:00Z"},
                    ],
                }.get(wf, [])
            if wf == "weekly.yml":
                return []
            return [{"databaseId": 7, "conclusion": "failure", "createdAt": "2026-11-07T05:07:15Z",
                     "headSha": "abc", "url": "https://x/runs/7"}]
        return {"jobs": [{"name": "qemu", "conclusion": "success"},
                         {"name": "nuttx", "conclusion": "failure"}]}

    def download(_repo, run_id):
        return {
            30: {"date": "2026-11-09", "boards": {"linux@Linux": {"outcome": "failed"},
                                                  "tx@ThreadxLinux": {"outcome": "passed"}}},
            20: {"date": "2026-11-08", "boards": {"linux@Linux": {"outcome": "passed"}}},
            10: {"date": "2026-11-07", "boards": {"linux@Linux": {"outcome": "no-verdict"}}},
            31: {"date": "2026-11-09", "boards": {"linux@Linux": {"outcome": "passed"}}},
        }.get(run_id)

    doc = record("o/r", fake, download)
    linux = doc["boards"]["linux@Linux"]
    # Newest first; the host-tests run (id 31) beats the nightly's (30) on the
    # same day; the push run (20) is not evidence; run 5 uploaded nothing.
    assert [(h["workflow"], h["outcome"]) for h in linux] == [
        ("host-tests.yml", "passed"), ("nightly.yml", "failed"),
        ("nightly.yml", "no-verdict")], linux
    assert linux[0]["event"] == "schedule" and linux[2]["event"] == "workflow_dispatch", linux
    assert [h["outcome"] for h in doc["boards"]["tx@ThreadxLinux"]] == ["passed"], doc["boards"]
    many = {i: {"date": f"2026-11-{i:02d}", "boards": {"b@B": {"outcome": "passed"}}}
            for i in range(1, 11)}
    capped = board_histories(
        "o/r",
        lambda a: [{"databaseId": i, "event": "schedule", "createdAt": f"2026-11-{i:02d}T00:00:00Z"}
                   for i in range(1, 11)] if a[a.index("--workflow") + 1] == "nightly.yml" else [],
        lambda _r, i: many.get(i))
    assert [h["date"] for h in capped["b@B"]] == [f"2026-11-{i:02d}" for i in range(10, 4, -1)], capped

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
    print(f"record-lane-runs: results history for {len(doc['boards'])} board(s), "
          f"at most {HISTORY_RUNS} run(s) each")
    return 0


if __name__ == "__main__":
    sys.exit(main())

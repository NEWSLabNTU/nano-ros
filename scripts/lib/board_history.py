"""Each registry board's recent results, from the `board-results` artifacts the
board-verifying lanes upload (RFC-0114 D13.3; `scripts/ci/board-results.py`).

ONE reader for "what did the lanes say about this board lately", so the daily
lane-health report (issue 2997) and the release recorder (phase-585 W5) cannot
come to disagree about which runs count.

    histories(repo, fetch, download) -> {board key: [result, ...]}, newest first

A result is `{"date", "workflow", "event", "outcome", "url"}`, `outcome` one of
`passed`, `failed`, `no-verdict`, `not-run`. `fetch` lists runs (`gh run list`
JSON), `download` returns one run's `board-results.json` or None; both are
parameters so a self-test needs no network.
"""

from __future__ import annotations

import json
import subprocess
import tempfile
from pathlib import Path

# The lanes that upload a `board-results` artifact, and the events whose runs
# count. A dispatch on `main` is evidence too (user ruling 2026-11-24); the
# status derivation gives it no cadence.
LANES = (
    ("nightly.yml", ("schedule", "workflow_dispatch")),
    ("weekly.yml", ("schedule", "workflow_dispatch")),
    ("live-peer.yml", ("schedule", "workflow_dispatch")),
    ("host-tests.yml", ("schedule", "workflow_dispatch")),
    ("run-matrix.yml", ("schedule", "workflow_dispatch")),
)
# D13's N: the results a history keeps.
HISTORY_RUNS = 6
# How many completed runs per lane are read for their artifact.
SCAN = 12

VERDICTS = ("passed", "failed")
# The cadences a lane delivers; `manual` is a maintainer's report.
SCHEDULED = ("daily", "weekly", "merge")


def gh_json(args: list[str]):
    r = subprocess.run(["gh", *args], capture_output=True, text=True)
    if r.returncode != 0:
        raise RuntimeError(f"gh {' '.join(args)}: {r.stderr.strip()}")
    return json.loads(r.stdout)


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


def histories(repo: str, fetch=gh_json, download=gh_board_results,
              keep: int = HISTORY_RUNS) -> dict[str, list[dict]]:
    """registry board key -> its newest `keep` results, newest first."""
    entries: list[tuple[str, int, str, dict]] = []
    for workflow, events in LANES:
        runs = fetch([
            "run", "list", "--repo", repo, "--workflow", workflow, "--branch", "main",
            "--status", "completed", "--limit", str(SCAN),
            "--json", "databaseId,event,createdAt",
        ]) or []
        for run in runs:
            if run.get("event") not in events:
                continue
            doc = download(repo, run["databaseId"])
            if not doc:
                continue
            date = doc.get("date") or run["createdAt"][:10]
            for key, b in (doc.get("boards") or {}).items():
                outcome = b.get("outcome")
                if not outcome:
                    continue
                entries.append((date, run["databaseId"], key, {
                    "date": date, "workflow": workflow, "event": run.get("event"),
                    "outcome": outcome, "url": doc.get("url", "")}))
    # Newest first; a run id breaks a same-day tie (ids grow with time).
    entries.sort(key=lambda e: (e[0], e[1]), reverse=True)
    out: dict[str, list[dict]] = {}
    for _date, _id, key, rec in entries:
        hist = out.setdefault(key, [])
        if len(hist) < keep:
            hist.append(rec)
    return out


def silent_boards(status: dict, hist: dict[str, list[dict]]) -> list[tuple[str, str]]:
    """(board key, why) for every board a SCHEDULED lane owes a verdict -- a
    non-infra registry row with a matrix platform and a nightly job or a
    scheduled cadence (a `manual` row's evidence is a person's report, not a
    lane's) -- whose recent results hold no verdict at all: every one
    no-verdict / not-run, or none recorded. Such a board reads "Builds" or
    "Not run since" while CI looks green (issue 2997)."""
    out = []
    for r in status.get("rows", []):
        if r.get("infra") or not r.get("matrix_platform"):
            continue
        if not (r.get("nightly_job") or r.get("cadence") in SCHEDULED):
            continue
        h = hist.get(r["key"], [])
        if not h:
            out.append((r["key"], "no lane recorded a result for it"))
        elif not any(x["outcome"] in VERDICTS for x in h):
            lanes = sorted({x["workflow"] for x in h})
            out.append((r["key"], f"no verdict in its last {len(h)} result(s) "
                                  f"({', '.join(lanes)}): "
                                  + " ".join(x["outcome"] for x in h)))
    return out


def self_test() -> list[str]:
    fails = []

    def fake(args):
        wf = args[args.index("--workflow") + 1]
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

    def download(_repo, run_id):
        return {
            30: {"date": "2026-11-09", "boards": {"linux@Linux": {"outcome": "failed"},
                                                  "nx@NuttxArm": {"outcome": "no-verdict"}}},
            20: {"date": "2026-11-08", "boards": {"linux@Linux": {"outcome": "passed"}}},
            10: {"date": "2026-11-07", "boards": {"linux@Linux": {"outcome": "no-verdict"},
                                                  "nx@NuttxArm": {"outcome": "not-run"}}},
            31: {"date": "2026-11-09", "boards": {"linux@Linux": {"outcome": "passed"}}},
        }.get(run_id)

    h = histories("o/r", fake, download)
    got = [(x["workflow"], x["outcome"]) for x in h["linux@Linux"]]
    if got != [("host-tests.yml", "passed"), ("nightly.yml", "failed"),
               ("nightly.yml", "no-verdict")]:
        fails.append(f"merge order / push filter / missing artifact: {got}")
    many = {i: {"date": f"2026-11-{i:02d}", "boards": {"b@B": {"outcome": "passed"}}}
            for i in range(1, 11)}
    capped = histories(
        "o/r",
        lambda a: [{"databaseId": i, "event": "schedule",
                    "createdAt": f"2026-11-{i:02d}T00:00:00Z"} for i in range(1, 11)]
        if a[a.index("--workflow") + 1] == "nightly.yml" else [],
        lambda _r, i: many.get(i))
    if [x["date"] for x in capped["b@B"]] != [f"2026-11-{i:02d}" for i in range(10, 4, -1)]:
        fails.append(f"cap at {HISTORY_RUNS}, newest first: {capped['b@B']}")

    status = {"rows": [
        {"key": "linux@Linux", "matrix_platform": "Linux", "cadence": "daily"},
        {"key": "nx@NuttxArm", "matrix_platform": "NuttxArm", "nightly_job": "NuttxArm"},
        {"key": "qb@QemuBaremetal", "matrix_platform": "QemuBaremetal", "cadence": "daily"},
        {"key": "infra", "infra": True, "matrix_platform": "Linux", "cadence": "daily"},
        {"key": "buildonly@-", "matrix_platform": None, "cadence": "daily"},
        # A manual board (an FVP a maintainer boots): no lane owes it one.
        {"key": "fvp@Fvp", "matrix_platform": "Fvp", "cadence": "manual"},
    ]}
    silent = dict(silent_boards(status, h))
    if set(silent) != {"nx@NuttxArm", "qb@QemuBaremetal"}:
        fails.append(f"silent boards: {sorted(silent)}")
    if "no verdict in its last 2" not in silent.get("nx@NuttxArm", ""):
        fails.append(f"why (nx): {silent.get('nx@NuttxArm')}")
    if "no lane recorded" not in silent.get("qb@QemuBaremetal", ""):
        fails.append(f"why (qb): {silent.get('qb@QemuBaremetal')}")
    return fails

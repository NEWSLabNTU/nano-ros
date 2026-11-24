#!/usr/bin/env python3
"""Which lanes on `main` are producing a signal, and which have stopped?

`nightly-triage` answers "did the nightly report?" for ONE workflow, and
`tier-health` answers "does each platform's evidence support its tier?". Neither
asks the question that let `gate.yml`'s push lane sit red for 100 consecutive
runs (issue 1345) and `host-suite.yml` complete 3 of 29 (2026-10-03): across
EVERY lane that runs on `main` without a pull request in front of it, which ones
have been red, or have never finished, for long enough that a new regression
landing in them would be invisible?

A lane red on every run has no signal capacity: "still red" and "newly red" are
the same colour in the run list. That is how issue 0876 rode in. The fix for
any one such lane is local; noticing that a lane has become one is not, because
nothing that ever reads a pull request's checks reads these.

WHAT IT READS

The lane set is HARVESTED from `.github/workflows/*.yml`: every workflow whose
`on:` names `schedule` or `push`. Never a list written here — an authored list
is only as complete as whoever last remembered to edit it (issue 1452's
argument). For each, the recent runs on the default branch from those events
(and `workflow_dispatch`), newest first.

ONLY CONCLUSIVE OUTCOMES ARE VERDICTS

`success` and `failure` are verdicts; `cancelled`, `skipped` and a run still in
flight are not (`tier-health`'s rule, for its reason: counting a cancellation as
a failure reports a scheduling artifact as a defect). But a lane whose window is
ALL inconclusive is reported too, as NEVER FINISHES — that is `host-suite`'
shape, and it is exactly as silent as a standing red.

STATES

  OK              newest verdict is green
  red             newest verdict is red, streak below the threshold
  NO SIGNAL       red on the last N verdicts (default 3) — fix the lane
  NEVER FINISHES  runs in the window, none conclusive — fix the trigger
  RARELY FINISHES fewer than 3 in 10 runs (of 5+) conclusive — host-suite'
                  measured 3-of-29; what it says is about whichever commit
                  happened to finish, not about `main`
  no runs         nothing on main in the window (dispatch-only, or new)
  NO VERDICT 2d   (scheduled lanes, phase-591 W8) the newest run's UTC day
                  and the day before produced no run whose cells all ran

NO VERDICT is about CELLS, not colour (phase-591 W8). A green run is a verdict.
A red run is a verdict only if every failed job reached what it tests: a job
that failed in preparation (`nightly-triage`'s classifier, shared), or whose
failing step was followed by a SKIPPED test step in the same job (a fixture
build before its cells: `Build (X)` then `Test / e2e (X)`, `just build tier1`
then `just ci tier1`, `Build the fixtures ...` then `Run the cells ...`), ran
no cell and is a NO VERDICT job. A day with no run at all has no verdict
either. Over 30 runs to 2026-11-23 most reds were the second kind, so each
blocker hid the next for a night (phase-591's "Why"); this names a lane in
that state on the second morning instead of the fifth. `--as-of <date>`
replays the rule against an earlier day, which is how it was checked against
the week before it landed.

For each red lane it names the first failing JOB and STEP of the newest red run,
because a lane stopping at a setup step and a lane failing its tests want
different people.

IT REPORTS. IT DOES NOT GATE. Exit 0 on any lane state; non-zero only when the
TOOL is broken (its self-test, an unreadable workflow). A reporter that can go
red about lanes being red is a second thing to ignore.

Usage::

    lane-health.py                    # every harvested lane, last 10 runs each
    lane-health.py --runs 20 --threshold 5
    lane-health.py --markdown         # the table automation.yml's triage job writes
    lane-health.py --as-of 2026-11-22 # replay the NO VERDICT rule on an earlier day
    lane-health.py --selftest
"""

from __future__ import annotations

import argparse
import importlib.util
import json
import os
import pathlib
import re
import subprocess
import sys

REPO = os.environ.get("NROS_QUEUE_REPO", "NEWSLabNTU/nano-ros")
ROOT = pathlib.Path(__file__).resolve().parents[2]
WORKFLOWS = ROOT / ".github" / "workflows"
EVENTS = {"schedule", "push", "workflow_dispatch"}
VERDICTS = {"success", "failure"}

# phase-591 W8 -- the preparation classifier is `nightly-triage`'s, imported
# rather than copied, so the two reports cannot disagree about a setup step.
_spec = importlib.util.spec_from_file_location(
    "nightly_triage", pathlib.Path(__file__).with_name("nightly-triage.py"))
nightly_triage = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(nightly_triage)

# A step whose running IS a lane's cells. A failed step followed by a SKIPPED
# step like this in the same job stopped before its cells. Word boundaries, as
# `nightly-triage` learned (`run` inside `runner`).
CELL_STEP = re.compile(r"\b(test|tests|e2e|cells?|ci tier\w*|ci matrix\S*)\b", re.I)


def job_reached_cells(job: dict) -> tuple[bool, str]:
    """(reached, why) for one job of a run. A green, skipped or cancelled job
    is not evidence of a missing verdict; only a failed one can be."""
    if job.get("conclusion") != "failure":
        return True, ""
    kind, why = nightly_triage.classify(job)
    if kind == "no-verdict":
        return False, why
    steps = job.get("steps", [])
    idx = next((i for i, s in enumerate(steps) if s.get("conclusion") == "failure"), None)
    if idx is None:
        return False, "job failed with no failing step (runner/container)"
    later = [s.get("name", "") for s in steps[idx + 1:]
             if s.get("conclusion") == "skipped" and CELL_STEP.search(s.get("name", ""))]
    if later:
        return False, f"{steps[idx].get('name', '?')} (so `{later[0]}` never ran)"
    return True, ""


def run_has_verdict(run: dict, jobs: list[dict]) -> tuple[bool, list[str]]:
    """A run is a verdict when it is green, or every failed job reached its cells."""
    if run.get("conclusion") == "success":
        return True, []
    if run.get("conclusion") != "failure":
        return False, [f"run {run.get('conclusion') or run.get('status') or 'unfinished'}"]
    missing = []
    for j in jobs:
        ok, why = job_reached_cells(j)
        if not ok:
            missing.append(f"{j.get('name', '?')} -> {why}")
    return (not missing), missing


def no_verdict_days(runs: list[dict], jobs_of, as_of: str | None = None,
                    days: int = 2) -> dict:
    """Did the lane produce a verdict on any of its last `days` UTC days?

    `runs` newest first; `jobs_of(run)` returns that run's jobs. The window
    ends at `as_of` (a `YYYY-MM-DD`) or at the newest run's day, and a day in
    it with no run counts as a day with no verdict. Returns
    {"flag": bool, "days": [(day, verdict, detail)], ...}."""
    from datetime import date, timedelta
    finished = [r for r in runs if r.get("status", "completed") == "completed"]
    if as_of:
        finished = [r for r in finished if r["createdAt"][:10] <= as_of]
    if not finished:
        return {"flag": False, "days": []}
    end = date.fromisoformat(as_of or finished[0]["createdAt"][:10])
    window = [(end - timedelta(days=i)).isoformat() for i in range(days)]
    out = []
    for d in window:
        todays = [r for r in finished if r["createdAt"][:10] == d]
        if not todays:
            out.append((d, False, "no run"))
            continue
        detail = ""
        verdict = False
        for r in todays:
            ok, missing = run_has_verdict(r, jobs_of(r))
            if ok:
                verdict, detail = True, f"run {r.get('databaseId')}"
                break
            if not detail:
                detail = f"run {r.get('databaseId')}: " + "; ".join(missing[:3]) + (
                    f" (+{len(missing) - 3} more)" if len(missing) > 3 else "")
        out.append((d, verdict, detail))
    return {"flag": not any(v for _, v, _ in out), "days": out}


def triggers(text: str) -> set[str]:
    """The event names in a workflow's top-level `on:`. Text, not PyYAML:
    `automation.yml`'s `triage` job runs this on a bare `ubuntu-latest`, which promises
    no PyYAML, and the shape here is fixed (`on:` at column 0, events as keys
    at the next indent, or the inline `on: push` / `on: [a, b]` forms)."""
    lines = text.splitlines()
    for i, line in enumerate(lines):
        m = re.match(r"""^["']?on["']?\s*:\s*(.*?)\s*(#.*)?$""", line)
        if not m:
            continue
        inline = m.group(1)
        if inline:
            return {t.strip(" '\"") for t in inline.strip("[]").split(",") if t.strip()}
        found, indent = set(), None
        for nxt in lines[i + 1:]:
            if not nxt.strip() or nxt.lstrip().startswith("#"):
                continue
            lead = len(nxt) - len(nxt.lstrip())
            if lead == 0:
                break
            indent = lead if indent is None else indent
            k = re.match(r"\s*([A-Za-z_]+)\s*:", nxt)
            if lead == indent and k:
                found.add(k.group(1))
        return found
    return set()


def harvest(workflows_dir: pathlib.Path = WORKFLOWS) -> list[str]:
    """Every workflow file that runs on `main` with no pull request in front."""
    return [p.name for p in sorted(workflows_dir.glob("*.yml"))
            if triggers(p.read_text(encoding="utf-8")) & {"schedule", "push"}]


def assess(runs: list[dict], threshold: int) -> dict:
    """Classify one lane from its runs, NEWEST FIRST. Pure — the self-test's subject."""
    if not runs:
        return {"state": "no runs", "streak": 0, "verdicts": 0, "inconclusive": 0,
                "last_green": None, "newest_red": None}
    verdicts = [r for r in runs if r.get("conclusion") in VERDICTS]
    inconclusive = len(runs) - len(verdicts)
    last_green = next((r["createdAt"][:10] for r in verdicts
                       if r["conclusion"] == "success"), None)
    if not verdicts:
        return {"state": "NEVER FINISHES", "streak": 0, "verdicts": 0,
                "inconclusive": inconclusive, "last_green": None, "newest_red": None}
    streak = 0
    for r in verdicts:
        if r["conclusion"] != "failure":
            break
        streak += 1
    if streak >= threshold:
        state = "NO SIGNAL"
    elif len(runs) >= 5 and len(verdicts) * 10 < len(runs) * 3:
        # Fewer than 3 in 10 runs reached a verdict: what little it says is
        # about whichever commit happened to be measured, not about `main`.
        state = "RARELY FINISHES"
    elif streak == 0:
        state = "OK"
    else:
        state = "red"
    return {"state": state, "streak": streak, "verdicts": len(verdicts),
            "inconclusive": inconclusive, "last_green": last_green,
            "newest_red": verdicts[0] if streak else None}


def first_failure(jobs: list[dict]) -> str:
    """`job -> step` of the first failing job, as `nightly-triage` names it."""
    for j in jobs:
        if j.get("conclusion") != "failure":
            continue
        step = next((s.get("name", "?") for s in j.get("steps", [])
                     if s.get("conclusion") == "failure"), "(no failing step recorded)")
        return f"{j.get('name', '?')} -> {step}"
    return "(no failing job recorded)"


def gh_json(args: list[str]):
    try:
        out = subprocess.run(["gh", *args], capture_output=True, text=True, timeout=180)
    except (OSError, subprocess.TimeoutExpired) as exc:
        print(f"[WARN] gh failed: {exc}", file=sys.stderr)
        return None
    if out.returncode != 0:
        print(f"[WARN] gh exited {out.returncode}: {out.stderr.strip()[:200]}", file=sys.stderr)
        return None
    try:
        return json.loads(out.stdout)
    except json.JSONDecodeError:
        return None


def lane_runs(workflow: str, n: int) -> list[dict] | None:
    # Over-fetch: a lane with PR or merge-group events interleaves them, and
    # those are not what this asks about.
    runs = gh_json(["run", "list", "--repo", REPO, "--workflow", workflow,
                    "--branch", "main", "--limit", str(n * 4),
                    "--json", "databaseId,event,conclusion,status,createdAt"])
    if runs is None:
        return None
    return [r for r in runs if r.get("event") in EVENTS][:n]


def selftest(verbose: bool = False) -> int:
    fails = 0

    def chk(desc, cond):
        nonlocal fails
        if verbose or not cond:
            print(f"  {'ok   ' if cond else 'FAIL '} {desc}")
        fails += 0 if cond else 1

    def r(c, d="2026-10-03"):
        return {"conclusion": c, "createdAt": d + "T00:00:00Z", "databaseId": 1}

    chk("no runs is `no runs`, not a failure", assess([], 3)["state"] == "no runs")
    chk("a green newest verdict is OK", assess([r("success"), r("failure")], 3)["state"] == "OK")
    chk("one red under the threshold is `red`",
        assess([r("failure"), r("success")], 3)["state"] == "red")
    a = assess([r("failure"), r("cancelled"), r("failure"), r("failure"),
                r("success", "2026-09-01")], 3)
    chk("three reds with a cancellation between them is NO SIGNAL (cancelled is skipped, "
        "not a reset)", a["state"] == "NO SIGNAL" and a["streak"] == 3)
    chk("...and the last green is reported", a["last_green"] == "2026-09-01")
    # host-suite' measured shape.
    chk("all-cancelled is NEVER FINISHES, not OK and not `no runs`",
        assess([r("cancelled")] * 5 + [r(None)], 3)["state"] == "NEVER FINISHES")
    chk("a success reached through cancellations still counts",
        assess([r("cancelled"), r("success")], 3)["state"] == "OK")
    chk("1 verdict in 10 runs is RARELY FINISHES even when that verdict is red",
        assess([r("failure")] + [r("cancelled")] * 9, 3)["state"] == "RARELY FINISHES")
    chk("...and even when it is green",
        assess([r("cancelled")] * 9 + [r("success")], 3)["state"] == "RARELY FINISHES")
    chk("3 verdicts in 10 is enough to be read as OK",
        assess([r("success")] * 3 + [r("cancelled")] * 7, 3)["state"] == "OK")
    chk("first_failure names the job and its failing step",
        first_failure([{"name": "a", "conclusion": "success", "steps": []},
                       {"name": "check", "conclusion": "failure",
                        "steps": [{"name": "Checkout", "conclusion": "success"},
                                  {"name": "just check fast", "conclusion": "failure"}]}])
        == "check -> just check fast")
    # The harvest is the one part that touches the tree, so prove it reads the
    # real directory AND rejects a PR-only workflow, rather than asserting it.
    chk("triggers reads a block `on:` with comments and nested filters",
        triggers("name: x\non:\n  # why\n  push:\n    branches: [main]\n"
                 "    paths:\n      - a\n  schedule:\n    - cron: '0 3 * * *'\n"
                 "  workflow_dispatch:\n\njobs:\n  a:\n")
        == {"push", "schedule", "workflow_dispatch"})
    chk("triggers reads the inline forms",
        triggers("on: push\n") == {"push"}
        and triggers("on: [push, pull_request]\n") == {"push", "pull_request"})
    lanes = harvest()
    chk("the harvest finds gate.yml (push)", "gate.yml" in lanes)
    chk("the harvest leaves out queue.yml (merge_group only)", "queue.yml" not in lanes)
    # No workflow_run-only file is left in the tree (phase-559 W8 folded
    # queue-notify.yml and nightly-report.yml into automation.yml, which also
    # has a schedule), so the exclusion is checked on text.
    chk("a workflow_run-only workflow is not a lane",
        not triggers("on:\n  workflow_run:\n    workflows: [gate]\n"
                     "    types: [completed]\n\njobs:\n  a:\n") & {"schedule", "push"})
    # phase-591 W8 -- cells, not colour.
    def job(name, concl, steps):
        return {"name": name, "conclusion": concl,
                "steps": [{"name": n, "conclusion": c} for n, c in steps]}
    fixture_red = job("ThreadxLinux", "failure",
                      [("Checkout", "success"), ("Build (ThreadxLinux)", "failure"),
                       ("Test / e2e (ThreadxLinux)", "skipped"), ("Upload logs", "success")])
    cell_red = job("ThreadxLinux", "failure",
                   [("Build (ThreadxLinux)", "success"), ("Test / e2e (ThreadxLinux)", "failure")])
    build_lane_red = job("check build", "failure",
                         [("just check build", "failure"), ("Disk report", "success")])
    setup_red = job("zephyr", "failure", [("Set up Zephyr 3.7 workspace", "failure"),
                                          ("just test zephyr", "skipped")])
    chk("a fixture build before a skipped test step reached no cell",
        job_reached_cells(fixture_red)[0] is False
        and "Test / e2e" in job_reached_cells(fixture_red)[1])
    chk("a red test step is a verdict", job_reached_cells(cell_red)[0] is True)
    chk("a build step with no test after it IS the verdict (a build lane)",
        job_reached_cells(build_lane_red)[0] is True)
    chk("a setup failure reached no cell (nightly-triage's classifier)",
        job_reached_cells(setup_red)[0] is False)
    chk("`run` inside `runner` is not a cell step",
        not CELL_STEP.search("Report runner hardware"))
    rn = lambda i, c, d: {"databaseId": i, "conclusion": c, "status": "completed",
                          "createdAt": d + "T03:00:00Z"}
    jobs = {1: [fixture_red], 2: [fixture_red], 3: [cell_red], 4: [fixture_red]}
    nv = no_verdict_days([rn(2, "failure", "2026-11-23"), rn(1, "failure", "2026-11-22")],
                         lambda r: jobs[r["databaseId"]])
    chk("two days of fixture reds is NO VERDICT 2d", nv["flag"] and len(nv["days"]) == 2)
    nv = no_verdict_days([rn(2, "failure", "2026-11-23"), rn(3, "failure", "2026-11-22")],
                         lambda r: jobs[r["databaseId"]])
    chk("a red that ran its cells the day before is a verdict", not nv["flag"])
    nv = no_verdict_days([rn(2, "failure", "2026-11-23"), rn(1, "failure", "2026-11-21")],
                         lambda r: jobs[r["databaseId"]])
    chk("a day with no run is a day with no verdict", nv["flag"]
        and nv["days"][1] == ("2026-11-22", False, "no run"))
    nv = no_verdict_days([rn(4, "failure", "2026-11-23"), rn(5, "success", "2026-11-23"),
                          rn(1, "failure", "2026-11-22")],
                         lambda r: jobs.get(r["databaseId"], []))
    chk("a green dispatch on the same day is that day's verdict", not nv["flag"])
    nv = no_verdict_days([rn(2, "failure", "2026-11-23"), rn(3, "failure", "2026-11-22")],
                         lambda r: jobs[r["databaseId"]], as_of="2026-11-22")
    chk("--as-of judges the window ending that day",
        [d for d, _, _ in nv["days"]] == ["2026-11-22", "2026-11-21"])
    return fails


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--runs", type=int, default=10, help="recent runs per lane")
    ap.add_argument("--threshold", type=int, default=3,
                    help="consecutive red verdicts that make a lane NO SIGNAL")
    ap.add_argument("--markdown", action="store_true")
    ap.add_argument("--as-of", metavar="YYYY-MM-DD",
                    help="judge the lanes as of the end of this UTC day (replay)")
    ap.add_argument("--lane", action="append", default=[],
                    help="only this workflow file (repeatable)")
    ap.add_argument("--selftest", action="store_true")
    args = ap.parse_args()

    if args.selftest:
        bad = selftest(verbose=True)
        print("lane-health self-test:", "OK" if not bad else f"{bad} FAILED")
        return 1 if bad else 0
    if selftest():
        print("lane-health: self-test FAILED — not reporting from a broken classifier",
              file=sys.stderr)
        return 1

    rows = []
    job_cache: dict = {}

    def jobs_of(run):
        rid = run["databaseId"]
        if rid not in job_cache:
            job_cache[rid] = (gh_json(["run", "view", str(rid), "--repo", REPO,
                                       "--json", "jobs"]) or {}).get("jobs", [])
        return job_cache[rid]

    scheduled = {p.name for p in WORKFLOWS.glob("*.yml")
                 if "schedule" in triggers(p.read_text(encoding="utf-8"))}
    nv_lanes = []
    for wf in harvest():
        if args.lane and wf not in args.lane:
            continue
        # Over-fetch when replaying, so the window before --as-of is still there.
        runs = lane_runs(wf, args.runs * (3 if args.as_of else 1))
        if runs is None:
            rows.append((wf, "unreadable", "", "", "", "`gh` could not list runs"))
            continue
        if args.as_of:
            runs = [r for r in runs if r["createdAt"][:10] <= args.as_of][:args.runs]
        a = assess(runs, args.threshold)
        nv = no_verdict_days(runs, jobs_of, args.as_of) if wf in scheduled else {"flag": False}
        if nv["flag"]:
            a["state"] = "NO VERDICT 2d"
            nv_lanes.append((wf, nv["days"]))
        where = ""
        if a["newest_red"]:
            data = gh_json(["run", "view", str(a["newest_red"]["databaseId"]),
                            "--repo", REPO, "--json", "jobs"])
            where = first_failure((data or {}).get("jobs", []))
        rows.append((wf, a["state"],
                     str(a["streak"]) if a["streak"] else "",
                     f"{a['verdicts']}/{len(runs)}",
                     a["last_green"] or ("never in window" if runs else ""),
                     where))

    order = {"NO VERDICT 2d": -1, "NO SIGNAL": 0, "NEVER FINISHES": 1, "RARELY FINISHES": 2, "unreadable": 3,
             "red": 4, "no runs": 5, "OK": 6}
    rows.sort(key=lambda row: (order.get(row[1], 9), row[0]))
    head = ("lane", "state", "red streak", "verdicts/runs", "last green", "newest red stopped at")
    if args.markdown:
        print("| " + " | ".join(head) + " |")
        print("|" + "---|" * len(head))
        for row in rows:
            print("| " + " | ".join(f"`{row[0]}`" if i == 0 else row[i].replace("|", "\\|")
                                    for i in range(len(row))) + " |")
    else:
        for row in [head, *rows]:
            print(f"{row[0]:<22} {row[1]:<16} {row[2]:<10} {row[3]:<13} {row[4]:<15} {row[5]}")
    if rows and all(row[1] == "unreadable" for row in rows):
        # Every lane unreadable is the TOOL failing (no `gh`, no auth), and
        # "0 of N produce no signal" over it would be a false all-clear.
        print("lane-health: could not read ANY lane — `gh` missing or unauthenticated",
              file=sys.stderr)
        return 1
    if nv_lanes:
        print()
        print(f"lane-health: {len(nv_lanes)} scheduled lane(s) with NO VERDICT for 2 days"
              + (f" (as of {args.as_of})" if args.as_of else "") + ": "
              + ", ".join(wf for wf, _ in nv_lanes) + ".")
        for wf, days in nv_lanes:
            for d, _, detail in days:
                print(f"  {wf} {d}: {detail}")
        print("No cell ran on either day, so a new regression in that lane is invisible\n"
              "behind the old one (phase-591 W8). Fix what stops it before its cells.")
    silent = [row[0] for row in rows
              if row[1] in ("NO VERDICT 2d", "NO SIGNAL", "NEVER FINISHES", "RARELY FINISHES")]
    print()
    print(f"lane-health: {len(silent)} of {len(rows)} lane(s) on main produce no signal"
          + (f": {', '.join(silent)}" if silent else "") + ".")
    if silent:
        print("A lane red on every run, or one that never finishes, cannot report a new\n"
              "regression — it looks exactly like yesterday. Fix the LANE first.")
    return 0


if __name__ == "__main__":
    sys.exit(main())

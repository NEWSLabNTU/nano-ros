#!/usr/bin/env python3
"""Which lanes on `main` are producing a signal, and which have stopped?

`nightly-triage` answers "did the nightly report?" for ONE workflow, and
`tier-health` answers "does each platform's evidence support its tier?". Neither
asks the question that let `gate.yml`'s push lane sit red for 100 consecutive
runs (issue 1345) and `host-tests.yml` complete 3 of 29 (2026-10-03): across
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
ALL inconclusive is reported too, as NEVER FINISHES — that is `host-tests`'
shape, and it is exactly as silent as a standing red.

STATES

  OK              newest verdict is green
  red             newest verdict is red, streak below the threshold
  NO SIGNAL       red on the last N verdicts (default 3) — fix the lane
  NEVER FINISHES  runs in the window, none conclusive — fix the trigger
  RARELY FINISHES fewer than 3 in 10 runs (of 5+) conclusive — host-tests'
                  measured 3-of-29; what it says is about whichever commit
                  happened to finish, not about `main`
  no runs         nothing on main in the window (dispatch-only, or new)

For each red lane it names the first failing JOB and STEP of the newest red run,
because a lane stopping at a setup step and a lane failing its tests want
different people.

IT REPORTS. IT DOES NOT GATE. Exit 0 on any lane state; non-zero only when the
TOOL is broken (its self-test, an unreadable workflow). A reporter that can go
red about lanes being red is a second thing to ignore.

Usage::

    lane-health.py                    # every harvested lane, last 10 runs each
    lane-health.py --runs 20 --threshold 5
    lane-health.py --markdown         # the table nightly-report.yml writes
    lane-health.py --selftest
"""

from __future__ import annotations

import argparse
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


def triggers(text: str) -> set[str]:
    """The event names in a workflow's top-level `on:`. Text, not PyYAML:
    `nightly-report.yml` runs this on a bare `ubuntu-latest`, which promises
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
    # host-tests' measured shape.
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
    chk("the harvest finds gate.yml (push + schedule)", "gate.yml" in lanes)
    chk("the harvest leaves out queue.yml (merge_group only)", "queue.yml" not in lanes)
    chk("the harvest leaves out queue-notify.yml (workflow_run only)",
        "queue-notify.yml" not in lanes)
    return fails


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--runs", type=int, default=10, help="recent runs per lane")
    ap.add_argument("--threshold", type=int, default=3,
                    help="consecutive red verdicts that make a lane NO SIGNAL")
    ap.add_argument("--markdown", action="store_true")
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
    for wf in harvest():
        runs = lane_runs(wf, args.runs)
        if runs is None:
            rows.append((wf, "unreadable", "", "", "", "`gh` could not list runs"))
            continue
        a = assess(runs, args.threshold)
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

    order = {"NO SIGNAL": 0, "NEVER FINISHES": 1, "RARELY FINISHES": 2, "unreadable": 3,
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
    silent = [row[0] for row in rows
              if row[1] in ("NO SIGNAL", "NEVER FINISHES", "RARELY FINISHES")]
    print()
    print(f"lane-health: {len(silent)} of {len(rows)} lane(s) on main produce no signal"
          + (f": {', '.join(silent)}" if silent else "") + ".")
    if silent:
        print("A lane red on every run, or one that never finishes, cannot report a new\n"
              "regression — it looks exactly like yesterday. Fix the LANE first.")
    return 0


if __name__ == "__main__":
    sys.exit(main())

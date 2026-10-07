#!/usr/bin/env python3
"""Which STAGE did a staged lane reach? — issue 1158, item 3.

## The defect

`run-matrix.yml` is the only automated path to tier 2's runtime cells. Its last
eight runs all say the same word — `failure` — and they hide FOUR different
stories:

    2026-09-01/02/03   died in `Verify this runner's labels are true`
    2026-09-04/05      died in `just setup tier2`
    2026-09-06 x3      died in `just build tier2`

Not one of them is a test result. Every failure is UPSTREAM of the cells, so the
lane produced no runtime verdict on any of those days — and a reader of the run
list cannot tell that from a reader of a run that ran its cells and found a real
regression. CLAUDE.md states the rule:

  > A red CI lane answers one of two questions and they look identical — the
  > lane RAN and the code is broken (a verdict), or it never ran (no verdict). A
  > uniformly-red lane has NO signal capacity.

This is the same shape issue 1043 fixed one layer down, for
`check-submodule-pins`: where there were two outcomes (OK / FAIL) there are
really three, and the third is "this could not be evaluated here". A lane needs
the same three:

    VERDICT     the cells RAN. Green or red, the answer is about the code.
    NO VERDICT  the lane stopped before the cells. The answer is about the lane.
    DID NOT RUN the job never started (interlock off, no runner).

## Why not a new mechanism

`nightly-triage.py` already classifies a failing job as verdict / no-verdict by
its failing STEP, and `queue-triage.sh` does the INFRA / MINE split for the
merge queue. This is the same question a third time, so it reuses their shape
and their hard-won lessons rather than inventing a fourth:

  * WORD BOUNDARIES, not substrings. `run` matches inside `runner`, which made
    `Verify this runner's labels are true` — the single most common failure in
    this very lane — score as a real failure. That is recorded in
    `nightly-triage.py`; repeating the mistake here would be a choice.
  * The FIRST failing step decides. Later steps fail because the first did.
  * It REPORTS, never gates. Exit status is always 0. A reporter that can redden
    a run has an opinion about the lane it was built to describe.

What is new here is the third axis nightly-triage does not have. Its question is
binary (was this failure a verdict?); this lane has an ORDERED pipeline —
provisioning -> build -> cells — and "how far did it get" is the thing a reader
needs. `just build tier2` failing is a real verdict about the code AND still not
the runtime verdict this lane exists to produce, and only a stage model can say
both of those at once.

## One classifier, two callers

`classify()` takes the same shape from both directions:

  * in the workflow, `steps.<id>.outcome` for each staged step (`--report`);
  * after the fact, `gh run view --json jobs` (`--history`).

Those two vocabularies are identical — `success` / `failure` / `skipped` /
`cancelled` — which is what makes the historical validation evidence about the
live path and not merely a related test. Every run in `HISTORICAL` below is a
real run of this lane, recorded from `gh`, and asserted in the self-test.

Usage::

    lane-stage.py --report --lane "tier 2 (1-wise matrix)"   # in-workflow
    lane-stage.py --history [--runs 8] [--workflow run-matrix.yml]
    lane-stage.py --selftest
"""

import argparse
import collections
import json
import os
import re
import subprocess
import sys

sys.path.insert(0, os.path.join(os.path.dirname(os.path.dirname(
    os.path.abspath(__file__))), "lib"))
import lane_step_markers  # noqa: E402  (issue 1754 — one parser, shared)

REPO = os.environ.get("NROS_QUEUE_REPO", "NEWSLabNTU/nano-ros")

# The lane's pipeline, in order. A stage is reached only by finishing the one
# before it, which is what makes "how far did it get" a single number.
PROVISIONING, BUILD, CELLS = "provisioning", "build", "cells"
STAGE_ORDER = (PROVISIONING, BUILD, CELLS)

# Step name -> stage. Matched with WORD BOUNDARIES against the lowercased name,
# most specific stage first, because a step that names the thing under test
# belongs to that stage even when it also prepares something.
#
# `run` is deliberately absent from every list: it is a substring of `runner`,
# and `Verify this runner's labels are true` is the most frequent failure in
# this lane. nightly-triage.py records what that cost there.
STAGE_MARKERS = (
    # The cells: the runtime verdict this lane exists to produce.
    (CELLS, ("ci", "matrix", "test", "e2e", "cell", "cells")),
    # The build: real code failures, but not the runtime verdict.
    (BUILD, ("build",)),
    # Everything that has to be true before the lane can build anything.
    # `ledger` is live-peer.yml's: that lane reads `.config/interop-verdicts.toml`
    # before it runs anything, and a lane that cannot read its own membership
    # has not reached a stage.
    #
    # `submodules` and `check out` are here beside their singular/closed
    # spellings because the matcher uses WORD BOUNDARIES: `\bsubmodule\b` does
    # not match "submodules", and `\bcheckout\b` does not match "check out".
    # A step named `Check out the submodules the fixtures vendor` therefore
    # matched NOTHING and fell out of every stage — silently, since an
    # unclassified step is simply not a stage. Caught by the self-test when the
    # step was added; the same boundary rule that keeps `run` out of `runner`
    # needs every inflection spelled.
    (PROVISIONING, ("setup", "set up", "provision", "install", "checkout",
                    "check out", "cache", "fetch", "submodule", "submodules",
                    "labels", "doctor", "ledger", "reclaim disk", "free disk",
                    "apt", "rustup", "register")),
)

# Steps that belong to no stage: housekeeping that runs `if: always()` after the
# lane has already decided. Counting these as a stage would let a successful
# `Sweep orphans and disk` claim the lane got somewhere.
NOT_A_STAGE = ("upload", "sweep", "complete job", "post ")

LABELS = {
    "verdict-pass": "VERDICT: cells ran and passed",
    "verdict-fail": "VERDICT: cells ran and FAILED",
    "no-verdict-provisioning": "NO VERDICT: stopped in provisioning",
    "no-verdict-build": "NO VERDICT: stopped in the build",
    # phase-441 W4. The cells STEP ran and failed, and the runner it invoked
    # said the cells themselves did not produce results — a fixture that would
    # not build, a peer that is not installed, a membership that only skipped.
    # Without this, a step outcome is the whole story and every such run reads
    # as "a recorded-passing cell regressed", which is the one thing the
    # live-peer lane exists to say.
    "no-verdict-cells": "NO VERDICT: the cells could not run",
    # issue 1754. The cells STEP is `just ci <lane>`, and its first inner step
    # is the `check::default` preflight: run 37685900447 failed there (one
    # `check fast` gate), ran 0 cells, and read `cells ran and FAILED`. The
    # runner's own `<== … FAILED` marker on a non-cell step, with no cell step
    # started, is the positive evidence this label needs.
    "no-verdict-preflight": "NO VERDICT: preflight failed before any cell",
    "no-verdict-none": "NO VERDICT: died before any stage",
    "cancelled": "NO VERDICT: cancelled",
    "running": "still running",
    "did-not-start": "DID NOT START",
}


def stage_of(step_name):
    """The stage a step belongs to, or None if it belongs to no stage."""
    low = (step_name or "").lower()
    if any(marker in low for marker in NOT_A_STAGE):
        return None
    for stage, markers in STAGE_MARKERS:
        for m in markers:
            if re.search(rf"\b{re.escape(m)}\b", low):
                return stage
    return None


Result = collections.namedtuple(
    "Result", "kind stage label failing_step reached_cells")


PREFLIGHT = "preflight"


def classify(steps, job_conclusion=None, cells_ran=None, inner=None):
    """Classify one job's ordered steps.

    `steps` is [{"name": str, "conclusion": str}] — the shape `gh run view
    --json jobs` returns AND the shape the workflow reports from
    `steps.<id>.outcome`. `job_conclusion` is optional and only used to tell a
    job that died with no failing step from one that never ran.

    `cells_ran` is the FOURTH axis, and only the in-workflow half can supply it
    (phase-441 W4): a step outcome says the cells step failed, never whether the
    cells produced results. `just native test-live-peer-regression` already
    knows — it answers 2 for "this lane could not run" and 1 for a real
    regression — and the workflow forwards that as `NROS_LANE_CELLS_RAN`.
    `None` means nobody said, which is what `--history` has for every past run,
    so the classification there is unchanged.

    `inner` is the FIFTH (issue 1754): a `lane_step_markers.Reached` read from
    the step runner's markers INSIDE the cells step. Its `cells_ran is False`
    means a non-cell inner step (the `check::default` preflight) reported
    FAILED before any cell started.

    Returns a Result whose `kind` is one of:
        verdict-pass  verdict-fail  no-verdict  cancelled  running
    """
    staged = [(s, stage_of(s.get("name", ""))) for s in steps]
    concl = {}
    for s, st in staged:
        if st is None:
            continue
        # First conclusion wins: a stage is decided by the step that decides it,
        # and a later `Post Run actions/checkout` must not overwrite it.
        concl.setdefault(st, s.get("conclusion"))

    failed = [s for s, st in staged if s.get("conclusion") == "failure"]
    first_failing = failed[0].get("name", "") if failed else ""

    cells = concl.get(CELLS)
    if cells == "success":
        return Result("verdict-pass", CELLS, LABELS["verdict-pass"], "", True)
    if cells == "failure":
        if cells_ran is False:
            return Result("no-verdict", CELLS, LABELS["no-verdict-cells"],
                          first_failing, False)
        if inner is not None and inner.cells_ran is False:
            return Result("no-verdict", PREFLIGHT,
                          LABELS["no-verdict-preflight"],
                          f"{first_failing} -> {inner.failed_step}", False)
        return Result("verdict-fail", CELLS, LABELS["verdict-fail"],
                      first_failing, True)

    # The cells did not run. How far DID it get? The stage that failed, or — if
    # nothing failed — the last stage that succeeded.
    if failed:
        stopped = stage_of(first_failing)
    else:
        reached = [st for st in STAGE_ORDER if concl.get(st) == "success"]
        stopped = reached[-1] if reached else None

    if job_conclusion == "cancelled" or any(
            s.get("conclusion") == "cancelled" for s, _ in staged):
        return Result("cancelled", stopped, LABELS["cancelled"],
                      first_failing, False)

    if not failed and job_conclusion not in ("failure", "success", None):
        return Result("running", stopped, LABELS["running"], "", False)

    key = {PROVISIONING: "no-verdict-provisioning",
           BUILD: "no-verdict-build"}.get(stopped, "no-verdict-none")
    return Result("no-verdict", stopped, LABELS[key], first_failing, False)


# --------------------------------------------------------------------------
# --report: the in-workflow half.
# --------------------------------------------------------------------------

def _read_step_record():
    """The step runner's markers from `$NROS_LANE_STEP_RECORD` (issue 1754).

    Unset or unreadable is `None` — nobody said, the old classification holds.
    """
    path = os.environ.get("NROS_LANE_STEP_RECORD", "").strip()
    if not path:
        return None
    try:
        with open(path, encoding="utf-8") as fh:
            return lane_step_markers.reached_cells(fh.read())
    except OSError:
        return None


def _job_log_failed(job):
    """One job's failed-step log via `gh`, or '' — for `--history`."""
    jid = job.get("databaseId")
    if not jid:
        return ""
    try:
        out = subprocess.run(["gh", "run", "view", "--repo", REPO, "--job",
                              str(jid), "--log-failed"], capture_output=True,
                             text=True, timeout=180)
    except (OSError, subprocess.TimeoutExpired):
        return ""
    return out.stdout if out.returncode == 0 else ""


def report(lane, steps_json):
    try:
        steps = json.loads(steps_json)
    except (json.JSONDecodeError, TypeError) as exc:
        print(f"[WARN] lane-stage: NROS_LANE_STEPS unreadable ({exc}); "
              "reporting nothing", file=sys.stderr)
        return 0

    # A step that never ran reports an empty outcome, not `skipped`.
    steps = [{"name": s.get("name", "?"), "conclusion": s.get("conclusion") or "skipped"}
             for s in steps]
    # An UNSET variable is `None`, not False: "nobody said" and "the runner said
    # the cells did not run" are different answers, and only the second may turn
    # a red into a no-verdict.
    said = os.environ.get("NROS_LANE_CELLS_RAN", "").strip().lower()
    cells_ran = {"true": True, "false": False}.get(said)
    res = classify(steps, cells_ran=cells_ran, inner=_read_step_record())

    out = [f"{lane}: {res.label}"]
    if res.failing_step:
        out.append(f"  first failing step: {res.failing_step}")
    for s in steps:
        mark = {"success": "ok  ", "failure": "FAIL", "skipped": "----",
                "cancelled": "canc"}.get(s["conclusion"], "?   ")
        out.append(f"  [{mark}] {stage_of(s['name']) or '-':<12} {s['name']}")
    if not res.reached_cells:
        out.append("")
        out.append("  This run answers NOTHING about the code under test. The lane")
        out.append("  stopped before its cells, so a regression landing today would")
        out.append("  look exactly like this. On `run-matrix.yml` that is how issues")
        out.append("  1075/1098/1104/1114 rode in (issue 1158); this reporter exists")
        out.append("  so no lane has to learn it a second time.")
    print("\n".join(out))

    # An annotation, because it is the one thing visible on the run page and in
    # `gh run view` WITHOUT opening a log. `::notice` when the lane reported.
    level = "notice" if res.reached_cells else "warning"
    title = f"{lane}: {res.label}"
    detail = (f"first failing step: {res.failing_step}" if res.failing_step
              else "no step failed")
    print(f"::{level} title={title}::{detail}")

    summary = os.environ.get("GITHUB_STEP_SUMMARY")
    if summary:
        with open(summary, "a", encoding="utf-8") as fh:
            fh.write(f"### {lane} — {res.label}\n\n")
            fh.write("| stage | step | outcome |\n| --- | --- | --- |\n")
            for s in steps:
                fh.write(f"| {stage_of(s['name']) or '—'} | `{s['name']}` "
                         f"| {s['conclusion']} |\n")
            fh.write("\n")
            if res.reached_cells:
                fh.write("The cells ran, so this run **is** a verdict about the "
                         "code.\n")
            else:
                fh.write("**The cells did not run**, so this run is not a verdict "
                         "about the code — it is a verdict about the lane. See "
                         "issue 1158.\n")

    gh_out = os.environ.get("GITHUB_OUTPUT")
    if gh_out:
        with open(gh_out, "a", encoding="utf-8") as fh:
            fh.write(f"stage={res.stage or 'none'}\n")
            fh.write(f"kind={res.kind}\n")
            fh.write(f"label={res.label}\n")
            fh.write(f"reached_cells={'true' if res.reached_cells else 'false'}\n")
    # ALWAYS 0. This describes the lane; the lane's own steps colour the run.
    return 0


# --------------------------------------------------------------------------
# --history: the run-list half.
# --------------------------------------------------------------------------

def gh_json(args):
    try:
        out = subprocess.run(["gh"] + args, capture_output=True, text=True,
                             timeout=180)
    except (OSError, subprocess.TimeoutExpired) as exc:
        print(f"[WARN] gh failed: {exc}", file=sys.stderr)
        return None
    if out.returncode != 0:
        print(f"[WARN] gh exited {out.returncode}: {out.stderr.strip()[:200]}",
              file=sys.stderr)
        return None
    try:
        return json.loads(out.stdout)
    except json.JSONDecodeError:
        return None


def history(workflow, want, lane_job=None):
    runs = gh_json(["run", "list", "--repo", REPO, "--workflow", workflow,
                    "--limit", str(want), "--json",
                    "databaseId,createdAt,event,status,conclusion"])
    if runs is None:
        print(f"no runs readable for {workflow} (is `gh` authenticated?)")
        return 0
    if not runs:
        print(f"{workflow}: no runs")
        return 0

    print(f"== {workflow} — last {len(runs)} run(s) ==\n")
    kinds = collections.Counter()
    for run in runs:
        data = gh_json(["run", "view", str(run["databaseId"]), "--repo", REPO,
                        "--json", "jobs"])
        jobs = (data or {}).get("jobs", [])
        # The lane job is the one with a stage-bearing step; the coverage job
        # that reports on it has none.
        lane_jobs = [j for j in jobs
                     if (lane_job is None or j.get("name") == lane_job)
                     and any(stage_of(s.get("name", "")) for s in j.get("steps", []))]
        if not lane_jobs:
            res = Result("no-verdict", None, LABELS["did-not-start"], "", False)
        else:
            j = lane_jobs[0]
            res = classify(j.get("steps", []), j.get("conclusion"))
            # issue 1754 — a red cells step is only a verdict if a cell RAN;
            # the runner's markers in the log say whether one did.
            if res.kind == "verdict-fail":
                res = classify(j.get("steps", []), j.get("conclusion"),
                               inner=lane_step_markers.reached_cells(
                                   _job_log_failed(j)))
        if run.get("status") != "completed" and not res.failing_step:
            res = res._replace(kind="running", label=LABELS["running"])
        kinds[res.kind] += 1
        print(f"  {run['databaseId']}  {run['createdAt'][:16].replace('T', ' ')}"
              f"  {run.get('event', '?'):<18} {(run.get('conclusion') or run.get('status') or '?'):<9}"
              f" {res.label}")
        if res.failing_step:
            print(f"{'':>58}   at: {res.failing_step}")

    verdicts = kinds["verdict-pass"] + kinds["verdict-fail"]
    total = sum(kinds.values())
    print()
    print(f"  {verdicts} of {total} run(s) reached the cells and produced a VERDICT.")
    print(f"  {kinds['no-verdict']} of {total} run(s) produced NO VERDICT — the lane "
          "stopped before its cells.")
    if verdicts == 0 and total:
        print()
        print("  This lane has no signal capacity right now. A regression landing")
        print("  today is indistinguishable from yesterday's infrastructure red —")
        print("  'still red' and 'newly red' look the same in the run list.")
        print("  See issue 1158.")
    return 0


# --------------------------------------------------------------------------
# The self-test, over real recorded runs.
# --------------------------------------------------------------------------

def _steps(*pairs):
    return [{"name": n, "conclusion": c} for n, c in pairs]


# Every entry is a REAL run of `run-matrix.yml`, recorded from
# `gh run view <id> --json jobs` on 2026-09-07. These are the eight runs issue
# 1158 tabulates. Housekeeping steps are kept verbatim so the classifier is
# exercised on the noise it will actually see.
_TAIL = (("Upload junit and logs", "success"),
         ("Sweep orphans and disk", "success"),
         ("Post Run actions/checkout@v4", "success"),
         ("Complete job", "success"))

_DOCTOR_DIED = _steps(
    ("Set up job", "success"),
    ("Run actions/checkout@v4", "success"),
    ("Verify this runner's labels are true", "failure"),
    ("just setup tier2", "skipped"),
    ("just ci matrix", "skipped"),
    *_TAIL)

_SETUP_DIED = _steps(
    ("Set up job", "success"),
    ("Run actions/checkout@v4", "success"),
    ("just setup tier2", "failure"),
    ("Verify this runner's labels are true", "skipped"),
    ("just build tier2", "skipped"),
    ("just ci matrix", "skipped"),
    *_TAIL)

_BUILD_DIED = _steps(
    ("Set up job", "success"),
    ("Run actions/checkout@v4", "success"),
    ("just setup tier2", "success"),
    ("Verify this runner's labels are true", "success"),
    ("just build tier2", "failure"),
    ("just ci matrix", "skipped"),
    *_TAIL)

HISTORICAL = (
    # (run id, date, recorded steps, expected kind, expected stage)
    (33477831186, "2026-09-01", _DOCTOR_DIED, "no-verdict", PROVISIONING),
    (33599360758, "2026-09-02", _DOCTOR_DIED, "no-verdict", PROVISIONING),
    (33723333691, "2026-09-03", _DOCTOR_DIED, "no-verdict", PROVISIONING),
    (33844476113, "2026-09-04", _SETUP_DIED, "no-verdict", PROVISIONING),
    (33949734179, "2026-09-05", _SETUP_DIED, "no-verdict", PROVISIONING),
    (34000628352, "2026-09-06", _BUILD_DIED, "no-verdict", BUILD),
    (34007251082, "2026-09-06", _BUILD_DIED, "no-verdict", BUILD),
    (34016427482, "2026-09-06", _BUILD_DIED, "no-verdict", BUILD),
)


# The one run `live-peer.yml` has had, recorded from
# `gh run view 34186427723 --json jobs` on 2026-09-09. Verbatim, housekeeping
# included, and under the step names that run actually used — which is why the
# fixture step here still says `cells`: this is the evidence for the rename, not
# a copy of the tree after it.
_LIVE_PEER_34186427723 = _steps(
    ("Set up job", "success"),
    ("Initialize containers", "success"),
    ("Run actions/checkout@v4", "success"),
    ("Build the nros CLI", "success"),
    ("Report what the ledger claims, before running anything", "success"),
    ("Build the fixtures those cells resolve", "failure"),
    ("Run the cells with a recorded PASS", "skipped"),
    ("Ledger after the run", "success"),
    ("Post Run actions/checkout@v4", "success"),
    ("Stop containers", "success"),
    ("Complete job", "success"))

# Run 34300486133 (2026-09-09), the first dispatch after PR #780 fixed the silent
# abort. It got ONE STEP FURTHER and stopped on an honest precondition — the
# cyclonedds submodule was never checked out — which is the failure the
# `Check out the submodules the fixtures vendor` step was added for. Recorded
# under the step names of that run, i.e. WITHOUT that step.
_LIVE_PEER_34300486133 = _steps(
    ("Build the nros CLI", "success"),
    ("Report what the ledger claims, before running anything", "success"),
    ("Build the fixtures those rows resolve", "failure"),
    ("Run the cells with a recorded PASS", "skipped"))

# The same run as the workflow now reports it: four steps, current names.
_LIVE_PEER_FIXTURES_DIED = _steps(
    ("Build the nros CLI", "success"),
    ("Report what the ledger claims, before running anything", "success"),
    ("Build the fixtures those rows resolve", "failure"),
    ("Run the cells with a recorded PASS", "skipped"))


REPO_ROOT = os.path.dirname(
    os.path.dirname(os.path.dirname(os.path.abspath(__file__))))


def _workflow_path(basename):
    return os.path.join(REPO_ROOT, ".github", "workflows", basename)


# A staged lane, and the step->stage map it MUST produce. Asserted against the
# workflow itself, both directions — the map is AUTHORED (each workflow hands
# `--report` a copy of its own `- name:` lines), and an authored map drifts the
# moment a step is renamed, silently and in the safe-looking direction.
#
# `report_job` is the job whose NAME carries the answer, because a check-run
# name is the last piece of a run that is legible without opening a log.
LaneSpec = collections.namedtuple(
    "LaneSpec", "workflow job report_job expected_map")

LANES = (
    LaneSpec("run-matrix.yml", "matrix", "coverage", {
        "just setup tier2": PROVISIONING,
        "Verify this runner's labels are true": PROVISIONING,
        "just build tier2": BUILD,
        "just ci matrix": CELLS,
    }),
    # issues 1684/1685 — tier 1's cells, moved off host-tests to this runner.
    LaneSpec("run-matrix.yml", "tier1", "coverage-tier1", {
        "just setup tier1": PROVISIONING,
        "Verify this runner's labels are true": PROVISIONING,
        "just build tier1": BUILD,
        "just ci tier1 run": CELLS,
    }),
    # phase-433 W5. This lane already told "the run did not happen" from "a cell
    # regressed" — but only INSIDE the cell loop, by exit code. Everything above
    # the loop was a flat `failure`, which is the same ambiguity one step
    # upstream of the guard.
    LaneSpec("live-peer.yml", "regression", "stage", {
        "Build the nros CLI": BUILD,
        "Report what the ledger claims, before running anything": PROVISIONING,
        "Reclaim disk before the fixtures": PROVISIONING,
        "Check out the submodules the fixtures vendor": PROVISIONING,
        "Provision the declared system closure (phase-413 W3)": PROVISIONING,
        "Build the XRCE Agent against the sourced ROS (issue 0741)": BUILD,
        "Build the fixtures those rows resolve": BUILD,
        "Run the cells with a recorded PASS": CELLS,
    }),
    # phase-441 W4 — the same lane's board half, split out because these rows
    # need a board SDK and an emulator the host container does not carry. It is
    # a SEPARATE LaneSpec rather than more steps in `regression` for the reason
    # the split exists at all: "the board rows could not be built" and "the
    # board rows regressed" have to be different sentences, and a stage label
    # is per JOB.
    LaneSpec("live-peer.yml", "board", "stage-board", {
        "Build the nros CLI": BUILD,
        "Register the baked Zephyr SDK for this HOME": PROVISIONING,
        "Reclaim disk before the SDK setup": PROVISIONING,
        "Unblock the rustup clippy-preview conflict": PROVISIONING,
        "Provision the declared system closure (phase-413 W3)": PROVISIONING,
        "just setup the scopes those rows need": PROVISIONING,
        "Install clang + libclang for bindgen": PROVISIONING,
        "Build the fixtures those rows resolve": BUILD,
        "Provision cargo-nextest": PROVISIONING,
        "Run the board cells with a recorded PASS": CELLS,
    }),
)

# Kept for `--history`'s default and for anything importing the old name.
WORKFLOW = _workflow_path("run-matrix.yml")
EXPECTED_MAP = LANES[0].expected_map


def _workflow_consistency(chk):
    """Cross-check every lane's authored step->stage map against its workflow.

    Returns lines to print when a check could not be made — a REPORTED skip,
    never a silent one (issue 1043's shape: "could not evaluate" is a third
    answer, not a pass).
    """
    try:
        import yaml
    except ModuleNotFoundError:
        return ["[skip] lane-stage: PyYAML missing — the workflow "
                "consistency arm did NOT run for ANY lane"]

    notes = []
    for lane in LANES:
        path = _workflow_path(lane.workflow)
        if not os.path.exists(path):
            notes.append(f"[skip] lane-stage: {path} absent — the consistency "
                         f"arm did NOT run for {lane.workflow}")
            continue
        with open(path, encoding="utf-8") as fh:
            doc = yaml.safe_load(fh)
        notes.extend(_one_lane_consistency(chk, lane, doc))
    return notes


def _one_lane_consistency(chk, lane, doc):
    w = lane.workflow
    job = doc["jobs"][lane.job]
    steps = job["steps"]
    names = [s.get("name", "") for s in steps]
    # `--report` as well as the filename: a step that merely NAMES this script
    # is not this script's reporter, and one does — the board lane's setup step
    # tells the reader to add a scope here. Matching on the filename alone found
    # it, called the job's reporter ambiguous, and then died on the env of the
    # wrong step.
    reporter = [
        s for s in steps
        if "lane-stage.py" in str(s.get("run", ""))
        and "--report" in str(s.get("run", ""))
    ]

    chk(f"{w} has exactly one lane-stage reporter", len(reporter) == 1)
    if not reporter:
        return []

    declared = json.loads(
        re.sub(r"\$\{\{[^}]*\}\}", "success",
               reporter[0]["env"]["NROS_LANE_STEPS"]))
    declared_names = [d["name"] for d in declared]

    chk(f"{w}: every reported step name is a real step in `{lane.job}`",
        all(n in names for n in declared_names))
    chk(f"{w}: every staged step is reported",
        set(declared_names) == set(lane.expected_map))
    for n in declared_names:
        chk(f"{w}: `{n}` still classifies as {lane.expected_map.get(n)}",
            stage_of(n) == lane.expected_map.get(n))
    # issue 1754 — a cells step that is a `just ci <lane>` runs a preflight
    # first, so its outcome alone cannot say a cell ran. It must hand the
    # runner a record, and the reporter must read the same one.
    cells_steps = [s for s in steps
                   if lane.expected_map.get(s.get("name", "")) == CELLS]
    for cs in cells_steps:
        if re.search(r"\bjust ci\b", str(cs.get("run", ""))):
            mine = (cs.get("env") or {}).get("NROS_LANE_STEP_RECORD")
            chk(f"{w}: `{cs['name']}` sets NROS_LANE_STEP_RECORD in its env",
                bool(mine))
            chk(f"{w}: the `{lane.job}` reporter reads the SAME record",
                bool(mine) and mine ==
                (reporter[0].get("env") or {}).get("NROS_LANE_STEP_RECORD"))
    chk(f"{w}: the reporter runs `if: always()` — a stage report that is "
        "skipped when the lane dies is no report",
        "always()" in str(reporter[0].get("if", "")))
    chk(f"{w}: `{lane.job}` exports the stage label for the report job's name",
        "stage.outputs.label" in
        str(job.get("outputs", {}).get("stage_label", "")))
    chk(f"{w}: the `{lane.report_job}` job's NAME carries the stage",
        f"needs.{lane.job}.outputs.stage_label" in
        str(doc["jobs"][lane.report_job]["name"]))
    chk(f"{w}: the `{lane.report_job}` job runs `if: always()`, so it reports "
        "on a dead run too",
        "always()" in str(doc["jobs"][lane.report_job].get("if", "")))
    return []


# issue 1754 — the two marker lines that matter, verbatim from run
# 37685900447's job log (GitHub's job/step/timestamp prefix included, because
# that is what `--history` and nightly-triage parse).
RUN_37685900447_LOG = (
    "tier 2 (1-wise matrix)\tUNKNOWN STEP\t2026-10-07T21:51:44.2448718Z "
    "==> ci tier2 [1/4] check::default — started 21:51:44Z\n"
    "tier 2 (1-wise matrix)\tUNKNOWN STEP\t2026-10-07T21:53:57.0939106Z "
    "check-zephyr-workspace-foreign-checkout: FAILED (issue 1387)\n"
    "tier 2 (1-wise matrix)\tUNKNOWN STEP\t2026-10-07T21:53:57.1090374Z "
    "<== ci tier2 [1/4] check::default — FAILED after 2m13s (at 21:53:57Z)\n"
    "tier 2 (1-wise matrix)\tUNKNOWN STEP\t2026-10-07T21:53:57.1103136Z "
    "ci tier2 FAILED at step 1 of 4 (check::default).\n")


def _read_or_empty(path):
    try:
        with open(path, encoding="utf-8") as fh:
            return fh.read()
    except OSError:
        return ""


def _runner_marker_contract(chk):
    """Run the REAL `run-lane-steps.sh` against a stub `just` (issue 1754).

    The runner's marker format and `lane_step_markers` are one contract; a
    reworded marker would make every lane read `None` (nobody said) and the
    mislabel would return silently. So the producer is executed, not quoted.
    """
    import shutil
    import tempfile
    runner = os.path.join(REPO_ROOT, "scripts", "ci", "run-lane-steps.sh")
    if not shutil.which("bash") or not os.path.exists(runner):
        return ["[skip] lane-stage: bash or run-lane-steps.sh absent — the "
                "runner-marker contract arm did NOT run"]
    with tempfile.TemporaryDirectory() as tmp:
        stub = os.path.join(tmp, "just")
        with open(stub, "w", encoding="utf-8") as fh:
            # The preflight fails; a cell would succeed if it were reached.
            fh.write('#!/bin/sh\ncase "$1" in check::*) exit 1;; esac\nexit 0\n')
        os.chmod(stub, 0o755)
        record = os.path.join(tmp, "record")
        env = dict(os.environ, PATH=tmp + os.pathsep + os.environ.get("PATH", ""),
                   NROS_LANE_STEP_RECORD=record)
        rc = subprocess.run(["bash", runner, "tier2", "check::default",
                             "test-all"], env=env, capture_output=True,
                            text=True).returncode
        got = lane_step_markers.reached_cells(_read_or_empty(record))
        chk("run-lane-steps.sh: a failed preflight exits non-zero", rc != 0)
        chk("run-lane-steps.sh's RECORD says the preflight failed and no cell "
            "started", got.cells_ran is False
            and got.failed_step == "check::default")
        if os.path.exists(record):
            os.remove(record)
        subprocess.run(["bash", runner, "tier2", "rust-rtos-link-check",
                        "test-all"], env=env, capture_output=True)
        got = lane_step_markers.reached_cells(_read_or_empty(record))
        chk("run-lane-steps.sh's RECORD says a cell started", got.cells_ran is True)
    return []


def _ci_just_cell_steps(chk):
    """`CELL_STEPS` is authored; hold it against tier 2's real step array."""
    path = os.path.join(REPO_ROOT, "just", "ci.just")
    with open(path, encoding="utf-8") as fh:
        text = fh.read()
    m = re.search(r"^_matrix-run:.*?^\s*steps=\(([^)]*)\)", text, re.S | re.M)
    chk("just/ci.just: `_matrix-run` has a `steps=(…)` array", m is not None)
    if not m:
        return
    arr = m.group(1).split()
    chk("tier 2's FIRST inner step is a preflight, not a cell — so a red "
        "`just ci matrix` needs the markers to be read",
        arr and arr[0] not in lane_step_markers.CELL_STEPS)
    chk("tier 2's inner steps include a cell `lane_step_markers` recognises",
        any(a in lane_step_markers.CELL_STEPS for a in arr))


def selftest(verbose=False):
    ok = fail = 0

    def chk(desc, cond):
        nonlocal ok, fail
        if verbose or not cond:
            print(f"  {'ok   ' if cond else 'FAIL '} {desc}")
        ok += 1 if cond else 0
        fail += 0 if cond else 1

    # 1. The eight real runs issue 1158 tabulates. Not one is a verdict, and
    #    they stopped at TWO different stages under one flat `failure`.
    for rid, when, steps, want_kind, want_stage in HISTORICAL:
        res = classify(steps, "failure")
        chk(f"{rid} ({when}) -> {want_kind}/{want_stage}",
            res.kind == want_kind and res.stage == want_stage)
    chk("no historical run reached the cells",
        not any(classify(s, "failure").reached_cells for _, _, s, _, _ in HISTORICAL))
    chk("the eight runs split into TWO stages under one `failure` word",
        len({classify(s, "failure").stage for _, _, s, _, _ in HISTORICAL}) == 2)

    # 2. The distinction the whole file exists for.
    ran_and_failed = _steps(("just setup tier2", "success"),
                            ("Verify this runner's labels are true", "success"),
                            ("just build tier2", "success"),
                            ("just ci matrix", "failure"), *_TAIL)
    chk("cells that RAN and failed are a VERDICT, not a no-verdict",
        classify(ran_and_failed, "failure").kind == "verdict-fail")
    chk("...and it reports having reached the cells",
        classify(ran_and_failed, "failure").reached_cells)
    chk("a run whose cells passed is a verdict too",
        classify(_steps(("just setup tier2", "success"),
                        ("just build tier2", "success"),
                        ("just ci matrix", "success"), *_TAIL),
                 "success").kind == "verdict-pass")

    # 3. The word-boundary trap nightly-triage.py paid for. `run` is a
    #    substring of `runner`; if it were a marker, the single commonest
    #    failure of this lane would classify as a cells verdict — the exact
    #    opposite of the truth.
    chk("`Verify this runner's labels are true` is PROVISIONING, not cells",
        stage_of("Verify this runner's labels are true") == PROVISIONING)
    chk("`just ci matrix` is the cells stage",
        stage_of("just ci matrix") == CELLS)
    chk("`just build tier2` is the build stage",
        stage_of("just build tier2") == BUILD)
    chk("`just setup tier2` is provisioning",
        stage_of("just setup tier2") == PROVISIONING)

    # 4. Housekeeping must not claim a stage. `Sweep orphans and disk` runs
    #    `if: always()` and succeeds on every run including the dead ones.
    for noise in ("Upload junit and logs", "Sweep orphans and disk",
                  "Complete job", "Post Run actions/checkout@v4"):
        chk(f"`{noise}` belongs to no stage", stage_of(noise) is None)

    # 5. A job that died with no failing step tested nothing — the runner or the
    #    container went away. Same call nightly-triage makes.
    chk("a red job with no failing step is a no-verdict",
        classify(_steps(("Set up job", "success")), "failure").kind == "no-verdict")
    chk("...and names no stage rather than inventing one",
        classify([], "failure").label == LABELS["no-verdict-none"])

    # 6. Cancellation is neither a verdict nor a lane defect.
    chk("a cancelled run is reported as cancelled",
        classify(_steps(("just setup tier2", "success"),
                        ("just ci matrix", "cancelled")), "cancelled").kind
        == "cancelled")

    # 7. The FIRST failing step decides: later steps fail because it did.
    chk("the first failing step decides the stage",
        classify(_steps(("just setup tier2", "failure"),
                        ("just build tier2", "failure")), "failure").stage
        == PROVISIONING)

    # 8. An empty outcome (a step that never started) is not a success.
    chk("--report treats an empty outcome as skipped, never as reached",
        classify(_steps(("just setup tier2", "success"),
                        ("just build tier2", "success"),
                        ("just ci matrix", "skipped")), "failure").reached_cells
        is False)

    # 8b. live-peer.yml — phase-433 W5. The lane whose ledger says 20 of 20
    #     cells pass live, and whose single run reached ZERO of them.
    lp = classify(_LIVE_PEER_FIXTURES_DIED, "failure")
    chk("34186427723 (2026-09-08) -> no-verdict/build, not a cell regression",
        lp.kind == "no-verdict" and lp.stage == BUILD)
    chk("...and it names the step that actually stopped it",
        lp.failing_step == "Build the fixtures those rows resolve")
    chk("...and does not claim to have reached the cells",
        lp.reached_cells is False)

    # The rename is load-bearing, so the old spelling is pinned as the
    # misclassification it was. `cells` is the CELLS marker and the classifier
    # takes the most specific stage first, so under the old name a FIXTURE BUILD
    # failure reported as `VERDICT: cells ran and FAILED` — this lane saying the
    # one thing it exists to say, about a run that never started a cell.
    chk("the OLD step name misclassified a fixture build as the cells stage",
        stage_of("Build the fixtures those cells resolve") == CELLS)
    chk("...and the current name does not",
        stage_of("Build the fixtures those rows resolve") == BUILD)
    chk("the old name turned the real run into a false cell regression",
        classify(_LIVE_PEER_34186427723, "failure").kind == "verdict-fail")

    lp2 = classify(_LIVE_PEER_34300486133, "failure")
    chk("34300486133 (2026-09-09) -> no-verdict/build, the run that proved the "
        "stage report works",
        lp2.kind == "no-verdict" and lp2.stage == BUILD)
    chk("the submodule checkout step is provisioning, not a build",
        stage_of("Check out the submodules the fixtures vendor") == PROVISIONING)

    chk("`Run the cells with a recorded PASS` is the cells stage",
        stage_of("Run the cells with a recorded PASS") == CELLS)
    chk("`Report what the ledger claims, before running anything` is provisioning",
        stage_of("Report what the ledger claims, before running anything")
        == PROVISIONING)
    chk("live-peer cells that RAN and failed are a verdict",
        classify(_steps(
            ("Build the nros CLI", "success"),
            ("Report what the ledger claims, before running anything", "success"),
            ("Build the fixtures those rows resolve", "success"),
            ("Run the cells with a recorded PASS", "failure")),
            "failure").kind == "verdict-fail")

    # 8c. phase-441 W4 — the fourth axis. The cells STEP failing is the same
    #     word for "a recorded-passing cell regressed" and for "the cells never
    #     produced a result": a fixture that would not build, a peer that is not
    #     installed, a membership that only skipped. The recipe already knows
    #     (exit 2 vs 1) and the workflow forwards it.
    cells_failed = _steps(
        ("Build the nros CLI", "success"),
        ("Build the fixtures those rows resolve", "success"),
        ("Run the board cells with a recorded PASS", "failure"))
    chk("a cells-step failure with NOTHING said is still a verdict",
        classify(cells_failed, "failure").kind == "verdict-fail")
    said_no = classify(cells_failed, "failure", cells_ran=False)
    chk("...and the SAME steps are a no-verdict when the runner says the cells "
        "did not run",
        said_no.kind == "no-verdict" and said_no.stage == CELLS)
    chk("...which does not claim to have reached the cells",
        said_no.reached_cells is False)
    chk("...and says so in its label",
        said_no.label == LABELS["no-verdict-cells"])
    chk("cells_ran=True changes nothing about a passing lane",
        classify(_steps(("Run the board cells with a recorded PASS", "success")),
                 "success", cells_ran=True).kind == "verdict-pass")

    # 8d. issue 1754 — run 37685900447, job 113013514754, VERBATIM lines from
    #     its job log. `just ci matrix` failed because `check::default` (the
    #     `check fast` preflight, gate `zephyr-workspace-foreign-checkout`)
    #     failed; 0 cells ran, 0 `PASS` lines. The reporter said
    #     `VERDICT: cells ran and FAILED`.
    run_37685900447 = _steps(
        ("just setup tier2", "success"),
        ("Verify this runner's labels are true", "success"),
        ("just build tier2", "success"),
        ("just ci matrix", "failure"))
    pre = lane_step_markers.reached_cells(RUN_37685900447_LOG)
    chk("37685900447's log: the runner says no cell started",
        pre.cells_ran is False and pre.failed_step == "check::default")
    chk("37685900447 without the markers is what it USED to say (the defect)",
        classify(run_37685900447, "failure").kind == "verdict-fail")
    got = classify(run_37685900447, "failure", inner=pre)
    chk("37685900447 with the markers is NOT `cells ran and FAILED`",
        got.label != LABELS["verdict-fail"] and got.kind == "no-verdict")
    chk("...it reads as a preflight failure and names the inner step",
        got.label == LABELS["no-verdict-preflight"]
        and got.failing_step.endswith("check::default")
        and got.reached_cells is False)
    ran = lane_step_markers.reached_cells(
        "==> ci tier2 [1/4] check::default — started 01:00:00Z\n"
        "<== ci tier2 [1/4] check::default — ok after 2m13s (at 01:02:13Z)\n"
        "==> ci tier2 [2/4] rust-rtos-link-check — started 01:02:13Z\n"
        "<== ci tier2 [2/4] rust-rtos-link-check — ok after 9m00s (at 01:11:13Z)\n"
        "==> ci tier2 [3/4] test-all — started 01:11:13Z\n"
        "<== ci tier2 [3/4] test-all — FAILED after 40m00s (at 01:51:13Z)\n")
    chk("a lane whose `test-all` ran and failed is still a VERDICT",
        classify(run_37685900447, "failure", inner=ran).kind == "verdict-fail")
    chk("no markers at all keeps the step-outcome answer (nobody said)",
        lane_step_markers.reached_cells("").cells_ran is None)
    for line in _runner_marker_contract(chk):
        print(line, file=sys.stderr)
    _ci_just_cell_steps(chk)

    # 9. THE MAP IS AUTHORED, SO IT DRIFTS. `--report` is handed step names by
    #    the workflow, and those names are a copy of the workflow's own `- name:`
    #    lines. Rename a step and the copy silently stops matching — the tool
    #    would keep reporting, on a step that no longer exists, and the answer
    #    would be wrong in the safe-looking direction. Same shape as the RMW
    #    parity map reading `gap` for slots that had landed.
    for line in _workflow_consistency(chk):
        print(line, file=sys.stderr)

    if verbose:
        print(f"\n{ok} passed, {fail} failed")
    if fail:
        print("lane-stage self-test: FAILED", file=sys.stderr)
        raise SystemExit(1)
    return 0


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--report", action="store_true",
                    help="classify the current job from $NROS_LANE_STEPS")
    ap.add_argument("--lane", default="this lane")
    ap.add_argument("--history", action="store_true",
                    help="classify recent runs of a workflow via `gh`")
    ap.add_argument("--workflow", default="run-matrix.yml")
    ap.add_argument("--job", default=None,
                    help="restrict --history to one job name")
    ap.add_argument("--runs", type=int, default=8)
    ap.add_argument("--selftest", action="store_true")
    args = ap.parse_args()

    if args.selftest:
        return selftest(verbose=True)
    # Every invocation proves the classifier still separates the two kinds. It
    # is pure text, so it costs milliseconds.
    selftest()

    if args.report:
        return report(args.lane, os.environ.get("NROS_LANE_STEPS", "[]"))
    if args.history:
        return history(args.workflow, args.runs, args.job)
    ap.print_help()
    return 0


if __name__ == "__main__":
    sys.exit(main())

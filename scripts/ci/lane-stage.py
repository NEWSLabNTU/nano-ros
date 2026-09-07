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
    (PROVISIONING, ("setup", "set up", "provision", "install", "checkout",
                    "cache", "fetch", "submodule", "labels", "doctor",
                    "reclaim disk", "free disk", "apt", "rustup", "register")),
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


def classify(steps, job_conclusion=None):
    """Classify one job's ordered steps.

    `steps` is [{"name": str, "conclusion": str}] — the shape `gh run view
    --json jobs` returns AND the shape the workflow reports from
    `steps.<id>.outcome`. `job_conclusion` is optional and only used to tell a
    job that died with no failing step from one that never ran.

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
    res = classify(steps)

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
        out.append("  look exactly like this — which is how issues 1075/1098/1104/1114")
        out.append("  rode in (issue 1158).")
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


WORKFLOW = os.path.join(
    os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__)))),
    ".github", "workflows", "run-matrix.yml")

# Every step name `run-matrix.yml` hands to `--report`, and the stage it MUST
# classify as. Asserted against the workflow itself, both directions.
EXPECTED_MAP = {
    "just setup tier2": PROVISIONING,
    "Verify this runner's labels are true": PROVISIONING,
    "just build tier2": BUILD,
    "just ci matrix": CELLS,
}


def _workflow_consistency(chk):
    """Cross-check the authored step->stage map against run-matrix.yml.

    Returns lines to print when the check could not be made — a REPORTED skip,
    never a silent one (issue 1043's shape: "could not evaluate" is a third
    answer, not a pass).
    """
    try:
        import yaml
    except ModuleNotFoundError:
        return ["[skip] lane-stage: PyYAML missing — the run-matrix.yml "
                "consistency arm did NOT run"]
    if not os.path.exists(WORKFLOW):
        return [f"[skip] lane-stage: {WORKFLOW} absent — the consistency arm "
                "did NOT run"]

    with open(WORKFLOW, encoding="utf-8") as fh:
        doc = yaml.safe_load(fh)
    steps = doc["jobs"]["matrix"]["steps"]
    names = [s.get("name", "") for s in steps]
    reporter = [s for s in steps
                if "lane-stage.py" in str(s.get("run", ""))]

    chk("run-matrix.yml has exactly one lane-stage reporter", len(reporter) == 1)
    if not reporter:
        return []

    declared = json.loads(
        re.sub(r"\$\{\{[^}]*\}\}", "success",
               reporter[0]["env"]["NROS_LANE_STEPS"]))
    declared_names = [d["name"] for d in declared]

    chk("every step name the workflow reports is a real step in that job",
        all(n in names for n in declared_names))
    chk("every staged step of the workflow is reported",
        set(declared_names) == set(EXPECTED_MAP))
    for n in declared_names:
        chk(f"`{n}` still classifies as {EXPECTED_MAP.get(n)}",
            stage_of(n) == EXPECTED_MAP.get(n))
    chk("the reporter runs `if: always()` — a stage report that is skipped when "
        "the lane dies is no report",
        "always()" in str(reporter[0].get("if", "")))
    chk("the matrix job exports the stage label for the coverage job's name",
        "stage.outputs.label" in
        str(doc["jobs"]["matrix"].get("outputs", {}).get("stage_label", "")))
    chk("the coverage job's NAME carries the stage",
        "needs.matrix.outputs.stage_label" in str(doc["jobs"]["coverage"]["name"]))
    return []


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

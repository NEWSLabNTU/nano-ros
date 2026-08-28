#!/usr/bin/env python3
"""phase-395 W17 — does each platform's CI evidence SUPPORT the tier it claims?

IT REPORTS. IT DOES NOT GATE.

`check-board-tiers` runs in `check-fast`: offline, deterministic, no network. It
can prove a tier's obligation is STRUCTURALLY met — the platform has Runtime
cells, it names a real `PlatformId`, it is in a lane, it has owners — and it
cannot know whether that lane is GREEN. That gap is not hypothetical: the gate
prints "Board support tiers match the evidence" while the `0 7` nightly cron is
failing on `threadx_linux`, `nuttx` and `freertos`. Structural coverage
asserted, health unverified.

This is the other half, and it is deliberately a different KIND of tool:

  check-board-tiers   offline, deterministic  -> GATES   (exit 1 on a defect)
  tier-health         network, time-varying   -> REPORTS (exit 0 on a defect)

The same split as `check-*` versus `enable-merge-queue.sh --readiness`.

WHY IT MUST NOT GATE, AND WHY THERE IS NO `--check`

Rust's target-tier policy, which phase-395 W15 adopted wholesale, requires an
RFC to demote a tier-1 target and explicitly permits temporarily disabling a
target's tests WITHOUT demoting it. Zephyr's is the same shape. Both refuse to
auto-demote on a red, for a reason worth restating: auto-demotion on a red is
precisely the pressure that makes people silence tests. A tier is a promise
between people. It changes when someone decides, with a record — not as a side
effect of a bad night.

So this exits 0 whether the evidence is green, red, or absent. It exits non-zero
ONLY when the TOOL is broken: no `gh`, not authenticated, an unreadable
registry, a failed self-test. That is the same distinction `reserve-claim.sh`
draws between contention (a normal answer) and a broken remote (an error).

THREE STATES, NOT TWO

"no evidence", "red evidence" and "green" mean different things and only one of
them is a defect in the PLATFORM:

  GREEN     the lane ran and passed          -> the tier is earned
  RED       the lane ran and failed          -> the platform (or the lane) is broken
  MIXED     the lane ran and disagreed       -> intermittent; earns nothing
  NO-RUNS   the lane never produced a verdict -> nothing is known either way

A lane that never ran is not a failing lane, and a report that conflates them
invents defects. `supported` is therefore TRI-state: a tier number, `none`
(there is evidence and it is negative), or `unknown` (there is no evidence).

ONLY CONCLUSIVE OUTCOMES COUNT

`success` and `failure` are verdicts. `cancelled`, `skipped`, `startup_failure`,
a still-running job and an absent job are NOT. GitHub's `cancel-in-progress`
produces cancellations routinely whenever pushes land faster than CI, and this
repo's 05:00 Zephyr cron currently SKIPS every one of its jobs — counting either
as a failure would report a platform as broken on the strength of a scheduling
artifact. `enable-merge-queue.sh --readiness` made exactly this mistake once
(five dispatches fired to measure a flake showed up as `absent,absent,cancelled`
and blocked a stage); the fix there and here is to drop inconclusive rows from
the record and report how many were dropped, so a thin record is visible rather
than silently equal to a full one.

WHY PYTHON AND NOT SH

`check-board-tiers.py`'s header explains its regex TOML parser: CI hosts are not
guaranteed a TOML library (this repo's Python is 3.10, so no `tomllib`). That
reasoning applies here too — and the cheapest way to honour it is to not write a
second parser at all. This IMPORTS `parse_registry` from that gate, so the
registry has one reader, not two spellings that can disagree (CLAUDE.md: one
shared helper, never a second idiom). Hence no `tomllib`/`tomli` import: the
question does not arise.

The rest is a lattice — claimed tier x execution class x lane cadence x evidence
state — plus a tri-state join over several lanes per row. In `awk` that is
unreadable; in Python it is a pure function with a table of synthetic cases
beside it, which is what "testable without a network" requires.

STRUCTURE: FETCH AND JUDGE ARE SEPARATE

Everything under "classification" is pure: records in, verdict out. Everything
under "fetch" touches `gh` and returns those records. `--selftest` exercises the
classifier on synthetic rows with no network at all, and it also runs on the
NORMAL path (a negative control nobody runs decays into a comment) — a failed
self-test is a TOOL failure, so it is one of the few things that exits non-zero.

Usage:
  scripts/ci/tier-health.py                 # the report
  scripts/ci/tier-health.py --limit 20      # look further back
  scripts/ci/tier-health.py --selftest      # classifier only, no network
"""

import argparse
import importlib.util
import json
import shutil
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
REGISTRY = ROOT / "packages/boards/board-support.toml"
NIGHTLY = ROOT / ".github/workflows/nightly.yml"
WORKFLOWS = ROOT / ".github/workflows"

DEFAULT_REPO = "NEWSLabNTU/nano-ros"
DEFAULT_BRANCH = "main"

# How many CONCLUSIVE outcomes make a record. Five is what
# `enable-merge-queue.sh --readiness` uses for the same question.
WANT_CONCLUSIVE = 5


class ToolFailure(Exception):
    """The tool could not do its job. The only thing that exits non-zero."""


def rel(path):
    """Repo-relative if it is inside the repo, absolute otherwise.

    `Path.relative_to` RAISES on a path outside the root, and every use of it
    here is inside an error message — so the bare form turns "your input was
    unreadable" into a ValueError traceback with no mention of the input.
    """
    try:
        return str(path.relative_to(ROOT))
    except ValueError:
        return str(path)


# ---------------------------------------------------------------------------
# The lane model
# ---------------------------------------------------------------------------
#
# A tier is a CADENCE promise, so a lane is characterised by how often it can
# deliver a verdict, not by its name:
#
#   per-merge  a regression fails BEFORE it lands      -> supports tier 1
#   daily      a regression is found within a day      -> supports tier 2
#   (none)     build-only; nothing runs it             -> tier 3 by construction
#
# `check-board-tiers` binds tiers to lane NAMES, which W15 records as the defect
# it was: a name is a scheduling artifact, so restructuring CI breaks a tier
# claim for no substantive reason. Cadence is the thing the promise is about.

CADENCE_TIER = {"per-merge": 1, "daily": 2}


class Lane:
    """A concrete CI job that could carry a platform's runtime evidence."""

    def __init__(self, cadence, workflow, job, source, events=None):
        self.cadence = cadence      # "per-merge" | "daily"
        self.workflow = workflow    # workflow FILE, e.g. "nightly.yml"
        self.job = job              # exact job display name
        self.source = source        # human-readable "where this comes from"
        self.events = events        # restrict to these run events, or None

    def tier(self):
        return CADENCE_TIER[self.cadence]

    def __repr__(self):
        return f"Lane({self.cadence}, {self.workflow}, {self.job!r})"


# The per-merge runtime lane. EMPTY, and that is the finding, not an oversight:
# tier 1 promises runtime evidence every merge and no lane delivers it today.
# The host-executable group's runtime cells run only in the nightly sweep and in
# host-tests' POST-merge push lane. `ci-l2` is phase-395 W16 and has not landed.
#
# When W16 lands, add its (workflow, job) here — one line. Until then every
# tier-1 row reports its obligation as ABSENT, which is the true state.
#
# This list is AUTHORED, and an authored map drifts (the rmw parity map read
# "gap" for 28 slots that had landed). So `per_merge_candidates()` below
# independently enumerates every workflow that CAN report per-merge, and the
# report prints that set: a reader sees the candidates the table did not claim,
# instead of taking "absent" on trust.
PER_MERGE_RUNTIME_LANES = []

# `Linux` carries no `nightly_token` — it is the native host, and its runtime
# evidence lives in host-tests.yml. `nightly.yml`'s own `lane-coverage` job says
# so in its `elsewhere` map ("native -> host-tests.yml — cron 0 3"), so this
# agrees with the workflow rather than inventing a second answer.
#
# The `unit` job is NOT here. It is `just test-unit`: no fixtures, no runtime
# cells. `integration` is the one that spawns prebuilt example binaries, so it
# is the only host-tests job that is runtime evidence for the Linux board.
HOST_TESTS_LANE = Lane(
    "daily",
    "host-tests.yml",
    "nros-tests integration (host)",
    "host-tests.yml — push to main + cron 0 3, just test-integration",
)

# `nightly_token = "zephyr"` does not name a job in the platform matrix: the
# Zephyr line has its own 05:00 cron with its own jobs, whose display names are
# templated (`zephyr ${{ matrix.line }} / ${{ matrix.example }}`). Matching by
# PREFIX is deliberate — an exact name cannot be written for a matrix job, and
# `nightly.yml`'s `elsewhere` map records the same home ("zephyr -> nightly.yml
# zephyr-* jobs — cron 0 5").
ZEPHYR_JOB_PREFIX = "zephyr "


def nightly_platform_tokens(text):
    """The tokens the nightly platform matrix can actually run.

    Parsed from the `runnable="..."` literal, for the reason
    `check-board-tiers.nightly_tokens` gives: the `all="qemu freertos …"`
    spelling left in the file is a COMMENT describing the old hand-written
    shape, and a parser that matches it reads a set nothing schedules.

    The platform job's display name is `${{ matrix.plat }}`, so a token in this
    set IS a job name in a nightly run — which is what makes the outcome
    lookup below a name match rather than a guess.
    """
    for line in text.splitlines():
        stripped = line.strip()
        if stripped.startswith('runnable="') and stripped.endswith('"'):
            return set(stripped[len('runnable="'):-1].split())
    return set()


def per_merge_candidates():
    """Workflows that CAN report on a pull request or a merge group.

    The cross-check on `PER_MERGE_RUNTIME_LANES`. It answers a different
    question — "which workflows could carry per-merge evidence" — so printing it
    beside an empty table lets a reader see that the absence was measured, not
    assumed. Text-level and deliberately loose: it is a cross-check, not a
    parser, and a false positive here costs a line of output.
    """
    out = []
    if not WORKFLOWS.is_dir():
        return out
    for path in sorted(WORKFLOWS.glob("*.yml")):
        text = path.read_text()
        head = text.split("\njobs:", 1)[0]
        events = [e for e in ("pull_request", "merge_group") if e + ":" in head]
        if events:
            out.append((path.name, ",".join(events)))
    return out


def resolve_lanes(row, nightly_tokens):
    """Which lanes should carry this row's runtime evidence.

    Pure: no network, no filesystem beyond what the caller already read. Returns
    `(lanes, notes)` where each note is `(kind, text)` and kind is one of:

      "MISSING"    the lane the tier OBLIGES does not exist. The platform is not
                   at fault and nothing about it is red; there is simply nothing
                   running that could keep the promise.
      "UNCHECKED"  the registry or this table names a lane that resolves to
                   nothing. The tool cannot answer, which is a different
                   sentence from "the platform failed".

    Neither is ever assumed green.

    A tier-1 row gets BOTH its own obligation (per-merge) and the tier-2 lane
    below it, because "which tier does the evidence support" cannot be answered
    without looking one step down. A tier-1 platform whose nightly is green has
    earned tier 2, and saying so is more useful than a bare "not tier 1".
    """
    tier = str(row.get("tier", ""))
    notes = []
    if tier == "3":
        # Build-only means it. There is no runtime lane to query, and inventing
        # one so the row can "pass" is the false green this whole family of
        # gates exists to prevent.
        return [], notes
    if tier not in ("1", "2"):
        return [], notes

    lanes = []
    if tier == "1":
        if PER_MERGE_RUNTIME_LANES:
            lanes.extend(PER_MERGE_RUNTIME_LANES)
        else:
            notes.append((
                "MISSING",
                "tier 1 obliges a PER-MERGE runtime lane — a regression must "
                "fail before it lands. No job runs runtime cells on "
                "pull_request or merge_group (`ci-l2` is phase-395 W16, not "
                "landed), so the tier-1 evidence is MISSING: not green, and "
                "not red either."))

    token = row.get("nightly_token")
    platform = row.get("matrix_platform")
    if token is None:
        if platform == "Linux":
            lanes.append(HOST_TESTS_LANE)
        else:
            notes.append((
                "UNCHECKED",
                f"no `nightly_token` and not the native host — nothing maps "
                f"{platform!r} to a daily lane"))
    elif token == "zephyr":
        lanes.append(Lane("daily", "nightly.yml", ZEPHYR_JOB_PREFIX,
                          "nightly.yml zephyr-* jobs — cron 0 5",
                          events=("schedule", "workflow_dispatch")))
    elif token in nightly_tokens:
        lanes.append(Lane("daily", "nightly.yml", token,
                          f"nightly.yml platform matrix job '{token}' — cron 0 7",
                          events=("schedule", "workflow_dispatch")))
    else:
        # A real finding, not a tool bug: the registry names a nightly job that
        # the workflow cannot schedule. `FreertosPosix` is the live case —
        # `nightly_token = "freertos_posix"` against a `runnable` set that has
        # only `freertos`. Reported as UNCHECKED, because whether those cells run
        # inside the `freertos` job is a question this tool cannot answer.
        notes.append((
            "UNCHECKED",
            f"`nightly_token = \"{token}\"` names no job the nightly platform "
            f"matrix can schedule (runnable: {' '.join(sorted(nightly_tokens))})"))
    return lanes, notes


# ---------------------------------------------------------------------------
# Classification — PURE. Everything below here is unit-tested with no network.
# ---------------------------------------------------------------------------

CONCLUSIVE = ("success", "failure")


def classify_record(outcomes):
    """A lane's conclusive outcomes (newest first) -> its evidence state.

    `outcomes` has already had the inconclusive rows dropped by the fetch layer;
    the caller reports how many, so a two-sample record is legible as thin
    rather than passing for a five-sample one.
    """
    if not outcomes:
        return "NO-RUNS"
    if all(o == "success" for o in outcomes):
        return "GREEN"
    if all(o == "failure" for o in outcomes):
        return "RED"
    return "MIXED"


def tier_supported(lane_states):
    """`[(lane_tier, state)]` -> the strongest tier the evidence supports.

    Tri-state, and the distinction is the point:

      an int    a lane at that cadence is GREEN
      "none"    a lane RAN and did not pass — evidence exists and is negative
      "unknown" no lane produced a verdict — nothing is known either way

    MIXED earns nothing. A promise that holds four times in five is not the
    promise a tier makes, and rounding it up is how a flaky lane certifies a
    tier. It is still reported separately from RED, because "intermittent" and
    "broken" send a maintainer to different places.
    """
    green = [t for t, s in lane_states if s == "GREEN"]
    if green:
        return min(green)
    if any(s in ("RED", "MIXED") for _, s in lane_states):
        return "none"
    return "unknown"


def classify_row(claimed, lane_states, has_resolution_notes):
    """The row verdict: `(status, supported, sentence)`.

    Statuses, each meaning something a maintainer would act on differently:

      OK           the evidence supports the claim
      OVERCLAIMED  claimed > supported, and BOTH are named
      UNVERIFIED   no evidence either way — not a defect, an absence
      NO-CLAIM     tier 3: build-only, so there is no runtime evidence to check
      UNCHECKED    a lane could not be resolved; the tool cannot answer
    """
    if claimed == "3":
        return ("NO-CLAIM", "3",
                "tier 3 is build-only: no runtime evidence exists to check, and "
                "none is claimed. This is not a green.")

    supported = tier_supported(lane_states)

    if supported == "unknown":
        if has_resolution_notes and not lane_states:
            return ("UNCHECKED", supported,
                    f"claims tier {claimed}; no lane could be resolved to carry "
                    "its runtime evidence, so nothing was measured.")
        return ("UNVERIFIED", supported,
                f"claims tier {claimed}; no lane produced a verdict, so the "
                "evidence is MISSING — which is not the same as red.")

    if supported == "none":
        # Deliberately "every lane that produced a verdict", not "the lanes":
        # a tier-1 row here typically has its per-merge obligation MISSING and
        # only its daily lane red, and saying "the lanes failed" would report a
        # lane that never ran as a failing one — the exact conflation this tool
        # exists to avoid.
        return ("OVERCLAIMED", supported,
                f"claims tier {claimed}; every lane that produced a verdict "
                "failed, so the evidence supports NO runtime tier.")

    if supported <= int(claimed):
        return ("OK", supported,
                f"claims tier {claimed}; evidence supports tier {supported}.")

    return ("OVERCLAIMED", supported,
            f"claims tier {claimed}; evidence supports tier {supported}.")


# ---------------------------------------------------------------------------
# Fetch — the only part that touches the network
# ---------------------------------------------------------------------------

def _gh(args, fatal):
    """Run `gh`. `fatal` decides whether a failure is a TOOL failure."""
    try:
        p = subprocess.run(["gh"] + args, capture_output=True, text=True, timeout=120)
    except (OSError, subprocess.SubprocessError) as exc:
        if fatal:
            raise ToolFailure(f"gh {' '.join(args)}: {exc}") from exc
        return None
    if p.returncode != 0:
        if fatal:
            raise ToolFailure(f"gh {' '.join(args)} failed: {p.stderr.strip()}")
        return None
    return p.stdout


def fetch_workflow_runs(workflow, repo, branch, limit, cache):
    """`[(run_id, event, {job_name: conclusion_or_None})]`, newest first.

    Cached per workflow: eleven registry rows resolve onto two workflows, and
    re-listing them per row would multiply the API calls by five for identical
    answers.

    A failed `gh run list` is FATAL — it means the remote could not be read at
    all, which is a broken tool, not an answer. A failed `gh run view` for one
    run is NOT: it costs one sample, and the sample count is printed.
    """
    if workflow in cache:
        return cache[workflow]
    out = _gh(["run", "list", "--repo", repo, "--workflow", workflow,
               "--branch", branch, "--limit", str(limit),
               "--json", "databaseId,event,conclusion"], fatal=True)
    try:
        runs = json.loads(out)
    except ValueError as exc:
        raise ToolFailure(f"gh run list --workflow {workflow}: unreadable JSON: {exc}")

    rows = []
    for run in runs:
        rid = run["databaseId"]
        detail = _gh(["run", "view", str(rid), "--repo", repo, "--json", "jobs"],
                     fatal=False)
        jobs = {}
        if detail:
            try:
                for job in json.loads(detail).get("jobs", []):
                    jobs[job["name"]] = job.get("conclusion")
            except ValueError:
                jobs = {}
        rows.append((rid, run.get("event"), jobs))
    cache[workflow] = rows
    return rows


def lane_outcomes(lane, runs):
    """Reduce fetched runs to `(conclusive_outcomes, dropped_counts)` for a lane.

    PURE over its inputs — `runs` is data, so this half of the fetch is unit
    tested below without a network.

    A lane whose `job` ends in a space is a PREFIX match (the templated Zephyr
    matrix jobs); everything else is an exact display-name match. A run
    contributes ONE outcome even when several jobs match: a matrix of eight
    Zephyr example builds is one nightly verdict, and counting each leaf would
    weight that platform eight times against a single-job one.
    """
    outcomes, dropped = [], {}
    for _rid, event, jobs in runs:
        if lane.events and event not in lane.events:
            continue
        if lane.job.endswith(" "):
            matched = [c for name, c in jobs.items() if name.startswith(lane.job)]
        else:
            matched = [c for name, c in jobs.items() if name == lane.job]
        if not matched:
            dropped["absent"] = dropped.get("absent", 0) + 1
            continue
        # A matrix leg that failed makes the run's verdict a failure; a run is
        # green only when every matched leg is.
        if any(c == "failure" for c in matched):
            verdict = "failure"
        elif all(c == "success" for c in matched):
            verdict = "success"
        else:
            # cancelled / skipped / startup_failure / still running.
            # `conclusion: null` is GitHub's spelling for "no verdict yet"; it
            # must not print as the string "None", which reads like a bug.
            names = sorted({(c if c else "running")
                            for c in matched if c != "success"})
            key = names[0] if names else "running"
            dropped[key] = dropped.get(key, 0) + 1
            continue
        outcomes.append(verdict)
        if len(outcomes) >= WANT_CONCLUSIVE:
            break
    return outcomes, dropped


# ---------------------------------------------------------------------------
# Self-test — the classifier, with no network
# ---------------------------------------------------------------------------

def selftest(quiet=False):
    fails = []

    def check(cond, what):
        if not cond:
            fails.append(what)

    # --- classify_record: the three states are three states -----------------
    check(classify_record([]) == "NO-RUNS",
          "an empty record must be NO-RUNS, never a pass and never a red")
    check(classify_record(["success"] * 5) == "GREEN", "all-success is GREEN")
    check(classify_record(["failure"] * 5) == "RED", "all-failure is RED")
    check(classify_record(["success", "failure"]) == "MIXED", "disagreement is MIXED")
    check(classify_record(["success"]) == "GREEN",
          "a THIN record still has a verdict; thinness is reported, not re-judged")

    # --- tier_supported: the tri-state join ---------------------------------
    check(tier_supported([]) == "unknown",
          "no lanes at all is unknown, not none")
    check(tier_supported([(1, "NO-RUNS"), (2, "NO-RUNS")]) == "unknown",
          "lanes that never ran leave the tier unknown — absence is not failure")
    check(tier_supported([(1, "NO-RUNS"), (2, "GREEN")]) == 2,
          "a green daily lane supports tier 2 even with the per-merge lane absent")
    check(tier_supported([(1, "GREEN"), (2, "RED")]) == 1,
          "the STRONGEST green wins; a red below it cannot demote it")
    check(tier_supported([(1, "NO-RUNS"), (2, "RED")]) == "none",
          "a red lane is evidence, and it supports no tier")
    check(tier_supported([(2, "MIXED")]) == "none",
          "MIXED earns nothing — a promise that holds 4-in-5 is not the promise")

    # --- classify_row -------------------------------------------------------
    st, sup, _ = classify_row("3", [], False)
    check(st == "NO-CLAIM" and sup == "3",
          "tier 3 must say there is nothing to check, NOT report a false pass")

    st, sup, _ = classify_row("1", [(1, "NO-RUNS"), (2, "NO-RUNS")], False)
    check(st == "UNVERIFIED" and sup == "unknown",
          "no evidence must read UNVERIFIED, distinct from a red")

    st, sup, _ = classify_row("1", [(1, "NO-RUNS"), (2, "RED")], False)
    check(st == "OVERCLAIMED" and sup == "none",
          "a claimed tier 1 over a red nightly is OVERCLAIMED")

    st, sup, msg = classify_row("1", [(1, "NO-RUNS"), (2, "GREEN")], False)
    check(st == "OVERCLAIMED" and sup == 2 and "tier 1" in msg and "tier 2" in msg,
          "where claimed > supported BOTH tiers must be named in the sentence")

    st, sup, _ = classify_row("2", [(2, "GREEN")], False)
    check(st == "OK" and sup == 2, "a green daily lane satisfies a tier-2 claim")

    st, sup, _ = classify_row("2", [(1, "GREEN")], False)
    check(st == "OK" and sup == 1,
          "evidence STRONGER than the claim is OK — this tool never promotes")

    st, _, _ = classify_row("2", [], True)
    check(st == "UNCHECKED",
          "an unresolvable lane is UNCHECKED, never assumed green")

    # --- resolve_lanes ------------------------------------------------------
    toks = {"qemu", "freertos", "nuttx", "threadx_linux", "threadx_riscv64", "esp32"}

    lanes, notes = resolve_lanes(
        {"tier": 1, "matrix_platform": "ThreadxLinux", "nightly_token": "threadx_linux"},
        toks)
    check([lane.cadence for lane in lanes] == ["daily"],
          "with no per-merge lane in the table a tier-1 row gets only the daily one")
    check([k for k, _ in notes] == ["MISSING"] and any(
              "PER-MERGE" in txt for _, txt in notes),
          "...and the missing per-merge obligation must be SAID as MISSING — "
          "not silently dropped, and not spelled the same as an unresolvable lane")

    lanes, notes = resolve_lanes(
        {"tier": 1, "matrix_platform": "Linux"}, toks)
    check([lane.workflow for lane in lanes] == ["host-tests.yml"] and not [
        txt for _, txt in notes if "nightly_token" in txt],
        "the native host resolves to host-tests.yml, not to a missing nightly token")

    lanes, notes = resolve_lanes(
        {"tier": 2, "matrix_platform": "FreertosPosix", "nightly_token": "freertos_posix"},
        toks)
    check(not lanes and [k for k, _ in notes] == ["UNCHECKED"]
          and any("names no job" in txt for _, txt in notes),
          "a nightly_token the matrix cannot schedule must resolve to NO lane "
          "plus an UNCHECKED note — not silently to the sibling job")

    lanes, notes = resolve_lanes(
        {"tier": 3, "matrix_platform": "Fvp"}, toks)
    check(not lanes and not notes,
          "tier 3 needs no lane and owes no explanation beyond its own tier")

    lanes, _ = resolve_lanes(
        {"tier": 2, "matrix_platform": "ZephyrQemuCortexM", "nightly_token": "zephyr"},
        toks)
    check(len(lanes) == 1 and lanes[0].job == ZEPHYR_JOB_PREFIX,
          "`zephyr` resolves to the 05:00 cron's templated jobs by prefix")

    lanes, _ = resolve_lanes({"tier": "infra", "crate": "nros-board-common"}, toks)
    check(not lanes, "infra carries no tier promise, so it gets no lane")

    # --- lane_outcomes: inconclusive rows are DROPPED, not counted red ------
    lane = Lane("daily", "nightly.yml", "nuttx", "x", events=("schedule",))
    runs = [
        (1, "schedule", {"nuttx": "failure"}),
        (2, "schedule", {"nuttx": "cancelled"}),
        (3, "schedule", {"nuttx": None}),
        (4, "push", {"nuttx": "success"}),
        (5, "schedule", {"qemu": "success"}),
        (6, "schedule", {"nuttx": "success"}),
    ]
    got, dropped = lane_outcomes(lane, runs)
    check(got == ["failure", "success"],
          "only success/failure count; cancelled, running and off-event are dropped")
    check(dropped.get("cancelled") == 1 and dropped.get("running") == 1
          and dropped.get("absent") == 1,
          "every dropped row must be COUNTED, so a thin record reads as thin")

    zl = Lane("daily", "nightly.yml", "zephyr ", "x", events=("schedule",))
    got, _ = lane_outcomes(zl, [(1, "schedule", {"zephyr a": "skipped",
                                                 "zephyr b": "skipped"})])
    check(got == [], "an all-skipped matrix is NO evidence, not a pass")
    got, _ = lane_outcomes(zl, [(1, "schedule", {"zephyr a": "success",
                                                 "zephyr b": "failure"})])
    check(got == ["failure"], "one failing matrix leg makes the run's verdict failure")
    got, _ = lane_outcomes(zl, [(1, "schedule", {"zephyr a": "success",
                                                 "zephyr b": "success"})])
    check(got == ["success"],
          "a matrix contributes ONE outcome per run, not one per leg")

    check(len(lane_outcomes(lane, [(i, "schedule", {"nuttx": "success"})
                                   for i in range(20)])[0]) == WANT_CONCLUSIVE,
          "the record is capped at WANT_CONCLUSIVE samples")

    # --- the parsers --------------------------------------------------------
    check(nightly_platform_tokens('  runnable="qemu freertos nuttx"\n')
          == {"qemu", "freertos", "nuttx"}, "runnable= is parsed from the literal")
    check(nightly_platform_tokens('  all="qemu freertos"\n') == set(),
          "the `all=` COMMENT spelling must NOT be read as the runnable set")

    if fails:
        for f in fails:
            print(f"  FAIL  {f}", file=sys.stderr)
        return False
    if not quiet:
        print(f"tier-health self-test: OK ({len(fails)} failures)")
    return True


# ---------------------------------------------------------------------------
# Report
# ---------------------------------------------------------------------------

def load_parse_registry():
    """Borrow `check-board-tiers.py`'s registry reader rather than writing a second.

    The filename has a hyphen, so it is not importable by name. The alternative
    is a second regex parser for the same file, which is the "one shared helper,
    never a second spelling" rule this repo keeps relearning.
    """
    path = ROOT / "scripts/check-board-tiers.py"
    spec = importlib.util.spec_from_file_location("nros_check_board_tiers", path)
    if spec is None or spec.loader is None:
        raise ToolFailure(f"cannot load {path}")
    mod = importlib.util.module_from_spec(spec)
    try:
        spec.loader.exec_module(mod)
    except Exception as exc:                      # noqa: BLE001 — any import fault
        raise ToolFailure(f"cannot load {path}: {exc}") from exc
    return mod.parse_registry


def label(row):
    plat = row.get("matrix_platform")
    return plat if plat else f"{row.get('crate', '?')} (no platform)"


def wrap(text, indent=" " * 16, width=76):
    """Fold a note onto the report's continuation column.

    Not cosmetic: these notes are the longest lines in the output and the
    interesting half is at the END of them ("...so the tier-1 evidence is
    MISSING"). A note that runs off the terminal loses exactly the clause that
    distinguishes MISSING from red.
    """
    words, lines, cur = text.split(), [], ""
    for word in words:
        if cur and len(cur) + 1 + len(word) > width:
            lines.append(cur)
            cur = word
        else:
            cur = f"{cur} {word}" if cur else word
    if cur:
        lines.append(cur)
    return ("\n" + indent).join(lines)


def main(argv=None):
    ap = argparse.ArgumentParser(
        description="Report whether each platform's CI evidence supports its "
                    "claimed support tier. REPORTS ONLY — never gates.")
    ap.add_argument("--repo", default=DEFAULT_REPO)
    ap.add_argument("--branch", default=DEFAULT_BRANCH)
    ap.add_argument("--limit", type=int, default=14,
                    help="workflow runs to inspect per workflow (default 14)")
    ap.add_argument("--selftest", action="store_true",
                    help="run the classifier tests only; no network")
    args = ap.parse_args(argv)

    if args.selftest:
        return 0 if selftest() else 3

    # Always, not only behind the flag: a negative control nobody runs decays
    # into a comment, and this classifier's whole job is to draw distinctions.
    # A broken classifier is a TOOL failure, which is one of the few things
    # allowed to exit non-zero here.
    if not selftest(quiet=True):
        print("[FAIL] tier-health self-test failed — the classifier is broken, "
              "so its report cannot be trusted", file=sys.stderr)
        return 3

    if shutil.which("gh") is None:
        raise ToolFailure("gh not installed")
    if subprocess.run(["gh", "auth", "status"], capture_output=True).returncode != 0:
        raise ToolFailure("gh not authenticated (gh auth login)")
    try:
        registry_text = REGISTRY.read_text()
        nightly_text = NIGHTLY.read_text()
    except OSError as exc:
        raise ToolFailure(f"cannot read the registry/workflow: {exc}") from exc

    parse_registry = load_parse_registry()
    rows = parse_registry(registry_text)
    tokens = nightly_platform_tokens(nightly_text)
    if not tokens:
        raise ToolFailure(
            f"no `runnable=\"...\"` literal in {rel(NIGHTLY)} — the "
            "nightly job-name mapping cannot be derived, so no daily lane could "
            "be resolved for any platform")

    print(f"== board support tiers vs CI evidence — {args.repo} @ {args.branch} ==")
    print()
    print("  REPORT, NOT A GATE. Exit 0 whatever the evidence says; only a broken")
    print("  tool exits non-zero. Demoting a tier is a deliberate human act with a")
    print("  record (phase-395 W15) — auto-demotion on a red is the pressure that")
    print("  makes people silence tests.")
    print()
    print("  Only CONCLUSIVE outcomes count. cancelled / skipped / still-running /")
    print("  absent are dropped and counted, never read as failures.")
    print()
    print(f"  registry: {rel(REGISTRY)}"
          f"   window: last {args.limit} runs per workflow")
    print()

    cand = per_merge_candidates()
    print("  per-merge runtime lane: "
          + (", ".join(f"{w}:{j}" for w, j in
                       ((l.workflow, l.job) for l in PER_MERGE_RUNTIME_LANES))
             if PER_MERGE_RUNTIME_LANES else "NONE DECLARED"))
    print("    workflows that COULD report per-merge (cross-check, so the")
    print("    absence above is measured rather than assumed):")
    for name, events in cand:
        print(f"      {name:<20} on: {events}")
    if not PER_MERGE_RUNTIME_LANES:
        print("    none of them runs runtime cells — `ci-l2` is phase-395 W16.")
    print()

    cache = {}
    tally = {}
    overclaimed = []

    for row in rows:
        tier = str(row.get("tier", ""))
        if tier not in ("1", "2", "3"):
            continue                       # infra / scaffold promise nothing
        lanes, notes = resolve_lanes(row, tokens)
        klass = row.get("execution_class", "<unset>")

        print(f"  {label(row):<24} claimed tier {tier}   {klass}")
        for kind, text in notes:
            print(f"    lane        {kind}")
            print(f"                {wrap(text)}")
            print(f"    evidence    {kind}")

        lane_states = []
        for lane in lanes:
            runs = fetch_workflow_runs(lane.workflow, args.repo, args.branch,
                                       args.limit, cache)
            outcomes, dropped = lane_outcomes(lane, runs)
            state = classify_record(outcomes)
            lane_states.append((lane.tier(), state))
            drop = ", ".join(f"{n} {k}" for k, n in sorted(dropped.items()))
            print(f"    lane        {lane.source}")
            print(f"                cadence {lane.cadence} -> can support tier "
                  f"{lane.tier()}")
            if outcomes:
                print(f"    evidence    {state} — last {len(outcomes)} conclusive: "
                      f"{','.join(outcomes)}")
            else:
                print(f"    evidence    {state} — no conclusive outcome in the "
                      f"last {args.limit} runs")
            if drop:
                print(f"                dropped as inconclusive: {drop}")
            print(f"                source: gh run list --workflow {lane.workflow} "
                  f"--branch {args.branch}")

        status, supported, sentence = classify_row(tier, lane_states, bool(notes))
        print(f"    supported   {supported}")
        print(f"    VERDICT     {status}: {wrap(sentence)}")
        print()
        tally[status] = tally.get(status, 0) + 1
        if status == "OVERCLAIMED":
            overclaimed.append((label(row), tier, supported))

    print("== summary ==")
    print()
    for status in ("OK", "OVERCLAIMED", "UNVERIFIED", "UNCHECKED", "NO-CLAIM"):
        if status in tally:
            print(f"  {status:<12} {tally[status]}")
    print()
    if overclaimed:
        print("  Claimed above the evidence:")
        for name, claimed, supported in overclaimed:
            print(f"    {name}: claims tier {claimed}, evidence supports "
                  f"{supported}")
        print()
        print("  This is a REPORT. Nothing here demotes anything: fix the lane, or")
        print("  edit packages/boards/board-support.toml deliberately and say why.")
    else:
        print("  No platform claims more than its evidence supports.")
    print()
    print("  UNVERIFIED is not a defect in the platform — it means no lane produced")
    print("  a verdict. A lane that never ran is not a failing lane.")
    return 0


if __name__ == "__main__":
    try:
        sys.exit(main())
    except ToolFailure as exc:
        print(f"[FAIL] {exc}", file=sys.stderr)
        print("  This is a TOOL failure, not a verdict about any platform: the "
              "report could not be produced.", file=sys.stderr)
        sys.exit(3)

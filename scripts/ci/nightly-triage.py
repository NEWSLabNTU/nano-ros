#!/usr/bin/env python3
"""Did the nightly REPORT, or did it merely go red? — issue 0878.

A red cell answers one of two completely different questions, and today they are
the same colour:

  * the lane RAN and the code is broken           -> a verdict
  * the lane never ran and could not have         -> no verdict

Only the first is information. The second is a lane with no signal capacity, and
a regression that lands while a lane is in that state is invisible: it looks
exactly like yesterday's failure. That is not hypothetical — issue 0876 (a conf
change that made the Zephyr C talker unbuildable) sat undetected while the
nightly's own `zephyr 3.7 / c/talker` cell reported `failure` both before and
after it landed, because all 21 Zephyr cells were failing in `just zephyr setup`
on a missing Python package.

phase-395 built exactly this distinction for the merge queue — `queue-triage`'s
INFRA vs MINE. This is the same question one lane over, and the signal is
cheaper here because a job's failing STEP says which kind it is directly.

A cell that has been red for several consecutive runs is called out separately.
"Still red" and "newly red" are indistinguishable in the run list, and the
difference is the whole point: a standing red is a lane to repair, a new red is
a change to fix.

This never gates. It reports. A tool that could block the nightly on its own
opinion of the nightly is a worse failure mode than the one it describes.

Usage::

    nightly-triage.py                 # triage the most recent nightly
    nightly-triage.py --runs 5        # ...and flag cells red across 5 runs
    nightly-triage.py --selftest
"""

import argparse
import collections
import json
import os
import subprocess
import re
import sys

REPO = os.environ.get("NROS_QUEUE_REPO", "NEWSLabNTU/nano-ros")

# A step whose failure means the lane never reached the thing it tests. Matched
# case-insensitively as substrings against the step name, because the names are
# prose ("Set up Zephyr 3.7 workspace", "Reclaim disk before build").
#
# Deliberately a list of PREPARATION verbs rather than a list of known steps: a
# new provisioning step added next month should classify correctly without
# anyone remembering this file. The cost of that choice is stated in
# `classify`: a genuine build step whose name happens to contain one of these
# words would be misfiled, so the match is anchored to the step's ROLE words
# rather than to anything that could appear mid-sentence.
INFRA_MARKERS = (
    "set up", "setup", "provision", "install", "checkout", "cache",
    "reclaim disk", "register", "fetch", "unblock", "log in", "login",
    "free disk", "apt", "rustup", "submodule",
    # A host asserting it is what its labels claim. `runner-doctor.sh` exits 1
    # when the label lies, BEFORE the lane runs a thing — the textbook
    # no-verdict, and the one this tool was scoring as a real failure.
    "labels", "doctor",
)

# A step whose failure IS the verdict the lane exists to produce.
VERDICT_MARKERS = ("build", "test", "e2e", "check", "clippy", "run", "lint")


def classify(job):
    """('pass'|'verdict'|'no-verdict'|'cancelled', reason) for one job dict.

    `job` is {name, conclusion, steps: [{name, conclusion}]} — the shape
    `gh run view --json jobs` returns.
    """
    concl = job.get("conclusion")
    if concl == "success":
        return "pass", ""
    if concl in ("skipped", None):
        return "skipped", ""
    if concl == "cancelled":
        return "cancelled", ""

    failed = [s for s in job.get("steps", []) if s.get("conclusion") == "failure"]
    if not failed:
        # Red with no failed step: the runner or the container died. Nothing
        # was tested, so it is a no-verdict — and saying so is the point.
        return "no-verdict", "job failed with no failing step (runner/container)"

    step = failed[0].get("name", "") or ""
    low = step.lower()

    # WORD BOUNDARIES, not substrings. The comment above these lists says they
    # are chosen "rather than to anything that could appear mid-sentence", and
    # `in` broke that promise on the very first real case: `run` matched inside
    # `Verify this RUNner's labels are true`, so a host failing its own label
    # check — nothing built, nothing tested — was reported as a verdict failure.
    # This tool exists to separate exactly those two, so a substring hit here is
    # not a cosmetic bug: it makes the tool answer the opposite of its purpose.
    def hit(markers):
        return any(re.search(rf"\b{re.escape(m)}\b", low) for m in markers)

    # VERDICT wins ties. "Build nros CLI" contains both "build" and "install"-ish
    # language in places; a step that names the thing under test is a verdict
    # even when it also prepares something.
    if hit(VERDICT_MARKERS):
        return "verdict", step
    if hit(INFRA_MARKERS):
        return "no-verdict", step
    # Unmatched defaults to VERDICT on purpose: under-reporting a real failure
    # is worse than over-reporting one, and an unrecognised step name is a
    # reason to look, not to dismiss.
    return "verdict", step


def summarise(jobs):
    """Counts + the no-verdict list, for one run."""
    kinds = collections.Counter()
    no_verdict = []
    for j in jobs:
        kind, why = classify(j)
        kinds[kind] += 1
        if kind == "no-verdict":
            no_verdict.append((j.get("name", "?"), why))
    return kinds, no_verdict


def gh_json(args):
    try:
        out = subprocess.run(["gh"] + args, capture_output=True, text=True, timeout=180)
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


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--runs", type=int, default=3,
                    help="how many recent nightly runs to scan for standing reds")
    ap.add_argument("--workflow", default="nightly.yml")
    ap.add_argument("--selftest", action="store_true")
    args = ap.parse_args()

    if args.selftest:
        return selftest(verbose=True)
    selftest()

    runs = gh_json(["run", "list", "--repo", REPO, "--workflow", args.workflow,
                    "--limit", str(args.runs), "--json", "databaseId,createdAt,conclusion"])
    if not runs:
        print("no nightly runs found (or `gh` unavailable) — nothing to triage")
        return 0

    per_job_history = collections.defaultdict(list)
    for i, run in enumerate(runs):
        data = gh_json(["run", "view", str(run["databaseId"]), "--repo", REPO, "--json", "jobs"])
        jobs = (data or {}).get("jobs", [])
        for j in jobs:
            per_job_history[j.get("name", "?")].append(classify(j)[0])
        if i == 0:
            kinds, no_verdict = summarise(jobs)
            print(f"== nightly {run['createdAt'][:16]} — {len(jobs)} job(s) ==\n")
            print(f"  passed        {kinds['pass']}")
            print(f"  VERDICT fail  {kinds['verdict']}     <- real failures; fix the code")
            print(f"  NO VERDICT    {kinds['no-verdict']}     <- the lane never ran; fix the lane")
            print(f"  skipped       {kinds['skipped']}")
            if kinds["cancelled"]:
                print(f"  cancelled     {kinds['cancelled']}")
            if no_verdict:
                print("\n  cells that produced NO VERDICT:")
                by_step = collections.Counter(w for _, w in no_verdict)
                for step, n in by_step.most_common():
                    print(f"    {n:>3}x  failed at: {step}")
                # A handful of distinct steps across many cells still means one
                # fault — the Zephyr matrix stops at "Set up Zephyr 3.7" or
                # "…4.4" depending on the cell's line, which is two names for
                # one missing package.
                if len(by_step) <= 3 and kinds["no-verdict"] >= 3:
                    print(f"\n  {kinds['no-verdict']} cells stopped at {len(by_step)} distinct step(s).\n"
                          "  That is one infrastructure fault, not N broken cells — and while\n"
                          "  it stands, none of these cells can report a regression.")

    print()
    standing = {n: h for n, h in per_job_history.items()
                if len(h) >= 2 and all(k in ("verdict", "no-verdict") for k in h)}
    if standing:
        print(f"== red across all {len(runs)} scanned run(s): {len(standing)} cell(s) ==\n")
        for n, h in sorted(standing.items())[:20]:
            print(f"  {n[:58]:<58} {'/'.join(h)}")
        print("\n  A cell red for every run in the window is not reporting. Whatever\n"
              "  lands in it next is invisible — 'still red' and 'newly red' look the\n"
              "  same in the run list, which is how issue 0876 rode in.")
    else:
        print(f"== no cell is red across all {len(runs)} scanned run(s) ==")
    return 0


def selftest(verbose=False):
    """Prove the classifier can distinguish the two kinds. Runs every invocation."""
    ok = fail = 0

    def chk(desc, cond):
        nonlocal ok, fail
        if verbose or not cond:
            print(f"  {'ok   ' if cond else 'FAIL '} {desc}")
        ok += 1 if cond else 0
        fail += 0 if cond else 1

    def job(concl, *steps):
        return {"name": "cell", "conclusion": concl,
                "steps": [{"name": n, "conclusion": c} for n, c in steps]}

    chk("a green job passes",
        classify(job("success", ("Build", "success")))[0] == "pass")
    # The exact shape of issue 0878.
    chk("failure in `Set up Zephyr 3.7 workspace` is NO VERDICT",
        classify(job("failure", ("Checkout", "success"),
                     ("Set up Zephyr 3.7 workspace", "failure")))[0] == "no-verdict")
    chk("failure in a build step IS a verdict",
        classify(job("failure", ("Set up Zephyr 3.7 workspace", "success"),
                     ("Build zephyr/c/talker (zenoh) on 3.7", "failure")))[0] == "verdict")
    chk("failure in a test step IS a verdict",
        classify(job("failure", ("Test / e2e (nuttx)", "failure")))[0] == "verdict")
    chk("a red job with NO failing step is a no-verdict, not a silent pass",
        classify(job("failure"))[0] == "no-verdict")
    chk("`Build nros CLI` is a verdict even though it also provisions",
        classify(job("failure", ("Build nros CLI from packages/cli/", "failure")))[0] == "verdict")
    chk("`Reclaim disk before build` is NO VERDICT despite containing 'build'",
        classify(job("failure", ("Reclaim disk before Zephyr setup", "failure")))[0] == "no-verdict")
    chk("skipped is neither",
        classify(job("skipped"))[0] == "skipped")
    chk("cancelled is not counted as a verdict",
        classify(job("cancelled"))[0] == "cancelled")
    # The FIRST failing step is the one that matters: later steps fail because
    # the first did.
    chk("the FIRST failing step decides, not the last",
        classify(job("failure", ("Set up Zephyr workspace", "failure"),
                     ("Build zephyr/c/talker", "failure")))[0] == "no-verdict")

    # The case that motivated word boundaries — phase-413 W2. `run` matched
    # inside `runner`, so a host failing its own label check scored as a real
    # failure. Measured: this step is the ONLY failing step in every
    # `run-matrix` run to date and in the tier-2 nightly job.
    chk("`Verify this runner's labels are true` is NO VERDICT (not `run` in `runner`)",
        classify(job("failure", ("Verify this runner's labels are true", "failure")))[0]
        == "no-verdict")
    # The sharp one: this step has an INFRA marker ("set up") and the letters
    # `run` inside `runner`. Under substring matching the VERDICT list hit
    # first and won the tie, so an infra step was reported as a real failure.
    chk("an infra step containing `runner` is not stolen by the `run` marker",
        classify(job("failure", ("Set up the runner workspace", "failure")))[0]
        == "no-verdict")
    chk("`run` as its own word is still a verdict",
        classify(job("failure", ("just ci run matrix", "failure")))[0] == "verdict")
    # KNOWN LIMITATION, asserted so it is a decision and not a surprise: this
    # classifies by step NAME, so a `Test / e2e` step that failed with "0 ran,
    # 9 skipped" still reads as a verdict. Only the log knows the difference,
    # and `check-skip-budget` is what prints it ("ERROR: 9 skip(s) and NOT ONE
    # test actually ran"). Do not "fix" this by demoting test steps — that
    # would hide real failures to catch a reporting nuance.
    chk("a Test step is a verdict even when its failure was all-skips (see note)",
        classify(job("failure", ("Test / e2e (threadx_linux)", "failure")))[0] == "verdict")

    kinds, nv = summarise([
        job("failure", ("Set up Zephyr 3.7 workspace", "failure")),
        job("failure", ("Set up Zephyr 4.4 workspace", "failure")),
        job("success", ("Build", "success")),
    ])
    chk("summarise counts both kinds separately",
        kinds["no-verdict"] == 2 and kinds["pass"] == 1 and len(nv) == 2)

    if verbose:
        print(f"\n{ok} passed, {fail} failed")
    if fail:
        print("nightly-triage self-test: FAILED", file=sys.stderr)
        raise SystemExit(1)
    return 0


if __name__ == "__main__":
    sys.exit(main())

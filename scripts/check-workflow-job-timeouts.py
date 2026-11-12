#!/usr/bin/env python3
"""Every job in a merge-queue workflow declares `timeout-minutes`.

A job that declares no timeout inherits GitHub's 360 minutes. On 2026-11-12
`check-template-copy-out-zephyr` held a hosted runner for over two hours in
merge group run 43231846745. The queue dropped the entry when its own
timeout ran out, and the run kept going. One slow runner stalled every PR
queued behind it, with nothing in the log saying why. 17 of 17 jobs in
gate.yml and both jobs in queue.yml declared no timeout.

Scope is DERIVED: every workflow under `.github/workflows/` whose `on:`
includes `merge_group`. Those are the workflows whose jobs hold the queue.
Each job must state `timeout-minutes` as an integer between 1 and 300. A job
that calls a reusable workflow (`uses:`) is exempt, because GitHub rejects
the key there; the called workflow's own jobs carry it. Size a timeout from
the job's measured longest successful run (twice it, rounded up to five
minutes), and say so in a comment beside it.

A self-test runs on every invocation.
"""

import glob
import os
import sys

import yaml

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
CEILING = 300  # below GitHub's 360, so a stated value always binds


def triggers(doc):
    # PyYAML reads a bare `on:` key as the boolean True.
    on = doc.get("on", doc.get(True))
    if isinstance(on, str):
        return {on}
    if isinstance(on, list):
        return set(on)
    if isinstance(on, dict):
        return set(on)
    return set()


def problems_in(doc, name):
    out = []
    if "merge_group" not in triggers(doc):
        return None  # out of scope
    for job, body in (doc.get("jobs") or {}).items():
        if not isinstance(body, dict) or "uses" in body:
            continue
        t = body.get("timeout-minutes")
        if t is None:
            out.append(f"{name}: job `{job}` declares no timeout-minutes (inherits 360)")
        elif not isinstance(t, int) or isinstance(t, bool) or not 1 <= t <= CEILING:
            out.append(f"{name}: job `{job}` timeout-minutes {t!r} is not an integer in 1..{CEILING}")
    return out


def self_test():
    cases = [
        ("on: [merge_group]\njobs:\n  a:\n    runs-on: x\n    timeout-minutes: 20\n", 0),
        ("on: [merge_group]\njobs:\n  a:\n    runs-on: x\n", 1),
        ("on:\n  merge_group:\n  pull_request:\njobs:\n  a:\n    runs-on: x\n", 1),
        ("on: [merge_group]\njobs:\n  a:\n    runs-on: x\n    timeout-minutes: 400\n", 1),
        ("on: [merge_group]\njobs:\n  a:\n    runs-on: x\n    timeout-minutes: ${{ inputs.t }}\n", 1),
        ("on: [merge_group]\njobs:\n  a:\n    uses: ./.github/workflows/x.yml\n", 0),
        ("on: [pull_request]\njobs:\n  a:\n    runs-on: x\n", 0),  # not a queue workflow
    ]
    bad = 0
    for text, want in cases:
        got = problems_in(yaml.safe_load(text), "t.yml") or []
        if len(got) != want:
            print(f"  self-test FAIL: {text!r} -> {got}, want {want} problem(s)")
            bad += 1
    if bad:
        print(f"check-workflow-job-timeouts self-test: {bad} of {len(cases)} case(s) FAILED")
        return 1
    print(f"check-workflow-job-timeouts self-test: OK ({len(cases)} cases)")
    return 0


def main():
    if self_test() != 0:
        return 1
    files = sorted(glob.glob(os.path.join(ROOT, ".github", "workflows", "*.yml")))
    scoped, problems = 0, []
    for f in files:
        with open(f, encoding="utf8") as fh:
            doc = yaml.safe_load(fh)
        got = problems_in(doc or {}, os.path.relpath(f, ROOT))
        if got is None:
            continue
        scoped += 1
        problems += got
    if scoped == 0:
        print("check-workflow-job-timeouts: no workflow is triggered by merge_group -- "
              "the gate would pass vacuously", file=sys.stderr)
        return 1
    if problems:
        print("check-workflow-job-timeouts: a merge-queue job with no bound holds the queue "
              "for up to 6 h on a stalled runner:", file=sys.stderr)
        for p in problems:
            print(f"  {p}", file=sys.stderr)
        print("Add `timeout-minutes: <twice the longest successful run>` to each, with a "
              "comment naming the measurement.", file=sys.stderr)
        return 1
    print(f"check-workflow-job-timeouts: OK -- every job in {scoped} merge-queue workflow(s) has a timeout.")
    return 0


if __name__ == "__main__":
    sys.exit(main())

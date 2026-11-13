#!/usr/bin/env python3
"""Every job in every workflow declares `timeout-minutes`.

A job that declares no timeout inherits GitHub's 360 minutes. On 2026-11-12
`check-template-copy-out-zephyr` held a hosted runner for over two hours in
merge group run 43231846745. The queue dropped the entry when its own
timeout ran out, and the run kept going. One slow runner stalled every PR
queued behind it, with nothing in the log saying why. 17 of 17 jobs in
gate.yml and both jobs in queue.yml declared no timeout.

Scope was the merge-queue workflows until 2026-11-13. It is now every
workflow under `.github/workflows/`. A scheduled lane without a bound holds
its runner just as long, and on a self-hosted runner GitHub's own limit is
five days, so the queue's L3 waited behind one (647 minutes on 2026-11-12).

Each job must state `timeout-minutes` as an integer. On a hosted runner it
must be at most 355: GitHub ends a hosted job at 360, and a value at or
above that never binds. On a self-hosted runner it must be at most 720. A job
that calls a reusable workflow (`uses:`) is exempt, because GitHub rejects
the key there; the called workflow's own jobs carry it. Size a timeout from
the job's measured longest successful run (twice it, rounded up to five
minutes, with a floor of 10), and say so in a comment beside it.

A self-test runs on every invocation.
"""

import glob
import os
import sys

import yaml

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
CEILING_HOSTED = 355  # below GitHub's 360, so a stated value always binds
CEILING_SELF_HOSTED = 720


def self_hosted(body):
    r = body.get("runs-on")
    if isinstance(r, dict):
        r = r.get("labels")
    return "self-hosted" in str(r)


def problems_in(doc, name):
    out = []
    for job, body in (doc.get("jobs") or {}).items():
        if not isinstance(body, dict) or "uses" in body:
            continue
        t = body.get("timeout-minutes")
        ceiling = CEILING_SELF_HOSTED if self_hosted(body) else CEILING_HOSTED
        if t is None:
            out.append(f"{name}: job `{job}` declares no timeout-minutes (inherits 360)")
        elif not isinstance(t, int) or isinstance(t, bool) or not 1 <= t <= ceiling:
            out.append(f"{name}: job `{job}` timeout-minutes {t!r} is not an integer in 1..{ceiling}")
    return out


def self_test():
    cases = [
        ("on: [merge_group]\njobs:\n  a:\n    runs-on: x\n    timeout-minutes: 20\n", 0),
        ("on: [merge_group]\njobs:\n  a:\n    runs-on: x\n", 1),
        ("on:\n  merge_group:\n  pull_request:\njobs:\n  a:\n    runs-on: x\n", 1),
        ("on: [merge_group]\njobs:\n  a:\n    runs-on: x\n    timeout-minutes: 400\n", 1),
        ("on: [schedule]\njobs:\n  a:\n    runs-on: x\n    timeout-minutes: 360\n", 1),
        ("on: [schedule]\njobs:\n  a:\n    runs-on: [self-hosted, linux]\n    timeout-minutes: 400\n", 0),
        ("on: [schedule]\njobs:\n  a:\n    runs-on: [self-hosted, linux]\n    timeout-minutes: 800\n", 1),
        ("on: [merge_group]\njobs:\n  a:\n    runs-on: x\n    timeout-minutes: ${{ inputs.t }}\n", 1),
        ("on: [merge_group]\njobs:\n  a:\n    uses: ./.github/workflows/x.yml\n", 0),
        ("on: [pull_request]\njobs:\n  a:\n    runs-on: x\n", 1),  # every workflow is in scope
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
        print("check-workflow-job-timeouts: no workflow found -- "
              "the gate would pass vacuously", file=sys.stderr)
        return 1
    if problems:
        print("check-workflow-job-timeouts: a job with no bound holds its runner for up to "
              "6 h hosted, 5 days self-hosted:", file=sys.stderr)
        for p in problems:
            print(f"  {p}", file=sys.stderr)
        print("Add `timeout-minutes: <twice the longest successful run>` to each, with a "
              "comment naming the measurement.", file=sys.stderr)
        return 1
    print(f"check-workflow-job-timeouts: OK -- every job in {scoped} workflow(s) has a timeout.")
    return 0


if __name__ == "__main__":
    sys.exit(main())

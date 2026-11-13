#!/usr/bin/env python3
"""The merge queue's self-hosted runners are reserved for the merge queue.

A queue runner registers as `nros-queue,nros-sdk-zephyr,nros-big` and never with
`nros-qemu` (scripts/ci/runner-doctor.sh refuses either mistake). GitHub hands a
job only to a runner holding EVERY label the job requests. So the reservation
holds when both of these are true of the workflows, which is what this gate
checks:

  1. A job that requests `nros-queue` can only run on a queue event
     (`merge_group`, or a manual `workflow_dispatch`). Either its workflow has
     no other trigger, or its `runs-on` is an expression routed on
     `github.event_name` whose event list holds nothing else.
  2. Every self-hosted job in a workflow with a `schedule` trigger requests
     `nros-qemu`, unless its `runs-on` is event-routed away from `schedule`.
     A scheduled job without `nros-qemu` could be handed to a queue runner.

Before the split, one long scheduled run on the only capable runner held L3,
and L3's constant concurrency group then held every later queue run behind it
(647 minutes on 2026-11-12).

Scope is DERIVED: every workflow under `.github/workflows/`. A self-test runs on
every invocation.
"""

import glob
import os
import re
import sys

import yaml

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
QUEUE_LABEL = "nros-queue"
SCHEDULED_LABEL = "nros-qemu"
QUEUE_EVENTS = {"merge_group", "workflow_dispatch"}
ROUTED = re.compile(r"contains\(\s*fromJSON\(\s*'(\[[^']*\])'\s*\)\s*,\s*github\.event_name\s*\)")
LABEL = re.compile(r'"([A-Za-z0-9_.-]+)"|\b(self-hosted|nros-[a-z0-9-]+)\b')


def triggers(doc):
    # PyYAML reads a bare `on:` key as the boolean True.
    on = doc.get("on", doc.get(True))
    if isinstance(on, str):
        return {on}
    if isinstance(on, (list, dict)):
        return set(on)
    return set()


def runs_on(body):
    """(labels, routed_events or None) for a job's runs-on."""
    r = body.get("runs-on")
    if isinstance(r, list):
        return {str(x) for x in r}, None
    if isinstance(r, dict):
        return {str(x) for x in (r.get("labels") or [])}, None
    if not isinstance(r, str):
        return set(), None
    labels = {a or b for a, b in LABEL.findall(r)}
    m = ROUTED.search(r)
    events = set(yaml.safe_load(m.group(1))) if m else None
    return labels, events


def problems_in(doc, name):
    out = []
    on = triggers(doc)
    for job, body in (doc.get("jobs") or {}).items():
        if not isinstance(body, dict):
            continue
        labels, routed = runs_on(body)
        if "self-hosted" not in labels:
            continue
        reach = routed if routed is not None else on
        if QUEUE_LABEL in labels and not reach <= QUEUE_EVENTS:
            extra = ", ".join(sorted(reach - QUEUE_EVENTS))
            out.append(f"{name}: job `{job}` requests {QUEUE_LABEL} but can also run on: {extra}. "
                       f"Only the merge queue may use the queue runners.")
        if "schedule" in reach and SCHEDULED_LABEL not in labels:
            out.append(f"{name}: job `{job}` runs on `schedule` on a self-hosted runner without "
                       f"requesting {SCHEDULED_LABEL}, so a merge-queue runner could be handed it.")
    return out


def self_test():
    q = "[self-hosted, linux, nros-queue]"
    s = "[self-hosted, linux, nros-qemu, nros-big]"
    routed_q = ("${{ contains(fromJSON('[\"merge_group\",\"workflow_dispatch\"]'), github.event_name) "
                "&& fromJSON('[\"self-hosted\",\"linux\",\"nros-queue\"]') || 'ubuntu-22.04' }}")
    routed_bad = routed_q.replace('"workflow_dispatch"', '"schedule"')
    cases = [
        (f"on: [merge_group]\njobs:\n  a:\n    runs-on: {q}\n", 0),
        (f"on: [merge_group, workflow_dispatch]\njobs:\n  a:\n    runs-on: {q}\n", 0),
        (f"on: [merge_group, pull_request]\njobs:\n  a:\n    runs-on: {q}\n", 1),
        (f"on:\n  schedule:\n    - cron: '0 5 * * *'\njobs:\n  a:\n    runs-on: {q}\n", 2),
        (f"on:\n  schedule:\n    - cron: '0 5 * * *'\njobs:\n  a:\n    runs-on: {s}\n", 0),
        ("on: [schedule]\njobs:\n  a:\n    runs-on: [self-hosted, linux, nros-big]\n", 1),
        ("on: [schedule]\njobs:\n  a:\n    runs-on: ubuntu-22.04\n", 0),
        (f"on: [schedule, pull_request, merge_group]\njobs:\n  a:\n    runs-on: {routed_q}\n", 0),
        (f"on: [schedule, merge_group]\njobs:\n  a:\n    runs-on: {routed_bad}\n", 2),
        (f"on: [merge_group]\njobs:\n  a:\n    runs-on:\n      labels: {q}\n", 0),
    ]
    bad = 0
    for text, want in cases:
        got = problems_in(yaml.safe_load(text), "t.yml")
        if len(got) != want:
            print(f"  self-test FAIL: {text!r} -> {got}, want {want} problem(s)")
            bad += 1
    if bad:
        print(f"check-queue-runner-label self-test: {bad} of {len(cases)} case(s) FAILED")
        return 1
    print(f"check-queue-runner-label self-test: OK ({len(cases)} cases)")
    return 0


def main():
    if self_test() != 0:
        return 1
    files = sorted(glob.glob(os.path.join(ROOT, ".github", "workflows", "*.yml")))
    if not files:
        print("check-queue-runner-label: no workflows found -- the gate would pass vacuously",
              file=sys.stderr)
        return 1
    problems, self_hosted = [], 0
    for f in files:
        with open(f, encoding="utf8") as fh:
            doc = yaml.safe_load(fh) or {}
        for body in (doc.get("jobs") or {}).values():
            if isinstance(body, dict) and "self-hosted" in runs_on(body)[0]:
                self_hosted += 1
        problems += problems_in(doc, os.path.relpath(f, ROOT))
    if self_hosted == 0:
        print("check-queue-runner-label: no self-hosted job found -- the label parse is broken "
              "or the gate is obsolete", file=sys.stderr)
        return 1
    if problems:
        print("check-queue-runner-label: the merge queue's runners are not reserved for it:",
              file=sys.stderr)
        for p in problems:
            print(f"  {p}", file=sys.stderr)
        print("See docs/development/multi-agent-ci-workflow.md, \"The merge queue has its own "
              "runners\".", file=sys.stderr)
        return 1
    print(f"check-queue-runner-label: OK -- {self_hosted} self-hosted job(s) checked.")
    return 0


if __name__ == "__main__":
    sys.exit(main())

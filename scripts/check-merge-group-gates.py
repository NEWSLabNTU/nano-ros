#!/usr/bin/env python3
"""A gate the maintainers DECIDED must gate the merge queue still does — issue 1453.

Issue 1226's shape, one decision later: a gate that works is not a gate that
runs. `check-default-gates-run-somewhere` asks whether a gate reaches SOME event,
and a nightly placement satisfies it. That is the right bar for most gates and
the wrong one for a gate someone deliberately put on the merge queue: moving it
back to `schedule` keeps every existing gate green while undoing the decision,
and the decision was taken precisely because the nightly placement let three
copy-out templates sit unbuildable on `main` unseen (issue 1429).

So for each gate in `MERGE_GATING` below this asserts, against `gate.yml`:

  1. some job runs `just check <gate>` in a step whose `if:` admits
     `merge_group`, and the step is not `continue-on-error`;
  2. that job's own `if:` admits `merge_group`;
  3. the required aggregator `CI` lists the job in `needs:` and inspects it with
     a `sibling` line whose events include `merge_group` — a job the aggregator
     does not hear is decorative (R3 of `check-default-gates-run-somewhere`);
  4. no step, job or `sibling` line admits `pull_request` — the decision was
     "batch thorough", and a copy on the PR lane is a cost nobody agreed to.

AUTHORED, deliberately: the list is a record of decisions, and there is nothing
to derive a decision from. Each entry names the decision that put it here.

Usage::

    check-merge-group-gates.py [--selftest]
"""

import re
import sys
from pathlib import Path

import yaml

ROOT = Path(__file__).resolve().parent.parent
GATE_YML = ROOT / ".github" / "workflows" / "gate.yml"

# gate name -> the decision that makes it merge-gating.
MERGE_GATING = {
    "template-copy-out": "issue 1453 — maintainer decision 2026-10-17: the FULL "
                         "set on merge_group, not on pull_request",
}

EVENTS = ("pull_request", "merge_group", "push", "schedule", "workflow_dispatch")


def _events_of(guard, wf_events):
    """Events an `if:` admits. Only the `fromJSON('[…]'), github.event_name`
    spelling every gate.yml guard uses is UNDERSTOOD; anything else is read as
    admitting every workflow event, which can only make rule 4 fire — the safe
    direction for a gate whose job is to refuse a placement."""
    if guard is None or guard is True:
        return set(wf_events)
    g = str(guard)
    m = re.search(r"fromJSON\(\s*'\[([^\]]*)\]'\s*\)\s*,\s*github\.event_name", g)
    if not m:
        return set(wf_events)
    named = set(re.findall(r'"(' + "|".join(EVENTS) + r')"', m.group(1)))
    if re.search(r"!\s*\(?\s*contains\(\s*$", g[: m.start()]):
        return set(wf_events) - named
    return named


def _runs_gate(run, gate):
    return re.search(r"(?m)^\s*just\s+check\s+" + re.escape(gate) + r"(?![\w-])", run or "") is not None


def check(doc, text):
    errs = []
    on = doc.get("on", doc.get(True, {})) or {}
    wf_events = set(on) if isinstance(on, dict) else set(on or [])
    jobs = doc.get("jobs", {}) or {}
    agg_id = next((j for j, b in jobs.items() if (b or {}).get("name") == "CI"), None)
    if agg_id is None:
        return ["gate.yml has no job named `CI` — the required aggregator is gone"]
    agg = jobs[agg_id]
    needs = agg.get("needs", [])
    needs = [needs] if isinstance(needs, str) else list(needs or [])
    agg_text = "\n".join(s.get("run", "") for s in agg.get("steps", []) if isinstance(s, dict))
    for gate, why in MERGE_GATING.items():
        hosts = []
        for jid, job in jobs.items():
            for step in (job or {}).get("steps", []) or []:
                if isinstance(step, dict) and _runs_gate(step.get("run"), gate):
                    hosts.append((jid, job, step))
        if not hosts:
            errs.append(f"`just check {gate}` is run by no job in gate.yml ({why})")
            continue
        heard = False
        for jid, job, step in hosts:
            job_ev = _events_of(job.get("if"), wf_events)
            step_ev = _events_of(step.get("if"), job_ev) & job_ev
            where = f"job `{jid}`, step `{step.get('name', '?')}`"
            if "pull_request" in step_ev:
                errs.append(f"{where} runs `{gate}` on pull_request — the decision is "
                            f"merge_group, not the PR lane ({why})")
            if "merge_group" not in step_ev:
                continue
            if step.get("continue-on-error"):
                errs.append(f"{where} is continue-on-error, so its red cannot eject ({why})")
                continue
            if jid not in needs:
                errs.append(f"job `{jid}` runs `{gate}` on merge_group but is not in "
                            f"`{agg_id}`'s needs: — the required `CI` never hears it")
                continue
            m = re.search(r"^\s*sibling\s+" + re.escape(jid)
                          + r'\s+"\$\{\{\s*needs\.' + re.escape(jid)
                          + r'\.result\s*\}\}"\s+"([^"]*)"', agg_text, re.M)
            if not m:
                errs.append(f"`{agg_id}` has no `sibling {jid} …` line, so a skip of "
                            f"`{gate}` would read as green")
                continue
            sib_ev = set(m.group(1).split())
            if "merge_group" not in sib_ev:
                errs.append(f"`sibling {jid}` does not list merge_group — a skip there "
                            f"would be excused")
                continue
            if "pull_request" in sib_ev:
                errs.append(f"`sibling {jid}` lists pull_request, but `{gate}` must not "
                            f"run there ({why})")
                continue
            heard = True
        if not heard:
            errs.append(f"`{gate}` does not gate merge_group through the required `CI` ({why})")
    return errs


def load(path=GATE_YML):
    text = path.read_text()
    return yaml.safe_load(text), text


SAMPLE = """\
on:
  pull_request:
  merge_group:
  workflow_dispatch:
jobs:
  changes:
    runs-on: x
  check-template-copy-out:
    needs: changes
    if: >-
      ${{ always() && contains(fromJSON('["merge_group","workflow_dispatch"]'), github.event_name) }}
    steps:
      - name: gate
        if: ${{ contains(fromJSON('["merge_group","workflow_dispatch"]'), github.event_name) && !cancelled() }}
        run: |
          source ./activate.sh
          just check template-copy-out
  ci-ok:
    name: CI
    needs:
      - changes
      - check-template-copy-out
    steps:
      - run: |
          sibling check-template-copy-out "${{ needs.check-template-copy-out.result }}" "merge_group workflow_dispatch"
"""


def selftest():
    def run(text):
        return check(yaml.safe_load(text), text)

    cases = [
        ("the decided placement is clean", SAMPLE, False),
        ("the gate step removed", SAMPLE.replace("just check template-copy-out", "true"), True),
        ("the step narrowed to the nightly",
         SAMPLE.replace('if: ${{ contains(fromJSON(\'["merge_group","workflow_dispatch"]\'), github.event_name) && !cancelled() }}',
                        'if: ${{ contains(fromJSON(\'["schedule"]\'), github.event_name) }}'), True),
        ("the job dropped from CI's needs", SAMPLE.replace("      - check-template-copy-out\n", ""), True),
        ("the sibling line removed",
         "\n".join(l for l in SAMPLE.split("\n") if "sibling" not in l), True),
        ("the sibling excusing merge_group",
         SAMPLE.replace('"merge_group workflow_dispatch"\n', '"workflow_dispatch"\n'), True),
        ("moved onto the PR lane (job and step)",
         SAMPLE.replace('["merge_group","workflow_dispatch"]',
                        '["pull_request","merge_group","workflow_dispatch"]'), True),
        ("the sibling excusing nothing on the PR lane",
         SAMPLE.replace('"merge_group workflow_dispatch"\n', '"pull_request merge_group workflow_dispatch"\n'), True),
        ("made continue-on-error",
         SAMPLE.replace("        run: |\n          source", "        continue-on-error: true\n        run: |\n          source"), True),
        ("a longer gate name is not this gate",
         SAMPLE.replace("just check template-copy-out", "just check template-copy-out-extra"), True),
    ]
    bad = 0
    for desc, text, want_err in cases:
        got = bool(run(text))
        ok = got == want_err
        bad += not ok
        print(f"  {'ok  ' if ok else 'FAIL'} {desc}")
    if bad:
        print(f"check-merge-group-gates: selftest FAILED ({bad} case(s))", file=sys.stderr)
        return 1
    print(f"check-merge-group-gates: selftest OK ({len(cases)} cases)")
    return 0


def main():
    # The negative control runs on the NORMAL path too: a control nobody runs
    # decays into a comment (`check-gate-selftests`).
    rc = selftest()
    if "--selftest" in sys.argv[1:] or rc:
        return rc
    doc, text = load()
    errs = check(doc, text)
    if errs:
        print("check-merge-group-gates: a gate DECIDED to be merge-gating no longer is:",
              file=sys.stderr)
        for e in errs:
            print(f"  - {e}", file=sys.stderr)
        print("  Restore the placement, or record a NEW decision by editing MERGE_GATING in "
              "scripts/check-merge-group-gates.py (with its issue).", file=sys.stderr)
        return 1
    print(f"check-merge-group-gates: OK — {len(MERGE_GATING)} decided gate(s) gate merge_group "
          f"through `CI`, none on pull_request")
    return 0


if __name__ == "__main__":
    sys.exit(main())

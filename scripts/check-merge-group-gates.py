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
     "batch thorough", and a copy on the PR lane is a cost nobody agreed to;
  5. ANY sharded step in the CI workflows (phase-489 W8: the step
     sets a `*_SHARD` variable) takes its shard from a matrix that lists
     exactly `1..n` for the `<k>/<n>` it spells. Each shard runs only its
     share, so a matrix that drops an entry, or a literal `2/3`, quietly drops
     work while every shard reads green. This rule covers every job in
     `SHARD_WORKFLOWS`, not just the merge-gating ones.
  6. a merge-gating gate split into PARTS (issue 2219: a step sets a
     `*_PART=<x>` variable) has, among the steps that run it on
     `merge_group`, the complement of every part it names: `zephyr` needs
     `no-zephyr` and the reverse. The parts are complements by construction,
     so a missing side is a whole class of units nobody builds. A part listed
     in `DEFERRED_PARTS` is the one exception: a recorded decision moved it
     off the merge queue, so its complement is satisfied by the NAMED job in
     the NAMED workflow running it on a DAILY `schedule` cron — and the part
     must then not run anywhere in `gate.yml`, or the record is stale.

     Phase-559 W7 changed this rule. It used to be "heard by `CI` on
     `schedule`", with the part a `CI` sibling on `gate.yml`'s cron; W7 took
     that cron away, and a job outside `gate.yml` cannot be a `CI` sibling at
     all, so the deferred part's evidence is now its own job's placement.
     The old placement (the part in `gate.yml`, on any event) is refused.

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
# Workflows whose sharded jobs rule 5 checks.
SHARD_WORKFLOWS = [GATE_YML, ROOT / ".github" / "workflows" / "host-tests.yml"]

# gate name -> the decision that makes it merge-gating.
MERGE_GATING = {
    "template-copy-out": "issue 1453 — maintainer decision 2026-10-17: the FULL "
                         "set on merge_group, not on pull_request",
}

# (gate, part) -> (workflow file, job id that runs it daily, the decision).
# A part here still runs EVERY DAY in that job; it is the merge-queue
# placement that was decided away, never the coverage. Since phase-559 W7 the
# job is outside gate.yml (which has no `schedule`), so it is not a `CI`
# sibling: its verdict is a daily red on `main`, not a blocked merge.
DEFERRED_PARTS = {
    ("template-copy-out", "zephyr"): (
        "nightly.yml", "template-copy-out-zephyr",
        'user ruling 2026-11-13: "Do Zephyr template check daily" — the zephyr '
        "part was the merge queue's critical path (median 37 min vs ~21)"),
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


_PART_SET = re.compile(r"_PART=\s*(?:\"([^\"]*)\"|(\S+))")


def _parts_of(step):
    vals = [q or b for q, b in _PART_SET.findall(step.get("run") or "")]
    for key, val in (step.get("env") or {}).items():
        if str(key).endswith("_PART"):
            vals.append(str(val))
    return vals


def _complement(part):
    return part[3:] if part.startswith("no-") else "no-" + part


_SHARD_SET = re.compile(r"_SHARD=\s*(?:\"([^\"]*)\"|(\S+))")
_MATRIX_SPEC = re.compile(r"^\$\{\{\s*matrix\.(\w+)\s*\}\}/(\d+)$")


def _shard_errors(job, step, where):
    """Rule 5: every `*_SHARD=<k>/<n>` the step sets is `${{ matrix.X }}/n`
    over a matrix whose X is exactly 1..n."""
    errs = []
    specs = [q or b for q, b in _SHARD_SET.findall(step.get("run") or "")]
    for key, val in (step.get("env") or {}).items():
        if str(key).endswith("_SHARD"):
            specs.append(str(val))
    matrix = ((job.get("strategy") or {}).get("matrix")) or {}
    for spec in specs:
        m = _MATRIX_SPEC.match(spec)
        if not m:
            errs.append(f"{where} shards the gate with `{spec}`, not `${{{{ matrix.<x> }}}}/<n>` — "
                        f"a fixed shard builds only its share and nothing builds the rest")
            continue
        axis, n = m.group(1), int(m.group(2))
        vals = matrix.get(axis) if isinstance(matrix, dict) else None
        if not isinstance(vals, list) or sorted(str(v) for v in vals) != sorted(str(k) for k in range(1, n + 1)):
            errs.append(f"{where} spells shard `<k>/{n}` but `matrix.{axis}` is {vals!r}, not 1..{n} — "
                        f"the missing shards' units are built by nobody")
    return errs


def _runs_gate(run, gate):
    return re.search(r"(?m)^\s*just\s+check\s+" + re.escape(gate) + r"(?![\w-])", run or "") is not None


_DAILY_CRON = re.compile(r"^\s*\d{1,2}\s+\d{1,2}\s+\*\s+\*\s+\*\s*$")


def _on_of(doc):
    on = doc.get("on", doc.get(True, {})) or {}
    return on if isinstance(on, dict) else {e: None for e in (on or [])}


def deferred_errors(doc, wf_name, jid, gate, part):
    """Rule 6, deferred side: why `wf_name`'s job `jid` does NOT run `gate`
    part `part` on a daily schedule ([] when it does)."""
    if doc is None:
        return [f"{wf_name} does not exist"]
    on = _on_of(doc)
    crons = [str((c or {}).get("cron", "")) for c in (on.get("schedule") or [])
             if isinstance(c, dict)]
    if not crons:
        return [f"{wf_name} has no `schedule` trigger"]
    if not any(_DAILY_CRON.match(c) for c in crons):
        return [f"{wf_name}'s cron(s) {crons} are not daily (`M H * * *`)"]
    job = (doc.get("jobs") or {}).get(jid)
    if not isinstance(job, dict):
        return [f"{wf_name} has no job `{jid}`"]
    wf_events = set(on)
    job_ev = _events_of(job.get("if"), wf_events)
    if "schedule" not in job_ev:
        return [f"{wf_name} job `{jid}`'s `if:` does not admit schedule"]
    if job.get("continue-on-error"):
        return [f"{wf_name} job `{jid}` is continue-on-error, so its red reports nothing"]
    for step in job.get("steps") or []:
        if not (isinstance(step, dict) and _runs_gate(step.get("run"), gate)
                and part in _parts_of(step)):
            continue
        if "schedule" not in (_events_of(step.get("if"), job_ev) & job_ev):
            return [f"{wf_name} job `{jid}`, step `{step.get('name', '?')}` does not run on schedule"]
        if step.get("continue-on-error"):
            return [f"{wf_name} job `{jid}`, step `{step.get('name', '?')}` is continue-on-error"]
        return []
    return [f"{wf_name} job `{jid}` has no step running `just check {gate}` with part `{part}`"]


def load_deferred():
    """{workflow file: parsed doc or None} for every DEFERRED_PARTS workflow."""
    out = {}
    for wf, _jid, _why in DEFERRED_PARTS.values():
        path = ROOT / ".github" / "workflows" / wf
        out[wf] = yaml.safe_load(path.read_text()) if path.is_file() else None
    return out


def check(doc, text, deferred_docs=None):
    deferred_docs = deferred_docs or {}
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
        parts = set()
        for jid, job, step in hosts:
            job_ev = _events_of(job.get("if"), wf_events)
            step_ev = _events_of(step.get("if"), job_ev) & job_ev
            where = f"job `{jid}`, step `{step.get('name', '?')}`"
            if "pull_request" in step_ev:
                errs.append(f"{where} runs `{gate}` on pull_request — the decision is "
                            f"merge_group, not the PR lane ({why})")
            for part in _parts_of(step):
                deferred = DEFERRED_PARTS.get((gate, part))
                if deferred is None:
                    continue
                dwf, djid, dwhy = deferred
                errs.append(f"{where} runs part `{part}` in gate.yml, but DEFERRED_PARTS "
                            f"placed it in {dwf} job `{djid}`, daily ({dwhy}) — drop the "
                            f"record or the gate.yml placement")
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
            parts.update(_parts_of(step))
        for p in sorted(parts):
            c = _complement(p)
            if c not in parts and (gate, c) in DEFERRED_PARTS:
                dwf, djid, dwhy = DEFERRED_PARTS[(gate, c)]
                why_not = deferred_errors(deferred_docs.get(dwf), dwf, djid, gate, c)
                if why_not:
                    errs.append(f"`{gate}` part `{c}` is deferred to {dwf} job `{djid}`, daily "
                                f"({dwhy}), but {'; '.join(why_not)} — its units are built "
                                f"by nobody")
                continue
            if c not in parts:
                errs.append(f"`{gate}` runs part `{p}` on merge_group but no job runs part "
                            f"`{_complement(p)}` — that part's units are built by nobody ({why})")
        if not heard:
            errs.append(f"`{gate}` does not gate merge_group through the required `CI` ({why})")
    return errs


def shard_errors(doc, label):
    """Rule 5 over EVERY step of a workflow."""
    errs = []
    for jid, job in (doc.get("jobs", {}) or {}).items():
        for step in (job or {}).get("steps", []) or []:
            if isinstance(step, dict):
                errs.extend(_shard_errors(job, step, f"{label}: job `{jid}`, step `{step.get('name', '?')}`"))
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


SHARDED = SAMPLE.replace(
    "    needs: changes\n    if:",
    "    needs: changes\n    strategy:\n      matrix:\n        shard: [1, 2, 3]\n    if:",
).replace(
    "          just check template-copy-out\n",
    "          export NROS_TEMPLATE_COPY_OUT_SHARD=\"${{ matrix.shard }}/3\"\n"
    "          just check template-copy-out\n",
)
assert SHARDED != SAMPLE and "shard: [1, 2, 3]" in SHARDED


# Issue 2219: the same gate as two complementary parts, each its own job.
_PART_JOB = SAMPLE[SAMPLE.index("  check-template-copy-out:\n"):SAMPLE.index("  ci-ok:\n")]
PARTED = SAMPLE.replace(
    _PART_JOB,
    _PART_JOB.replace("          just check template-copy-out\n",
                      "          export NROS_TEMPLATE_COPY_OUT_PART=no-zephyr\n"
                      "          just check template-copy-out\n")
    + _PART_JOB.replace("check-template-copy-out:", "check-template-copy-out-zephyr:")
               .replace("          just check template-copy-out\n",
                        "          export NROS_TEMPLATE_COPY_OUT_PART=zephyr\n"
                        "          just check template-copy-out\n"),
).replace(
    "      - check-template-copy-out\n",
    "      - check-template-copy-out\n      - check-template-copy-out-zephyr\n",
).replace(
    '          sibling check-template-copy-out "${{ needs.check-template-copy-out.result }}" "merge_group workflow_dispatch"\n',
    '          sibling check-template-copy-out "${{ needs.check-template-copy-out.result }}" "merge_group workflow_dispatch"\n'
    '          sibling check-template-copy-out-zephyr "${{ needs.check-template-copy-out-zephyr.result }}" "merge_group workflow_dispatch"\n',
)
assert PARTED.count("_PART=") == 2 and "check-template-copy-out-zephyr.result" in PARTED

# The OLD (2026-11-13 to phase-559 W7) placement of the ruling: the zephyr
# part a `CI` sibling on gate.yml's own daily schedule. Refused since W7.
_Z = PARTED.index("  check-template-copy-out-zephyr:\n")
OLD_DEFERRED = (PARTED[:_Z] + PARTED[_Z:].replace('["merge_group","workflow_dispatch"]',
                                                  '["schedule","workflow_dispatch"]', 2)).replace(
    'sibling check-template-copy-out-zephyr "${{ needs.check-template-copy-out-zephyr.result }}" "merge_group workflow_dispatch"',
    'sibling check-template-copy-out-zephyr "${{ needs.check-template-copy-out-zephyr.result }}" "schedule workflow_dispatch"',
).replace("  merge_group:\n", "  merge_group:\n  schedule:\n    - cron: \"0 2 * * *\"\n")
assert OLD_DEFERRED.count('["schedule","workflow_dispatch"]') == 2

# The W7 placement: gate.yml runs the no-zephyr part alone on merge_group, and
# nightly.yml's named job runs the zephyr part on its daily cron.
DEFERRED = SAMPLE.replace("          just check template-copy-out\n",
                          "          export NROS_TEMPLATE_COPY_OUT_PART=no-zephyr\n"
                          "          just check template-copy-out\n")
assert DEFERRED.count("_PART=") == 1
NIGHTLY = """\
on:
  workflow_dispatch:
  schedule:
    - cron: "0 5 * * *"
jobs:
  template-copy-out-zephyr:
    runs-on: x
    steps:
      - name: gate
        if: ${{ !cancelled() }}
        run: |
          export NROS_TEMPLATE_COPY_OUT_PART=zephyr
          source ./activate.sh
          just check template-copy-out
"""


def selftest():
    def run(text, nightly=NIGHTLY):
        doc = yaml.safe_load(text)
        others = {"nightly.yml": yaml.safe_load(nightly) if nightly is not None else None}
        return check(doc, text, others) + shard_errors(doc, "sample")

    # A nightly.yml variant is checked against the W7 gate.yml (`DEFERRED`).
    N = "nightly:"

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
        ("sharded over the whole matrix", SHARDED, False),
        ("a shard dropped from the matrix", SHARDED.replace("shard: [1, 2, 3]", "shard: [1, 3]"), True),
        ("a fixed shard, no matrix", SHARDED.replace("${{ matrix.shard }}/3", "2/3"), True),
        ("the matrix shorter than the spec", SHARDED.replace("}}/3", "}}/4"), True),
        # Both parts on merge_group was the placement before 2026-11-13; with
        # the zephyr part recorded as deferred, it is a stale record now.
        ("both parts on merge_group against the deferral record", PARTED, True),
        ("one part dropped, and no deferred job to supply it",
         (PARTED.replace("export NROS_TEMPLATE_COPY_OUT_PART=zephyr\n", ""), None), True),
        ("the complement renamed away",
         PARTED.replace("NROS_TEMPLATE_COPY_OUT_PART=no-zephyr", "NROS_TEMPLATE_COPY_OUT_PART=host"), True),
        ("the zephyr part in nightly.yml's daily job, as ruled", DEFERRED, False),
        # phase-559 W7: the old placement is refused, with or without the new job.
        ("the OLD placement: the part a CI sibling on gate.yml's schedule", OLD_DEFERRED, True),
        ("the OLD placement, and no nightly.yml job", (OLD_DEFERRED, None), True),
        ("the deferred part's workflow missing", (DEFERRED, None), True),
        ("the deferred workflow has no schedule",
         N + NIGHTLY.replace('  schedule:\n    - cron: "0 5 * * *"\n', ""), True),
        ("the deferred workflow's cron is weekly",
         N + NIGHTLY.replace('"0 5 * * *"', '"0 5 * * 1"'), True),
        ("the deferred job renamed",
         N + NIGHTLY.replace("  template-copy-out-zephyr:", "  zephyr-other:"), True),
        ("the deferred job never runs on schedule",
         N + NIGHTLY.replace("    runs-on: x\n",
                         "    runs-on: x\n    if: ${{ contains(fromJSON('[\"workflow_dispatch\"]'), github.event_name) }}\n"), True),
        ("the deferred step lost its part",
         N + NIGHTLY.replace("          export NROS_TEMPLATE_COPY_OUT_PART=zephyr\n", ""), True),
        ("the deferred step made continue-on-error",
         N + NIGHTLY.replace("        run: |\n", "        continue-on-error: true\n        run: |\n"), True),
        ("the no-zephyr part may not be deferred",
         N + NIGHTLY.replace("_PART=zephyr", "_PART=no-zephyr"), True),
        ("a longer gate name is not this gate",
         SAMPLE.replace("just check template-copy-out", "just check template-copy-out-extra"), True),
    ]
    bad = 0
    for desc, text, want_err in cases:
        if isinstance(text, tuple):
            got = bool(run(*text))
        elif text.startswith(N):
            got = bool(run(DEFERRED, text[len(N):]))
        else:
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
    errs = check(doc, text, load_deferred())
    for wf in SHARD_WORKFLOWS:
        errs.extend(shard_errors(yaml.safe_load(wf.read_text()), wf.name))
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

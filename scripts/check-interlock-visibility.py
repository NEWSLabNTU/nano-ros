#!/usr/bin/env python3
"""An interlocked CI job must report that it did not run — phase-405.

THE DEFECT THIS GENERALISES

`post-submit.yml`'s tier-2 job and `queue.yml`'s L3 are gated on
`vars.NROS_SELF_HOSTED_READY`. The interlock is CORRECT: a required check that
can never start does not fail, it stays PENDING, and a merge queue waits for
pending forever.

But a SKIPPED job does not colour its run. Measured 2026-08-31: post-submit's
last several runs are `success` with `tier 2 (1-wise matrix)` skipped — so every
fixture build and every E2E cell in that lane had not run, and the run said
success. Meanwhile the one lane that does run E2E (`host-tests`) had been red
for 20 consecutive runs. Nothing anywhere said "the expensive lanes are not
running".

A lane that reports success for work it did not do is the same defect as a gate
that reports OK for a comparison it did not make. This repo has now fixed that
shape three times — `check-submodule-pins` (silent skip on an unresolvable
baseline), `check-gate-selftests` (a negative control nobody runs), and here.
So it gets a gate rather than a fourth fix.

WHAT IS REQUIRED

For every job gated on a `vars.*` interlock, the SAME workflow must contain a
job that:

  * `needs` the gated job, so it can see its result;
  * runs `if: always()`, so it reports on a skip (the whole point);
  * invokes `scripts/ci/report-interlock-coverage.sh` with that job's
    `needs.<job>.result`.

The reporter is shared on purpose. Two workflows spelling this themselves is
the "second idiom instead of a shared helper" that CLAUDE.md files under
#282 -> #326.

Buildless: parses the workflow YAML.
"""

import os
import re
import sys

try:
    import yaml
except ModuleNotFoundError:
    sys.exit("check-interlock-visibility: PyYAML missing (just dev-tools --install)")

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
WORKFLOWS = os.path.join(ROOT, ".github", "workflows")
REPORTER = "report-interlock-coverage.sh"
INTERLOCK = re.compile(r"vars\.([A-Z0-9_]+)")


def gated_jobs(doc):
    """{job_id: interlock_var} for every job whose `if:` tests a `vars.*`."""
    out = {}
    for jid, job in (doc.get("jobs") or {}).items():
        if not isinstance(job, dict):
            continue
        m = INTERLOCK.search(str(job.get("if", "")))
        if m:
            out[jid] = m.group(1)
    return out


def reporting_jobs(doc):
    """{reported_job_id: job_id} for every job that invokes the shared reporter."""
    out = {}
    for jid, job in (doc.get("jobs") or {}).items():
        if not isinstance(job, dict):
            continue
        body = " ".join(str(s.get("run", "")) for s in (job.get("steps") or [])
                        if isinstance(s, dict))
        if REPORTER not in body:
            continue
        if "always()" not in str(job.get("if", "")):
            continue
        for ref in re.findall(r"needs\.([A-Za-z0-9_-]+)\.result", body):
            out[ref] = jid
    return out


def check_doc(doc, name):
    problems = []
    reporters = reporting_jobs(doc)
    for jid, var in gated_jobs(doc).items():
        if jid in reporters:
            continue
        problems.append(
            f"{name}: job `{jid}` is gated on `vars.{var}` but no job reports "
            f"whether it ran. A skipped job does not colour its run, so this "
            f"workflow can report SUCCESS having executed none of it. Add a "
            f"job with `needs: [{jid}]`, `if: always()`, running "
            f"scripts/ci/{REPORTER} with ${{{{ needs.{jid}.result }}}}."
        )
    return problems


def selftest():
    """Exercise both verdicts on every run — phase-395."""
    bad = {"jobs": {
        "heavy": {"if": "${{ vars.NROS_SELF_HOSTED_READY == 'true' }}", "steps": []},
    }}
    assert check_doc(bad, "x"), "a gated job with no reporter must be a problem"

    good = {"jobs": {
        "heavy": {"if": "${{ vars.NROS_SELF_HOSTED_READY == 'true' }}", "steps": []},
        "coverage": {"needs": ["heavy"], "if": "always()", "steps": [
            {"run": f"./scripts/ci/{REPORTER} 'L' \"${{{{ needs.heavy.result }}}}\" x"}]},
    }}
    assert not check_doc(good, "x"), f"a reported job must pass: {check_doc(good,'x')}"

    # A reporter WITHOUT `if: always()` never runs on the skip it exists to
    # report — the exact shape that makes this whole check necessary, so it
    # must not satisfy it.
    lazy = {"jobs": {
        "heavy": {"if": "${{ vars.NROS_SELF_HOSTED_READY == 'true' }}", "steps": []},
        "coverage": {"needs": ["heavy"], "steps": [
            {"run": f"./scripts/ci/{REPORTER} 'L' \"${{{{ needs.heavy.result }}}}\" x"}]},
    }}
    assert check_doc(lazy, "x"), "a reporter without always() must not satisfy the rule"

    # An ungated job needs no reporter.
    assert not check_doc({"jobs": {"plain": {"steps": []}}}, "x")


def main():
    selftest()

    if not os.path.isfile(os.path.join(ROOT, "scripts", "ci", REPORTER)):
        sys.exit(f"check-interlock-visibility: scripts/ci/{REPORTER} is missing — "
                 "it is the shared reporter this rule requires.")

    problems, gated, files = [], 0, 0
    for fn in sorted(os.listdir(WORKFLOWS)):
        if not fn.endswith((".yml", ".yaml")):
            continue
        with open(os.path.join(WORKFLOWS, fn), encoding="utf-8") as fh:
            try:
                doc = yaml.safe_load(fh)
            except Exception as e:  # noqa: BLE001 — report, do not raise
                problems.append(f"{fn}: not valid YAML: {e}")
                continue
        if not isinstance(doc, dict):
            continue
        files += 1
        gated += len(gated_jobs(doc))
        problems += check_doc(doc, fn)

    if problems:
        sys.stderr.write("check-interlock-visibility: FAILED\n")
        for p in problems:
            sys.stderr.write(f"  {p}\n")
        return 1
    print(f"interlock visibility: OK ({gated} interlocked job(s) across {files} "
          f"workflow(s), each reported by scripts/ci/{REPORTER})")
    return 0


if __name__ == "__main__":
    sys.exit(main())

#!/usr/bin/env python3
"""Gate: the persistent fixture tree is used where it is safe, and only there.

phase-489 W6. The self-hosted fixture lanes (run-matrix tier 1 / tier 2, the
tier-2 nightly) build in a persistent checkout BESIDE the job's own, kept
across runs by `scripts/ci/fixture-tree.sh`. That only accelerates if every
piece lines up, and each piece fails silently when it does not:

R1  A job that syncs the tree runs on a SELF-HOSTED runner. A hosted VM has no
    tomorrow; the tree would be built cold every run and nothing would say so.

R2  Its `defaults.run.working-directory` is exactly TREE_WD — the path
    `fixture-tree.sh` derives (`tree_for`: a sibling of the checkout). A
    different spelling builds in a directory the script never synced, i.e. at
    a commit nobody chose.

R3  The sync step comes right after `actions/checkout` and runs in
    `${{ github.workspace }}` (the tree may not exist yet); NO OTHER step
    overrides `working-directory`, so nothing quietly runs in the cleaned
    checkout, which holds no build.

R4  Every `upload-artifact` path in the job is the per-run reports dir
    REPORTS, and an `if: always()` step running `fixture-tree.sh reports
    REPORTS` comes before the first upload. The tree KEEPS files, including an
    earlier run's junit and logs, so an upload read from the tree shipped them
    as this run's: nightly 40073092367 ran no test and uploaded a 3-day-old
    junit naming three failures (issue 1991). (Before that, R4 required paths
    rooted at `${{ env.NROS_FIXTURE_TREE }}/`, because upload paths resolve
    against the cleaned workspace; REPORTS is absolute too.)

R5  Never in a MERGE-GATING job: no job in a workflow triggered by
    `pull_request` / `merge_group`, and no job running
    `self-hosted-clean-tree.sh`. Gates start clean (phase-489 W2, issues
    0196 / 1476); a persistent tree there is the warm-runner false pass.

R6  The other direction: every self-hosted job that runs `just build <scope>`
    outside a merge-gating workflow builds in the tree. A new self-hosted
    fixture lane that forgets it is silently cold again — the state W6 found.
"""
from __future__ import annotations

import pathlib
import subprocess
import sys

ROOT = pathlib.Path(__file__).resolve().parent.parent
sys.path.insert(0, str(ROOT / "scripts" / "lib"))
from population import require_population  # noqa: E402

try:
    import yaml
except ImportError:
    print("check-fixture-tree-persistence: FAILED — PyYAML is missing, so no workflow\n"
          "  was read. Install it: `just dev-tools --install`.", file=sys.stderr)
    sys.exit(1)

GATE = "check-fixture-tree-persistence"
SYNC = "scripts/ci/fixture-tree.sh sync"
TREE_WD = "${{ github.workspace }}/../nros-fixture-tree"
SYNC_WD = "${{ github.workspace }}"
REPORTS = "${{ runner.temp }}/nros-run-reports"
COLLECT = f'scripts/ci/fixture-tree.sh reports "{REPORTS}"'
GATING_EVENTS = {"pull_request", "pull_request_target", "merge_group"}


def _events(doc):
    on = doc.get("on", doc.get(True))  # PyYAML reads a bare `on:` as True
    if isinstance(on, str):
        return {on}
    if isinstance(on, list):
        return set(on)
    if isinstance(on, dict):
        return set(on)
    return set()


def _self_hosted(job):
    ro = job.get("runs-on")
    if isinstance(ro, list):
        return "self-hosted" in ro
    if isinstance(ro, str):
        return "self-hosted" in ro
    return False


def _run(step):
    r = step.get("run")
    return r if isinstance(r, str) else ""


def job_problems(wf, name, job, gating):
    """Problems for one job; [] when it does not touch the tree and needn't."""
    steps = job.get("steps") or []
    syncs = [i for i, s in enumerate(steps) if SYNC in _run(s)]
    builds = any("just build " in _run(s) for s in steps)
    cleans = any("self-hosted-clean-tree.sh" in _run(s) for s in steps)
    where = f"{wf}: job `{name}`"
    out = []
    if not syncs:
        if builds and _self_hosted(job) and not gating and not cleans:
            out.append(f"{where} (R6): a self-hosted job runs `just build` in the checkout "
                       f"actions/checkout cleans, so every run is cold. Sync the tree "
                       f"(`{SYNC}`) and run in it.")
        return out
    if gating or cleans:
        out.append(f"{where} (R5): a merge-gating job may not build in a persistent tree "
                   f"— gates start clean (phase-489 W2, issues 0196/1476).")
    if not _self_hosted(job):
        out.append(f"{where} (R1): syncs the persistent tree on a non-self-hosted runner; "
                   f"a hosted VM keeps nothing, so it would be cold every run.")
    wd = ((job.get("defaults") or {}).get("run") or {}).get("working-directory")
    if wd != TREE_WD:
        out.append(f"{where} (R2): defaults.run.working-directory is {wd!r}, "
                   f"must be {TREE_WD!r} (where fixture-tree.sh puts the tree).")
    i = syncs[0]
    if len(syncs) > 1:
        out.append(f"{where} (R3): syncs the tree {len(syncs)} times.")
    if i == 0 or "actions/checkout" not in str(steps[i - 1].get("uses", "")):
        out.append(f"{where} (R3): the sync step must directly follow actions/checkout.")
    if steps[i].get("working-directory") != SYNC_WD:
        out.append(f"{where} (R3): the sync step must run in {SYNC_WD!r} — the tree may "
                   f"not exist yet.")
    collected = False
    for k, s in enumerate(steps):
        if COLLECT in _run(s) and k > i:
            collected = True
            if "always()" not in str(s.get("if", "")):
                out.append(f"{where} (R4): step {s.get('name', k)!r} collects the run's "
                           f"reports without `if: always()`, so a red step before it "
                           f"uploads nothing.")
        if k == i:
            continue
        if k < i and _run(s):
            out.append(f"{where} (R3): step {s.get('name', k)!r} runs before the tree is "
                       f"synced.")
        if "working-directory" in s:
            out.append(f"{where} (R3): step {s.get('name', k)!r} overrides "
                       f"working-directory, so it does not run in the tree.")
        if "upload-artifact" in str(s.get("uses", "")):
            paths = str((s.get("with") or {}).get("path", "")).split("\n")
            if not collected:
                out.append(f"{where} (R4): upload {s.get('name', k)!r} has no "
                           f"`{COLLECT}` step before it, so nothing separates this "
                           f"run's reports from an earlier run's (issue 1991).")
            for p in (p.strip() for p in paths):
                if p and p.rstrip("/") != REPORTS:
                    out.append(f"{where} (R4): upload path {p!r} is not {REPORTS!r}: "
                               f"read from the tree it ships an earlier run's files "
                               f"as this run's (issue 1991), and a relative path "
                               f"resolves against the cleaned checkout.")
    return out


def workflow_problems(wf, text):
    doc = yaml.safe_load(text) or {}
    gating = bool(_events(doc) & GATING_EVENTS)
    probs, n = [], 0
    for name, job in (doc.get("jobs") or {}).items():
        if not isinstance(job, dict):
            continue
        n += 1
        probs += job_problems(wf, name, job, gating)
    return probs, n


def scan(files):
    probs, jobs, users = [], 0, 0
    for f in files:
        text = f.read_text()
        p, n = workflow_problems(f.name, text)
        probs += p
        jobs += n
        users += text.count(SYNC)
    return probs, jobs, users


GOOD = """
on: {schedule: [{cron: "0 6 * * *"}], workflow_dispatch: {}}
jobs:
  t:
    runs-on: [self-hosted, linux]
    defaults:
      run:
        shell: bash
        working-directory: ${{ github.workspace }}/../nros-fixture-tree
    steps:
      - uses: actions/checkout@v4
      - name: tree
        working-directory: ${{ github.workspace }}
        run: scripts/ci/fixture-tree.sh sync
      - run: just build tier1
      - name: collect
        if: always()
        run: scripts/ci/fixture-tree.sh reports "${{ runner.temp }}/nros-run-reports"
      - uses: actions/upload-artifact@v4
        with:
          path: ${{ runner.temp }}/nros-run-reports/
"""


def self_test():
    bad = 0

    def expect(label, text, rule):
        nonlocal bad
        probs, _ = workflow_problems("t.yml", text)
        hit = any(f"({rule})" in p for p in probs) if rule else not probs
        print(f"  {'ok  ' if hit else 'FAIL'} {label}")
        if not hit:
            bad += 1
            for p in probs:
                print(f"         {p}")

    expect("the shipped shape passes", GOOD, None)
    expect("hosted runner refused", GOOD.replace("[self-hosted, linux]", "ubuntu-22.04"), "R1")
    expect("wrong working dir refused",
           GOOD.replace("/../nros-fixture-tree", "/../other-tree"), "R2")
    expect("sync not after checkout refused",
           GOOD.replace("      - name: tree\n", "      - run: echo hi\n      - name: tree\n"), "R3")
    expect("a step escaping the tree refused",
           GOOD.replace("      - run: just build tier1\n",
                        "      - run: just build tier1\n        working-directory: .\n"), "R3")
    expect("unrooted upload path refused",
           GOOD.replace("path: ${{ runner.temp }}/nros-run-reports/", "path: build/**/*.log"),
           "R4")
    expect("an upload read from the tree refused (issue 1991)",
           GOOD.replace("path: ${{ runner.temp }}/nros-run-reports/",
                        "path: ${{ env.NROS_FIXTURE_TREE }}/target/nextest/**/junit*.xml"),
           "R4")
    expect("an upload with no collect step refused",
           GOOD.replace("      - name: collect\n        if: always()\n"
                        "        run: scripts/ci/fixture-tree.sh reports "
                        "\"${{ runner.temp }}/nros-run-reports\"\n", ""), "R4")
    expect("a collect step without if: always() refused",
           GOOD.replace("      - name: collect\n        if: always()\n",
                        "      - name: collect\n"), "R4")
    expect("merge-gating workflow refused",
           GOOD.replace("workflow_dispatch: {}", "merge_group: {}"), "R5")
    expect("clean-tree job refused",
           GOOD.replace("      - run: just build tier1\n",
                        "      - run: scripts/ci/self-hosted-clean-tree.sh\n"
                        "      - run: just build tier1\n"), "R5")
    cold = GOOD.replace("      - name: tree\n        working-directory: ${{ github.workspace }}\n"
                        "        run: scripts/ci/fixture-tree.sh sync\n", "")
    expect("a self-hosted lane building cold refused", cold, "R6")
    expect("a hosted lane building cold is not this gate's business",
           cold.replace("[self-hosted, linux]", "ubuntu-22.04")
               .replace("        working-directory: ${{ github.workspace }}/../nros-fixture-tree\n", ""),
           None)
    print(f"{GATE} self-test: {'ok' if not bad else f'{bad} FAILED'}")
    return bad == 0


def main():
    if "--self-test" in sys.argv:
        return 0 if self_test() else 1
    if not self_test():
        return 1
    # The script's own negative controls, on the normal path: build output
    # kept, tracked edits undone, an unchanged source keeps its mtime across a
    # pull, a tree inside the checkout refused, an over-age tree goes cold.
    rc = subprocess.run(["bash", str(ROOT / "scripts" / "ci" / "fixture-tree.sh"),
                         "--self-test"]).returncode
    if rc != 0:
        print(f"{GATE}: FAILED — scripts/ci/fixture-tree.sh --self-test", file=sys.stderr)
        return 1
    files = sorted((ROOT / ".github" / "workflows").glob("*.yml"))
    probs, jobs, users = scan(files)
    if not require_population(jobs, "workflow job(s)", gate=GATE):
        return 1
    if not require_population(users, "persistent-fixture-tree sync step(s)", gate=GATE):
        return 1
    if probs:
        print(f"{GATE}: FAILED", file=sys.stderr)
        for p in probs:
            print(f"  {p}", file=sys.stderr)
        return 1
    print(f"{GATE}: ok")
    return 0


if __name__ == "__main__":
    sys.exit(main())

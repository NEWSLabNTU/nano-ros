#!/usr/bin/env python3
"""Refuse a release whose commit the slow lanes have not passed (phase-567 W1).

A release is cut only from a commit that every slow lane passed, or from a
descendant of one whose every change since is documentation. "Every" is the
user's rule (any red nightly blocks a release), so the TOOL enforces it: every
scheduled workflow is either a lane below or named in NOT_LANES with a reason,
and the self-test fails a scheduled workflow that is neither.
"Documentation" is `scripts/ci/gate-tier.py`'s own `tier()` predicate,
imported, never a second spelling (the changelog audit uses the same one).

  LANES     nightly    nightly.yml, the whole run's conclusion
            tier 1     run-matrix.yml, job `tier 1 (cells)`
            tier 2     run-matrix.yml, job `tier 2 (1-wise matrix)`
            host-tests host-tests.yml, the whole run's conclusion
            live-peer  live-peer.yml, the whole run's conclusion
  COVERAGE  every `.github/workflows/*.yml` with a `schedule:` trigger is in
            LANES or in NOT_LANES (with its reason), never both, and every
            NOT_LANES entry names a scheduled workflow that exists.
            Scheduled runs on `main` only, as `record-lane-runs.py` reads them:
            a dispatch may narrow what a lane runs, and a cadence is what a
            release is measured against.
  QUALIFIES a run whose head IS the commit, or is an ancestor of it with only
            documentation changed since (by `git diff --name-only`). So this
            needs the full history; `release-nros.yml` checks out with
            `fetch-depth: 0` for it.
  WHICH     on a release branch, `release-branch-frozen.py` (W2) names the
            commits: its first cut and each cherry-pick's source on main. The
            lanes run on main, never on a release branch, so a fix is measured
            where it landed; the release head itself has no lane runs.
  PASSES    a lane passes when ANY qualifying run of it is green. A refusal
            names the lane, the newest run it read and why that run did not
            count (red, not an ancestor, or code changed since).

  release-lanes-green.py --repo OWNER/NAME --sha <sha> [--sha …] [--shas-from FILE]
                         every commit named must pass (needs `gh`, GH_TOKEN)
  release-lanes-green.py --self-test                     (`just check release-lanes-green`)

Every invocation runs the self-test first (check-gate-selftests), and a
release is refused if it fails: a broken refusal would let anything through.
"""

from __future__ import annotations

import argparse
import importlib.util
import json
import os
import re
import subprocess
import sys
import tempfile
from pathlib import Path

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE.parent / "lib"))
from git_hook_env import nros_clear_inherited_git_env  # noqa: E402
from git_history import UNKNOWN, ancestry  # noqa: E402  (a NO from a shallow clone is not a verdict)

# (name, workflow file, job name or None for the whole run)
LANES = (
    ("nightly", "nightly.yml", None),
    ("tier 1", "run-matrix.yml", "tier 1 (cells)"),
    ("tier 2", "run-matrix.yml", "tier 2 (1-wise matrix)"),
    ("host-tests", "host-tests.yml", None),
    ("live-peer", "live-peer.yml", None),
)
# Scheduled workflows that are NOT a release lane, each with why. A new
# scheduled workflow must land in LANES or here: `coverage()` refuses one that
# is in neither, so a new nightly cannot silently fall outside the release rule.
NOT_LANES = {
    "runner-watchdog.yml": "infrastructure: polls the self-hosted runners every 15 min and "
                           "tests no commit",
    "automation.yml": "repository plumbing (phase-559 W8): the pull-request verdict sweep and "
                      "the monthly roadmap audit judge no code",
    "weekly.yml": "a WEEKLY advisory cadence (Sunday, user ruling 2026-11-13) that gates "
                  "nothing; the release rule is about the nightly cadence",
}
WORKFLOWS = HERE.parent.parent / ".github" / "workflows"
LIMIT = 30


def _gate_tier():
    spec = importlib.util.spec_from_file_location("gate_tier", HERE.parent / "ci" / "gate-tier.py")
    mod = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(mod)
    return mod


GATE_TIER = _gate_tier()


def gh_json(args):
    r = subprocess.run(["gh", *args], capture_output=True, text=True)
    if r.returncode != 0:
        raise RuntimeError(f"gh {' '.join(args)}: {r.stderr.strip()}")
    return json.loads(r.stdout)


def git(args, cwd):
    return subprocess.run(["git", *args], cwd=cwd, capture_output=True, text=True)


def relation(head, sha, cwd, included):
    """Why a run on `head` does or does not stand for `sha`: (ok, reason)."""
    if head == sha:
        return True, "the release commit itself"
    a = ancestry(cwd, head, sha)
    if a is UNKNOWN:
        return False, (f"cannot tell whether {head[:10]} is an ancestor of {sha[:10]} "
                       "(truncated history: release-nros.yml checks out with fetch-depth 0)")
    if not a:
        return False, f"{head[:10]} is not an ancestor of {sha[:10]}"
    p = git(["diff", "--name-only", head, sha], cwd)
    if p.returncode != 0:
        return False, f"git diff {head[:10]} {sha[:10]} failed: {p.stderr.strip()}"
    paths = [x for x in p.stdout.splitlines() if x]
    t, why = GATE_TIER.tier(paths, included)
    if t != "docs":
        return False, f"code changed since {head[:10]}: {why}"
    return True, f"{len(paths)} documentation path(s) since {head[:10]}"


def scheduled_workflows(wf_dir):
    """File names of the workflows in `wf_dir` that a `schedule:` event triggers."""
    out = set()
    for f in sorted(Path(wf_dir).glob("*.y*ml")):
        on = False
        for line in f.read_text().splitlines():
            if re.match(r"^(on|\"on\"|'on'):", line):
                on = True
                continue
            if on and line and not line[0].isspace() and not line.startswith("#"):
                on = False
            if on and re.match(r"^\s+schedule:\s*(#.*)?$", line):
                out.add(f.name)
    return out


def coverage(wf_dir, lanes=None, not_lanes=None):
    """Problems with the lane set's reach over the scheduled workflows: [str]."""
    lanes = LANES if lanes is None else lanes
    not_lanes = NOT_LANES if not_lanes is None else not_lanes
    lane_files = {wf for _, wf, _ in lanes}
    sched = scheduled_workflows(wf_dir)
    bad = []
    for wf in sorted(sched - lane_files - set(not_lanes)):
        bad.append(f"{wf} runs on a schedule and is neither a release lane (LANES) nor listed in "
                   "NOT_LANES with a reason, so a red run of it would not block a release")
    for wf in sorted(lane_files & set(not_lanes)):
        bad.append(f"{wf} is both a release lane and in NOT_LANES")
    for wf in sorted(set(not_lanes) - sched):
        bad.append(f"NOT_LANES names {wf}, which is not a scheduled workflow (stale entry)")
    for wf in sorted(lane_files - sched):
        bad.append(f"LANES reads {wf}, which is not a scheduled workflow, so it has no "
                   "scheduled run to read")
    for wf, why in not_lanes.items():
        if not why.strip():
            bad.append(f"NOT_LANES entry {wf} gives no reason")
    return bad


def lane_verdict(run, job, jobs_of):
    """`success`, `failure`, … for this lane in this run."""
    if job is None:
        return run.get("conclusion") or "none"
    for j in jobs_of(run["databaseId"]):
        if j.get("name") == job:
            return j.get("conclusion") or "none"
    return "absent"


def decide(sha, cwd, included, runs_of, jobs_of):
    """[(lane, ok, message)] over LANES."""
    out = []
    for name, wf, job in LANES:
        runs = runs_of(wf)
        newest = None   # the newest run read, when none stood for the commit
        red = None      # the newest run that stood for it and was not green
        hit = None
        for run in runs:  # newest first
            ok, why = relation(run["headSha"], sha, cwd, included)
            if not ok:
                newest = newest or (run, why)
                continue
            v = lane_verdict(run, job, jobs_of)
            if v == "success":
                hit = (run, why)
                break
            red = red or (run, f"{name} was `{v}` there ({why})")
        newest = red or newest
        if hit:
            run, why = hit
            out.append((name, True, f"{name}: green in run {run['databaseId']} on "
                                    f"{run['headSha'][:10]} ({why})"))
        elif newest:
            run, why = newest
            out.append((name, False, f"{name}: no green run on {sha[:10]} or a docs-only ancestor "
                                     f"in the last {len(runs)} scheduled {wf} runs; newest read: "
                                     f"run {run['databaseId']} on {run['headSha'][:10]}: {why}"))
        else:
            out.append((name, False, f"{name}: no scheduled {wf} run on main to read"))
    return out


def live(repo):
    cache = {}

    def runs_of(wf):
        if wf not in cache:
            runs = gh_json(["run", "list", "--repo", repo, "--workflow", wf, "--branch", "main",
                            "--event", "schedule", "--status", "completed", "--limit", str(LIMIT),
                            "--json", "databaseId,conclusion,headSha,createdAt"])
            cache[wf] = sorted(runs, key=lambda r: r.get("createdAt", ""), reverse=True)
        return cache[wf]

    jobs = {}

    def jobs_of(rid):
        if rid not in jobs:
            jobs[rid] = gh_json(["run", "view", str(rid), "--repo", repo, "--json", "jobs"])["jobs"]
        return jobs[rid]

    return runs_of, jobs_of


# ------------------------------------------------------------------ self-test

def self_test():
    env = nros_clear_inherited_git_env(dict(os.environ))
    env.update(GIT_AUTHOR_NAME="t", GIT_AUTHOR_EMAIL="t@t", GIT_COMMITTER_NAME="t",
               GIT_COMMITTER_EMAIL="t@t", GIT_CONFIG_NOSYSTEM="1")
    saved = dict(os.environ)
    os.environ.clear()
    os.environ.update(env)
    fails = []
    try:
        with tempfile.TemporaryDirectory() as t:
            r = Path(t)

            def g(*a):
                p = git(list(a), r)
                if p.returncode != 0:
                    raise RuntimeError(p.stderr)
                return p.stdout.strip()

            g("init", "-q", "-b", "main")

            def commit(path, text):
                f = r / path
                f.parent.mkdir(parents=True, exist_ok=True)
                f.write_text(text)
                g("add", path)
                g("commit", "-q", "-m", path)
                return g("rev-parse", "HEAD")

            a = commit("packages/core/a.rs", "1")          # code: the lanes ran here
            b = commit("book/src/guide/a.md", "2")          # docs-only past a
            c = commit("packages/core/c.rs", "3")           # code past b
            g("checkout", "-q", "-b", "side", a)
            side = commit("packages/core/s.rs", "4")        # not an ancestor of b
            g("checkout", "-q", "main")
            read_by_rust = {"book/src/guide/a.md"}

            def run(i, head, concl="success"):
                return {"databaseId": i, "headSha": head, "conclusion": concl, "createdAt": f"{i:04d}"}

            def scenario(nightly, matrix, jobs, daily):
                def runs_of(wf):
                    pick = {"nightly.yml": nightly, "run-matrix.yml": matrix}.get(wf)
                    if pick is None:
                        pick = daily.get(wf, [run(5, a)])
                    return sorted(pick, key=lambda x: x["createdAt"], reverse=True)
                return runs_of, lambda rid: jobs[rid]

            green = [{"name": "tier 1 (cells)", "conclusion": "success"},
                     {"name": "tier 2 (1-wise matrix)", "conclusion": "success"}]
            red2 = [{"name": "tier 1 (cells)", "conclusion": "success"},
                    {"name": "tier 2 (1-wise matrix)", "conclusion": "failure"}]

            def verdict(sha, nightly, matrix, jobs, included=frozenset(), daily=None):
                rs, js = scenario(nightly, matrix, jobs, daily or {})
                return {n: (ok, m) for n, ok, m in decide(sha, str(r), set(included), rs, js)}

            ALL = {"nightly", "tier 1", "tier 2", "host-tests", "live-peer"}
            none = {"host-tests.yml": [], "live-peer.yml": []}
            cases = [
                ("every lane green on the commit itself", a,
                 [run(1, a)], [run(2, a)], {2: green}, {}, set()),
                ("one docs-only commit past a green run", b,
                 [run(1, a)], [run(2, a)], {2: green}, {}, set()),
                ("no green nightly: the only nightly is red", b,
                 [run(1, a, "failure")], [run(2, a)], {2: green}, {}, {"nightly"}),
                ("no nightly that stands for the commit (a side branch)", b,
                 [run(1, side)], [run(2, a)], {2: green}, {}, {"nightly"}),
                ("red tier 2 is refused even with a green nightly", b,
                 [run(1, a)], [run(2, a)], {2: red2}, {}, {"tier 2"}),
                ("code changed since the green run", c,
                 [run(1, a)], [run(2, a)], {2: green}, {}, ALL),
                ("a book page Rust reads is not documentation", b,
                 [run(1, a)], [run(2, a)], {2: green}, read_by_rust, ALL),
                ("an older green run counts when the newest is red", b,
                 [run(3, b, "failure"), run(1, a)], [run(2, a)], {2: green}, {}, set()),
                ("no run at all", a, [], [], {}, {}, ALL, none),
                ("a red host-tests blocks with every other lane green", b,
                 [run(1, a)], [run(2, a)], {2: green}, {}, {"host-tests"},
                 {"host-tests.yml": [run(6, a, "failure")]}),
                ("a red live-peer blocks with every other lane green", b,
                 [run(1, a)], [run(2, a)], {2: green}, {}, {"live-peer"},
                 {"live-peer.yml": [run(6, a, "failure")]}),
            ]
            for name, sha, nightly, matrix, jobs, inc, want_refused, *daily in cases:
                v = verdict(sha, nightly, matrix, jobs, inc, daily[0] if daily else None)
                refused = {n for n, (ok, _) in v.items() if not ok}
                if refused != want_refused:
                    fails.append(f"{name}: want refused {sorted(want_refused)}, got {sorted(refused)}")
            v = verdict(b, [run(7, a, "failure")], [run(2, a)], {2: green})
            if "run 7" not in v["nightly"][1] or "`failure`" not in v["nightly"][1]:
                fails.append(f"a refusal does not name the run and its verdict: {v['nightly'][1]}")
            v = verdict(b, [run(9, side), run(8, a, "failure")], [run(2, a)], {2: green})
            if "run 8" not in v["nightly"][1]:
                fails.append(f"a refusal names a run that does not stand for the commit over a red one "
                             f"that does: {v['nightly'][1]}")
            v = verdict(c, [run(1, a)], [run(2, a)], {2: green})
            if "code changed since" not in v["nightly"][1]:
                fails.append(f"a refusal does not say code changed: {v['nightly'][1]}")
        # coverage: synthetic workflow dirs, then the real tree
        with tempfile.TemporaryDirectory() as t:
            d = Path(t)
            (d / "lane.yml").write_text("name: lane\non:\n  schedule:\n    - cron: '0 3 * * *'\n")
            (d / "infra.yml").write_text("name: infra\non:\n  workflow_dispatch:\n  schedule:  # x\n"
                                         "    - cron: '*/15 * * * *'\n")
            (d / "pr.yml").write_text("name: pr\non:\n  pull_request:\njobs:\n  j:\n"
                                      "    schedule:\n")  # a `schedule:` outside `on:` is no trigger
            lanes = (("l", "lane.yml", None),)
            if coverage(d, lanes, {"infra.yml": "polls runners"}):
                fails.append(f"coverage refuses a covered set: {coverage(d, lanes, {'infra.yml': 'r'})}")
            got = coverage(d, lanes, {})
            if len(got) != 1 or "infra.yml" not in got[0]:
                fails.append(f"coverage misses an unlisted scheduled workflow: {got}")
            got = coverage(d, lanes, {"infra.yml": "r", "gone.yml": "r"})
            if len(got) != 1 or "stale" not in got[0]:
                fails.append(f"coverage misses a stale NOT_LANES entry: {got}")
            got = coverage(d, lanes, {"infra.yml": "r", "lane.yml": "r"})
            if not any("both" in x for x in got):
                fails.append(f"coverage misses a workflow in both tables: {got}")
            if coverage(d, lanes, {"infra.yml": " "}) == []:
                fails.append("coverage accepts a NOT_LANES entry with no reason")
        if WORKFLOWS.is_dir():
            fails += [f"this tree: {x}" for x in coverage(WORKFLOWS)]
    finally:
        os.environ.clear()
        os.environ.update(saved)
    for f in fails:
        print(f"release-lanes-green self-test FAIL: {f}", file=sys.stderr)
    if fails:
        return 1
    print("release-lanes-green self-test: OK (exact commit, docs-only descendant, red nightly, "
          "side branch, red tier 2, code since, Rust-read page, older green, no runs, red host-tests, "
          "red live-peer; coverage: covered, unlisted, stale, both, no reason, this tree)")
    return 0


def main(argv):
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    ap.add_argument("--repo")
    ap.add_argument("--sha", action="append", default=[])
    ap.add_argument("--shas-from", help="a file of commits, one per line")
    ap.add_argument("--self-test", action="store_true")
    a = ap.parse_args(argv)
    rc = self_test()
    if a.self_test or rc != 0:
        if rc != 0 and not a.self_test:
            print("::error::release-lanes-green: its self-test fails, so it cannot vouch for "
                  "this commit; the release is refused")
        return rc
    want = list(a.sha)
    if a.shas_from:
        want += [x.strip() for x in Path(a.shas_from).read_text().splitlines() if x.strip()]
    if not a.repo or not want:
        ap.error("--repo and at least one --sha (or --shas-from) are required")
    nros_clear_inherited_git_env()
    cwd = os.getcwd()
    included = GATE_TIER.included_markdown()
    lanes = live(a.repo)
    refused = []
    for w in dict.fromkeys(want):
        sha = git(["rev-parse", f"{w}^{{commit}}"], cwd).stdout.strip() or w
        try:
            rows = decide(sha, cwd, included, *lanes)
        except RuntimeError as e:
            print(f"::error::release-lanes-green: could not read the lanes ({e}); the release is refused")
            return 1
        print(f"== {sha[:10]}")
        for _, ok, msg in rows:
            print(("OK      " if ok else "::error::REFUSED ") + msg)
        if not all(ok for _, ok, _ in rows):
            refused.append(sha[:10])
    if not refused:
        print("release-lanes-green: every commit passed every slow lane (phase-567 W1)")
        return 0
    print(f"release-lanes-green: refused {', '.join(refused)}: a release cuts only from commits every "
          f"slow lane ({', '.join(n for n, _, _ in LANES)}) passed, or docs-only descendants of "
          "them (phase-567 W1)")
    return 1


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))

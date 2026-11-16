#!/usr/bin/env python3
"""A red lane names its suspects: the commits merged since its last green run.

Phase-567 W6. A red nightly, tier-1 or tier-2 run says THAT something broke and
nothing about WHAT. The question a reader asks next is "what landed since this
lane was last green", and that was answered by hand: find the last green run,
read its head commit, list the commits between, and look up each one's pull
request. This is that procedure, once, for `nightly-triage.py` and for
`lane-stage.py --history` (`just matrix-triage`).

A LANE is a workflow, or one job of a workflow (`run-matrix.yml` holds the tier-1
and tier-2 lanes as two jobs). Its last green run is the newest earlier run on
`main` in which the lane passed: the workflow's conclusion, or that job's.

The commits come from GitHub's compare API, not from `git log`: a CI checkout
is shallow, so the green commit is often not in it. Each commit carries:

  * its pull request (the one the merge queue merged it from), and
  * its author SESSION, the `Claude-Session:` trailer every agent commit carries,
    which is how a session finds the commits it is answerable for.

What it refuses to guess:

  * no earlier green run in the window: it says so, and lists NOTHING. Listing
    all history would name every commit as a suspect, which names none;
  * the green and red heads are the same commit: no commit is a suspect, and the
    red is not a change (a flaky or infrastructure red, or a re-run);
  * more commits than it looks up: it lists the first ones and says how many it
    left out.

Never gates: it reports, like the two tools that call it.

    lane_suspects.py --workflow nightly.yml            # the latest run
    lane_suspects.py --workflow run-matrix.yml --job "tier 1 (cells)" --run <id>
    lane_suspects.py --selftest
"""

import argparse
import json
import os
import re
import subprocess
import sys

REPO = os.environ.get("NROS_QUEUE_REPO", "NEWSLabNTU/nano-ros")
BRANCH = "main"
# How far back a red lane looks for its last green run, and how many suspects it
# looks up a pull request for (one API call each).
GREEN_WINDOW = 30
PR_LOOKUP_CAP = 60

SESSION = re.compile(r"^Claude-Session:\s*(\S+)\s*$", re.M)
PR_SUFFIX = re.compile(r"\(#(\d+)\)\s*$")


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


# --------------------------------------------------------------------------
# Pure core: everything below takes data, so the self-test can drive it.
# --------------------------------------------------------------------------

def last_green(runs, red_id, passed):
    """The newest run OLDER than `red_id` for which `passed(run)` is True.

    `runs` is newest first (as `gh run list` returns them). Returns
    (run, scanned) where run is None when the window holds no green run.
    """
    seen_red, scanned = False, 0
    for run in runs:
        if not seen_red:
            seen_red = run.get("databaseId") == red_id
            continue
        scanned += 1
        if passed(run):
            return run, scanned
    return None, scanned


def suspect_rows(compare, pr_of):
    """[(sha, subject, pr, session)] from a compare API answer, oldest first.

    `pr_of(sha, message)` gives the pull request number or None.
    """
    rows = []
    for c in (compare or {}).get("commits", []):
        msg = (c.get("commit") or {}).get("message", "")
        sha = c.get("sha", "")
        m = SESSION.search(msg)
        rows.append((sha, msg.split("\n", 1)[0], pr_of(sha, msg), m.group(1) if m else None))
    return rows


def render(lane, red, green, scanned, rows, total, window=GREEN_WINDOW):
    """The suspect block, as lines."""
    out = [f"== suspects for {lane} (red run {red.get('databaseId')}, "
           f"{(red.get('headSha') or '?')[:10]}) =="]
    if green is None:
        out.append(f"  no green run of this lane in the {scanned} earlier run(s) scanned "
                   f"(window {window}), so no commit range can be named.")
        out.append("  Listing all history would name every commit; it is not listed.")
        return out
    gsha, rsha = green.get("headSha") or "", red.get("headSha") or ""
    out.append(f"  last green: run {green.get('databaseId')} "
               f"({(green.get('createdAt') or '')[:16].replace('T', ' ')}, {gsha[:10]})")
    if gsha == rsha:
        out.append("  the green and the red run are the SAME commit: no commit is a suspect.")
        out.append("  This red is not a change. Read it as flaky or infrastructure.")
        return out
    prs = sorted({pr for _s, _t, pr, _x in rows if pr})
    out.append(f"  {total} commit(s) merged since, oldest first"
               + (f", from {len(prs)} PR(s) in the {len(rows)} looked up" if rows else "") + ":")
    for sha, subject, pr, session in rows:
        out.append(f"    {sha[:10]}  {('#' + str(pr)) if pr else '(no PR)':<8} {subject[:72]}")
        out.append(f"{'':16}session: {session or '(no Claude-Session trailer)'}")
    if total > len(rows):
        out.append(f"  ... and {total - len(rows)} more, newer, not listed: the oldest are closest "
                   f"to the green run, and each listed commit costs one PR lookup (cap {PR_LOOKUP_CAP}).")
    return out


# --------------------------------------------------------------------------
# The gh-backed half.
# --------------------------------------------------------------------------

def _runs(workflow, limit):
    return gh_json(["run", "list", "--repo", REPO, "--workflow", workflow,
                    "--branch", BRANCH, "--limit", str(limit),
                    "--json", "databaseId,createdAt,headSha,status,conclusion"]) or []


class JobCache:
    """`run id -> {job name: conclusion}`, read once per run: a red nightly has
    several red jobs and each looks back over the same runs."""

    def __init__(self):
        self._jobs = {}

    def __call__(self, run):
        rid = run["databaseId"]
        if rid not in self._jobs:
            data = gh_json(["run", "view", str(rid), "--repo", REPO, "--json", "jobs"]) or {}
            self._jobs[rid] = {j.get("name"): j.get("conclusion") for j in data.get("jobs", [])}
        return self._jobs[rid]


def job_passed(job, jobs_of):
    """A run is green for `job` when it ran that job and the job succeeded. A
    run that did not schedule the job is not green for it."""
    return lambda run: run.get("status") == "completed" and jobs_of(run).get(job) == "success"


def group_by_green(red_jobs, runs, red_id, jobs_of):
    """{green run id or None: (green run, scanned, [job, ...])}: the red jobs
    that share a last green run share one commit range, so one block."""
    out = {}
    for job in red_jobs:
        green, scanned = last_green(runs, red_id, job_passed(job, jobs_of))
        key = green["databaseId"] if green else None
        out.setdefault(key, [green, scanned, []])[2].append(job)
    return out


def _pr_of(sha, msg):
    m = PR_SUFFIX.search(msg.split("\n", 1)[0])
    if m:
        return int(m.group(1))
    pulls = gh_json(["api", f"repos/{REPO}/commits/{sha}/pulls"]) or []
    merged = [p for p in pulls if p.get("merged_at")] or pulls
    return merged[0]["number"] if merged else None


def _range(green, red):
    if green is None or green.get("headSha") == red.get("headSha"):
        return [], 0
    compare = gh_json(["api", f"repos/{REPO}/compare/{green['headSha']}...{red['headSha']}"]) or {}
    total = compare.get("total_commits", len(compare.get("commits", [])))
    return suspect_rows({"commits": compare.get("commits", [])[:PR_LOOKUP_CAP]}, _pr_of), total


def suspects(workflow, red_run=None, jobs=None):
    """The suspect blocks for a red run, as lines.

    The lane is a JOB: `jobs` names the lane jobs to answer for (default: every
    job that failed in the red run). A workflow whose whole run is never green
    (the nightly: one red cell in 26 makes the run red) would otherwise never
    name a range at all.
    """
    runs = _runs(workflow, GREEN_WINDOW + 1)
    if not runs:
        return [f"== suspects for {workflow}: no runs readable (is `gh` authenticated?) =="]
    red = next((r for r in runs if str(r.get("databaseId")) == str(red_run)), None) \
        if red_run else runs[0]
    if red is None:
        return [f"== suspects: run {red_run} is not among {workflow}'s last {len(runs)} on {BRANCH} =="]
    jobs_of = JobCache()
    red_jobs = jobs or sorted(n for n, c in jobs_of(red).items() if c == "failure")
    if not red_jobs:
        return [f"== suspects for {workflow} run {red['databaseId']}: no job failed, nothing to name =="]
    out = []
    for _key, (green, scanned, names) in sorted(group_by_green(red_jobs, runs, red["databaseId"],
                                                               jobs_of).items(),
                                                key=lambda kv: str(kv[0])):
        rows, total = _range(green, red)
        label = f"{workflow}: " + ", ".join(f"`{n}`" for n in names)
        out += render(label, red, green, scanned, rows, total) + [""]
    return out


# --------------------------------------------------------------------------
# Self-test: the acceptance, on recorded-shape data.
# --------------------------------------------------------------------------

def selftest(verbose=False):
    bad = 0

    def case(desc, ok):
        nonlocal bad
        if verbose or not ok:
            print(f"  {'ok  ' if ok else 'FAIL'} {desc}", file=sys.stderr if not ok else sys.stdout)
        bad += not ok

    wf_passed = lambda r: r.get("status") == "completed" and r.get("conclusion") == "success"
    runs = [  # newest first
        {"databaseId": 5, "headSha": "e" * 40, "conclusion": "failure", "status": "completed"},
        {"databaseId": 4, "headSha": "d" * 40, "conclusion": "failure", "status": "completed"},
        {"databaseId": 3, "headSha": "c" * 40, "conclusion": "success", "status": "completed"},
        {"databaseId": 2, "headSha": "b" * 40, "conclusion": "success", "status": "completed"},
    ]
    g, n = last_green(runs, 5, wf_passed)
    case("the last green is the newest EARLIER passing run, not an older one",
         g and g["databaseId"] == 3 and n == 2)
    case("the red run itself is never its own green",
         last_green([runs[2]], 3, wf_passed)[0] is None)
    g, _ = last_green(runs[:2], 5, wf_passed)
    case("no green in the window: none is invented", g is None)
    lines = render("nightly.yml", runs[0], None, 1, [], 0)
    case("no prior green says so and lists no commit",
         any("no green run" in l for l in lines) and not any("session:" in l for l in lines))

    compare = {"commits": [
        {"sha": "1" * 40, "commit": {"message": "fix(x): one (#11)\n\nbody\n\nClaude-Session: https://s/a\n"}},
        {"sha": "2" * 40, "commit": {"message": "feat(y): two\n\nno trailer\n"}},
    ]}
    rows = suspect_rows(compare, lambda sha, msg: 11 if sha.startswith("1") else 22)
    case("each commit carries its PR and its author session",
         rows == [("1" * 40, "fix(x): one (#11)", 11, "https://s/a"),
                  ("2" * 40, "feat(y): two", 22, None)])
    lines = render("nightly.yml", runs[0], runs[2], 2, rows, 2)
    listed = [l for l in lines if re.match(r"^    [0-9a-f]{10}  ", l)]
    case("exactly the commits between green and red are listed, in order",
         len(listed) == 2 and "#11" in listed[0] and "#22" in listed[1])
    case("a commit with no trailer says so instead of borrowing one",
         any("(no Claude-Session trailer)" in l for l in lines))
    same = dict(runs[2], databaseId=6)
    lines = render("nightly.yml", same, runs[2], 1, [], 0)
    case("green and red on one commit name no suspect",
         any("SAME commit" in l for l in lines)
         and not any(re.match(r"^    [0-9a-f]{10}  ", l) for l in lines))
    lines = render("nightly.yml", runs[0], runs[2], 2, rows, 7)
    case("a truncated list says how many it left out", any("5 more, newer, not listed" in l for l in lines))
    # The lane is a JOB: per-job last green, and red jobs sharing one green
    # share one block. Run 4 did not schedule `b` at all, which is not green.
    jobs = {5: {"a": "failure", "b": "failure", "c": "failure"},
            4: {"a": "success", "c": "failure"},
            3: {"a": "success", "b": "success", "c": "success"},
            2: {"a": "success", "b": "success", "c": "success"}}
    jobs_of = lambda run: jobs[run["databaseId"]]
    groups = group_by_green(["a", "b", "c"], runs, 5, jobs_of)
    case("each red job finds its own last green; a run without the job is not green for it",
         {k: v[2] for k, v in groups.items()} == {4: ["a"], 3: ["b", "c"]})
    case("a job never green in the window gets no range",
         group_by_green(["z"], runs, 5, lambda r: {}) == {None: [None, 3, ["z"]]})
    case("a PR number in the subject is read without an API call",
         _pr_of("x", "fix: thing (#4041)\n") == 4041)
    if bad:
        print("lane_suspects self-test: FAILED", file=sys.stderr)
    return 1 if bad else 0


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--workflow", default="nightly.yml")
    ap.add_argument("--job", action="append", default=None,
                    help="a lane job to answer for (repeatable; default: every job that failed)")
    ap.add_argument("--run", default=None, help="the red run (default: the latest)")
    ap.add_argument("--selftest", action="store_true")
    args = ap.parse_args()
    if args.selftest:
        return selftest(verbose=True)
    if selftest():
        return 1
    print("\n".join(suspects(args.workflow, args.run, args.job)))
    return 0


if __name__ == "__main__":
    sys.exit(main())

#!/usr/bin/env python3
"""Every gate lane runs somewhere, provisioned by its own claims (phase-565 W4).

A lane is a scope (`.config/gate-lanes.toml` `[lane.<name>]`), and a job runs
one by setting `NROS_GATE_SCOPE: <name>`. Three things are held here:

  1. every declared lane is the `NROS_GATE_SCOPE` of at least one workflow job,
     or names the LOCAL place it runs (`local = "..."`: `dev` is a worktree,
     `pristine` a step of `just ci gate`). A lane nothing runs is vocabulary
     with no subject, which is what `[lane.queue]` was until W4 deleted it;
  2. every job that sets a scope either runs the lane provisioner (the
     `provision-gate-lane` action, which runs the lane's claims through
     scripts/lib/claim-provision.sh) or says, in `NROS_GATE_PROVISIONED_BY`,
     what provides its claims instead;
  3. every `NROS_GATE_SCOPE` (a workflow's, or a script's or recipe's literal
     `NROS_GATE_SCOPE=<name>`) names a declared lane.

Run with --self-test for the negative controls (they also run first on the
normal path).
"""

import re
import subprocess
import sys
from pathlib import Path

import yaml

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / "scripts" / "lib"))
import lane_provision  # noqa: E402  the one reader of gate-lanes.toml

ACTION = "./.github/actions/provision-gate-lane"
LITERAL = re.compile(r"\bNROS_GATE_SCOPE=([A-Za-z0-9_-]+)")


def scoped_jobs(workflows):
    """(workflow, job, scope, provisioned, reason) for every job that sets a
    scope, at the job's or a step's `env`."""
    out = []
    for wf, doc in workflows:
        for name, job in ((doc or {}).get("jobs") or {}).items():
            env = job.get("env") or {}
            steps = job.get("steps") or []
            scope = env.get("NROS_GATE_SCOPE")
            for s in steps:
                scope = scope or (s.get("env") or {}).get("NROS_GATE_SCOPE")
            if not scope:
                continue
            provisioned = any((s.get("uses") or "").rstrip("/") == ACTION for s in steps)
            reason = env.get("NROS_GATE_PROVISIONED_BY") or ""
            out.append((wf, name, str(scope), provisioned, str(reason).strip()))
    return out


def problems(lanes, jobs, literals):
    """lanes: name -> lane table; jobs: scoped_jobs(); literals: (file, scope)."""
    out = []
    run = {scope for _, _, scope, _, _ in jobs}
    for name, decl in sorted(lanes.items()):
        if name not in run and not str(decl.get("local", "")).strip():
            out.append(f"lane `{name}` is the NROS_GATE_SCOPE of no job and names no `local` "
                       "place it runs: run it in a job, say where it runs, or delete it")
    for wf, job, scope, provisioned, reason in jobs:
        if scope not in lanes:
            out.append(f"{wf} job `{job}`: NROS_GATE_SCOPE `{scope}` is not a declared lane")
        elif not provisioned and not reason:
            out.append(f"{wf} job `{job}` runs lane `{scope}` without the provision-gate-lane "
                       "action and without NROS_GATE_PROVISIONED_BY saying what provides its claims")
    for f, scope in literals:
        if scope not in lanes:
            out.append(f"{f}: NROS_GATE_SCOPE={scope} is not a declared lane")
    return out


def self_test():
    bad = 0

    def case(desc, got, want_refused):
        nonlocal bad
        ok = bool(got) == want_refused
        print(f"  {'ok  ' if ok else 'FAIL'} {desc}: {'refused' if got else 'accepted'}")
        bad += not ok

    lanes = {"fast": {}, "runner": {}, "dev": {"local": "a worktree"}}
    good = [("gate.yml", "fast", "fast", True, ""), ("rm.yml", "t2", "runner", False, "just setup tier2")]
    case("every lane run, every job provisioned or explained", problems(lanes, good, []), False)
    # Negative controls: the three shapes W4 exists for.
    case("negative control: a lane no job runs and no local place names",
         problems({**lanes, "queue": {}}, good, []), True)
    case("negative control: a job with neither the action nor a reason",
         problems(lanes, good + [("x.yml", "j", "runner", False, "")], []), True)
    case("negative control: a job naming an undeclared lane",
         problems(lanes, good + [("x.yml", "j", "nope", True, "")], []), True)
    case("negative control: a script literal naming an undeclared lane",
         problems(lanes, good, [("s.sh", "queue")]), True)
    doc = {"jobs": {"a": {"env": {"NROS_GATE_SCOPE": "fast"},
                          "steps": [{"uses": ACTION}]},
                    "b": {"steps": [{"env": {"NROS_GATE_SCOPE": "runner"}}]}}}
    got = scoped_jobs([("w.yml", doc)])
    case("a step-level scope is found, and the action counts as provisioning",
         [] if got == [("w.yml", "a", "fast", True, ""), ("w.yml", "b", "runner", False, "")] else ["x"],
         False)
    if bad:
        print("check-gate-lane-jobs self-test: FAILED", file=sys.stderr)
        return 1
    return 0


def main(argv):
    if argv == ["--self-test"]:
        return self_test()
    import contextlib
    import io
    buf = io.StringIO()
    with contextlib.redirect_stdout(buf):
        rc = self_test()
    if rc:
        sys.stderr.write(buf.getvalue())
        return 1
    lanes = lane_provision.load().get("lane", {})
    wfs = []
    for p in sorted((ROOT / ".github" / "workflows").glob("*.yml")):
        wfs.append((p.name, yaml.safe_load(p.read_text())))
    files = subprocess.run(["git", "ls-files", "-z", "*.sh", "*.just", "justfile", "*.py"],
                           cwd=ROOT, capture_output=True, check=True).stdout.decode().split("\0")
    literals = []
    for f in filter(None, files):
        if f == "scripts/check/check-gate-lane-jobs.py":
            continue
        try:
            text = (ROOT / f).read_text(encoding="utf-8")
        except (OSError, UnicodeDecodeError):
            continue
        literals += [(f, m) for m in LITERAL.findall(text)]
    found = problems(lanes, scoped_jobs(wfs), literals)
    if found:
        print("check-gate-lane-jobs: FAILED (phase-565 W4)", file=sys.stderr)
        for p in found:
            print("  " + p, file=sys.stderr)
        return 1
    jobs = scoped_jobs(wfs)
    print(f"check-gate-lane-jobs: OK ({len(lanes)} lanes, {len(jobs)} scoped jobs, each lane run "
          "somewhere and each job provisioned or explained)")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))

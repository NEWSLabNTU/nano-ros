#!/usr/bin/env python3
"""A CI job that runs a gate lane provides what that lane claims (issue 2382).

`.config/gate-lanes.toml` declares, per lane, the capabilities its gates may
assume, and the lane preflight refuses a host that lacks one. Some claims are
the image's (cmake, a compiler, ROS); four must be FETCHED by the job itself:
`submodules`, `git-history`, `zephyr-tree` and `arm-gcc` (issue 2413). `scripts/ci/provision-gate-lane.sh
<lane>` fetches them, from the lane's own declaration.

Issue 2382 is what happens without this rule: the `build` lane claimed
submodules and history, the hosted jobs that run it (gate.yml's build-tier
shards, host-tests' tier-1 gates) fetched neither, and from phase-543 W5/W6
every one of them stopped in the preflight with no gate run, for three nights,
while every check on the pull requests stayed green.

So: a job that sets `NROS_GATE_SCOPE: <lane>` (on the job or on a step), where
the lane claims a fetched capability, runs `provision-gate-lane.sh <lane>` in
a step before the first step that runs the lane.

Buildless, milliseconds. The self-test runs on the normal path.
"""
from __future__ import annotations

import re
import sys
from pathlib import Path

import yaml

try:
    import tomllib
except ModuleNotFoundError:
    import tomli as tomllib

REPO = Path(__file__).resolve().parents[2]
FETCHED = {"submodules", "git-history", "zephyr-tree", "arm-gcc"}


def fetched_claims(lanes: dict, lane: str) -> set[str] | None:
    decl = lanes.get(lane)
    if decl is None:
        return None
    return FETCHED & set(decl.get("claims", []))


PROVISION_ACTION = "./.github/actions/provision-gate-lane"


def check_workflow(name: str, wf: dict, lanes: dict) -> list[str]:
    problems = []
    for job_id, job in (wf.get("jobs") or {}).items():
        steps = job.get("steps") or []
        job_lane = (job.get("env") or {}).get("NROS_GATE_SCOPE")
        provided: set[str] = set()
        first_use: dict[str, int] = {}
        for i, step in enumerate(steps):
            run = step.get("run") or ""
            for m in re.finditer(r"provision-gate-lane\.sh\s+([A-Za-z0-9_-]+)", run):
                provided.add(m.group(1))
            # The composite that wraps the script (`.github/actions/provision-gate-lane`).
            if str(step.get("uses", "")) == PROVISION_ACTION:
                lane_in = (step.get("with") or {}).get("lane")
                if lane_in:
                    provided.add(str(lane_in))
            lane = (step.get("env") or {}).get("NROS_GATE_SCOPE")
            if lane and lane not in first_use:
                first_use[lane] = i
                # A step-level lane must be provided by a step BEFORE it.
                if lane not in provided:
                    first_use[lane] = -1 - i  # mark: used before provided
        if job_lane:
            first_use.setdefault(job_lane, 0)
        for lane, at in first_use.items():
            claims = fetched_claims(lanes, lane)
            if claims is None:
                problems.append(f"  {name} job `{job_id}`: NROS_GATE_SCOPE `{lane}` is no "
                                "[lane.*] in .config/gate-lanes.toml")
                continue
            if not claims:
                continue
            step_level_unprovided = at < 0
            job_level_unprovided = lane == job_lane and lane not in provided
            if step_level_unprovided or job_level_unprovided:
                problems.append(
                    f"  {name} job `{job_id}`: runs lane `{lane}`, which claims "
                    f"{', '.join(sorted(claims))}, with no `provision-gate-lane` step (lane: "
                    f"{lane}) before it (issue 2382: its preflight then refuses and no gate runs)")
    return problems


def self_test(lanes: dict) -> None:
    def wf(*steps, job_env=None):
        job = {"steps": list(steps)}
        if job_env:
            job["env"] = job_env
        return {"jobs": {"j": job}}
    prov = {"run": "bash scripts/ci/provision-gate-lane.sh build"}
    use = {"run": "just check build", "env": {"NROS_GATE_SCOPE": "build"}}
    assert check_workflow("t", wf(prov, use), lanes) == []
    comp = {"uses": PROVISION_ACTION, "with": {"lane": "build"}}
    assert check_workflow("t", wf(comp, use), lanes) == [], "the composite did not count"
    assert check_workflow("t", wf(use, comp), lanes), "the composite AFTER the lane passed"
    assert check_workflow("t", wf({"uses": PROVISION_ACTION, "with": {"lane": "fast"}}, use),
                          lanes), "the composite for another lane passed"
    assert check_workflow("t", wf(use), lanes), "an unprovisioned build lane passed"
    assert check_workflow("t", wf(use, prov), lanes), "provisioning AFTER the lane passed"
    wrong = {"run": "bash scripts/ci/provision-gate-lane.sh fast"}
    assert check_workflow("t", wf(wrong, use), lanes), "provisioning another lane passed"
    # A lane that claims nothing fetched needs no step.
    runner = {"run": "just build tier2", "env": {"NROS_GATE_SCOPE": "runner"}}
    assert check_workflow("t", wf(runner), lanes) == []
    # Job-level scope.
    assert check_workflow("t", wf({"run": "just check fast"}, job_env={"NROS_GATE_SCOPE": "fast"}),
                          lanes), "an unprovisioned job-level fast lane passed"
    assert check_workflow("t", wf({"run": "bash scripts/ci/provision-gate-lane.sh fast"},
                                  job_env={"NROS_GATE_SCOPE": "fast"}), lanes) == []
    # A lane that does not exist.
    assert check_workflow("t", wf({"run": "x", "env": {"NROS_GATE_SCOPE": "nope"}}), lanes)


def main() -> int:
    with open(REPO / ".config" / "gate-lanes.toml", "rb") as f:
        lanes = tomllib.load(f).get("lane", {})
    self_test(lanes)
    problems = []
    for p in sorted((REPO / ".github" / "workflows").glob("*.yml")):
        problems += check_workflow(f".github/workflows/{p.name}", yaml.safe_load(p.read_text()) or {},
                                   lanes)
    if problems:
        print("check-gate-lane-provisioned: a job runs a gate lane without providing its claims:",
              file=sys.stderr)
        print("\n".join(problems), file=sys.stderr)
        return 1
    print("check-gate-lane-provisioned OK -- every job running a lane with fetched claims "
          "provisions it first")
    return 0


if __name__ == "__main__":
    sys.exit(main())

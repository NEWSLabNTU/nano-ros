#!/usr/bin/env python3
"""check-release-rehearsal — a release dry run rehearses everything but the
publish (phase-567 W3).

`release-nros.yml`'s dry run and its publish must run the same jobs and the
same steps; the only differences allowed are the ones that publish:

  release-nros.yml   step `Publish the release` (the tag + the GitHub release)
                     and the `push:` input it hands `starter-image.yml`
  starter-image.yml  step `Push` (the registry push), gated on `inputs.push`

So every place either file reads `inputs.publish` / `inputs.push` must be one
of those; a job gated on them, a step gated anywhere else, or a `run:` that
branches on them is refused, naming where. Before phase-567 the whole
`starter-image` job was `if: inputs.publish`, which is how 0.6.0-nros11's
image failure surfaced only after the release had published.

And the dry run must run W4's release leg: some job of release-nros.yml runs
`just check island-shaped-workspace`, and `build` (which holds `Publish the
release`) needs that job, so a red island-shaped workspace stops the publish.
A job gated on publish is already refused above, so that job runs on a dry
run too.

And a rehearsal (the `rehearse` input, phase-567 W3 follow-up, user ruling
2026-11-17) never publishes: `rehearse` with `publish` is refused by the
lanes job's first step; only W2 and W1 carry on past a refusal, and only on
`inputs.rehearse`; the lanes `verdict` prints REFUSED and opens the run summary
with it; and `releasable` is a second lock beside `inputs.publish` on
`Publish the release` and on the image push (so the guard step is the one more
place allowed to read `inputs.publish`).

On every run: the negative controls first (a job gated on publish, a step
gated on publish, a `run:` reading it, a push-gated step that is not `Push`,
the `Push` step ungated, the island-shaped step removed, `build` no longer
needing its job, and eight for the rehearsal: the guard removed or blind to
publish, either publishing step without `releasable`, starter-image not
needing lanes, the summary's REFUSED line removed, another lanes step or a
bare W1 carrying on), then the real workflows. ~0.1 s.
"""
import copy
import sys
from pathlib import Path

import yaml

ROOT = Path(__file__).resolve().parents[2]
WF = ROOT / ".github" / "workflows"
# (file, the input that means "publish", the paths allowed to read it)
RULES = (
    ("release-nros.yml", "inputs.publish",
     {"jobs.build.steps[Publish the release].if", "jobs.starter-image.with.push",
      "jobs.lanes.steps[Refuse a rehearsal that would publish].if"}),
    ("starter-image.yml", "inputs.push",
     {"jobs.build.steps[Push].if"}),
)


def walk(node, path):
    """(path, string) for every string value, with steps named by `name:`."""
    if isinstance(node, dict):
        for k, v in node.items():
            yield from walk(v, f"{path}.{k}" if path else str(k))
    elif isinstance(node, list):
        for i, v in enumerate(node):
            label = v.get("name", i) if isinstance(v, dict) else i
            yield from walk(v, f"{path}[{label}]")
    elif isinstance(node, str):
        yield path, node


def problems(tree, word, allowed, fname):
    out = []
    for path, text in walk(tree.get("jobs", {}), "jobs"):
        if word in text and path not in allowed:
            out.append(f"{fname}: `{path}` reads {word}; a dry run and a publish must differ only at "
                       f"{', '.join(sorted(allowed))}")
    for p in sorted(allowed):
        if not any(path == p and word in text for path, text in walk(tree.get("jobs", {}), "jobs")):
            out.append(f"{fname}: `{p}` no longer reads {word}; the publish-only difference moved "
                       "(update this check with the workflow)")
    return out


ISLAND = "just check island-shaped-workspace"

# phase-567 W3 follow-up (user ruling 2026-11-17): the `rehearse` input.
GUARD = "Refuse a rehearsal that would publish"
LOCK = "needs.lanes.outputs.releasable == 'true'"
REFUSERS = ("frozen", "lanes-green")  # W2, W1: the only steps a rehearsal carries on past


def rehearse_problems(tree, fname="release-nros.yml"):
    """A rehearsal never publishes, and says so.

    1. `rehearse` with `publish` is refused by a step that runs before W2/W1.
    2. Only W2 and W1 carry on past a refusal, and only on `inputs.rehearse`.
    3. `releasable` is a second lock: `Publish the release` and the image push
       each need it beside `inputs.publish`, and the jobs that read it need `lanes`.
    4. A rehearsal's verdict prints REFUSED and opens the run summary with it.
    """
    out = []
    jobs = tree.get("jobs", {})
    inputs = (tree.get("on") or tree.get(True) or {}).get("workflow_dispatch", {}).get("inputs", {})
    if "rehearse" not in inputs:
        return [f"{fname}: no `rehearse` input"]
    lanes = jobs.get("lanes", {})
    steps = lanes.get("steps", []) or []
    names = [st.get("name") for st in steps]
    ids = [st.get("id") for st in steps]
    guard = next((i for i, st in enumerate(steps) if st.get("name") == GUARD), None)
    if guard is None:
        out.append(f"{fname}: lanes has no `{GUARD}` step, so rehearse=true with publish=true is not refused")
    else:
        st = steps[guard]
        cond = st.get("if") or ""
        if "inputs.rehearse" not in cond or "inputs.publish" not in cond or "exit 1" not in (st.get("run") or ""):
            out.append(f"{fname}: `{GUARD}` must be `if: inputs.rehearse && inputs.publish` and `exit 1`")
        later = [i for i, x in enumerate(ids) if x in REFUSERS]
        if later and guard > min(later):
            out.append(f"{fname}: `{GUARD}` runs after W2/W1; it must refuse before anything is measured")
        if st.get("continue-on-error"):
            out.append(f"{fname}: `{GUARD}` carries on past its own refusal")
    for i, st in enumerate(steps):
        coe = st.get("continue-on-error")
        if coe is None:
            continue
        if st.get("id") not in REFUSERS or str(coe).replace(" ", "") != "${{inputs.rehearse}}":
            out.append(f"{fname}: lanes step `{names[i]}` has continue-on-error `{coe}`; only W2/W1 "
                       "may, and only as `${{ inputs.rehearse }}`")
    for rid in REFUSERS:
        st = next((st for st in steps if st.get("id") == rid), None)
        if st is None or str(st.get("continue-on-error", "")).replace(" ", "") != "${{inputs.rehearse}}":
            out.append(f"{fname}: lanes step id `{rid}` missing or not `continue-on-error: ${{{{ inputs.rehearse }}}}`")
    if "releasable" not in (lanes.get("outputs") or {}):
        out.append(f"{fname}: lanes has no `releasable` output")
    verdict = next((st for st in steps if st.get("id") == "verdict"), None)
    run = (verdict or {}).get("run") or ""
    for needle, why in (("releasable=false", "a refusal sets releasable=false"),
                        ("REFUSED (rehearsal: continuing, NOT releasable)", "the rehearsal prints REFUSED"),
                        ("GITHUB_STEP_SUMMARY", "the run summary carries the refusal"),
                        ("## REHEARSAL", "the summary opens with REHEARSAL"),
                        ("not releasable", "the summary says not releasable")):
        if needle not in run:
            out.append(f"{fname}: lanes `verdict` step lacks `{needle}` ({why})")
    pub = next((st for st in jobs.get("build", {}).get("steps", []) or []
                if st.get("name") == "Publish the release"), {})
    if LOCK not in (pub.get("if") or ""):
        out.append(f"{fname}: `Publish the release` is reachable without `{LOCK}`")
    if LOCK not in str((jobs.get("starter-image", {}).get("with") or {}).get("push", "")):
        out.append(f"{fname}: the image push is reachable without `{LOCK}`")
    for job in ("build", "starter-image"):
        needs = jobs.get(job, {}).get("needs", [])
        needs = [needs] if isinstance(needs, str) else list(needs)
        if "lanes" not in needs:
            out.append(f"{fname}: `{job}` does not need `lanes`, so it cannot read `releasable`")
    return out


def island_problems(tree, fname="release-nros.yml"):
    """W4's release leg: a job runs ISLAND, and `build` needs that job."""
    jobs = tree.get("jobs", {})
    runners = [name for name, job in jobs.items()
               if any(ISLAND in (st.get("run") or "") for st in job.get("steps", []) or [])]
    if not runners:
        return [f"{fname}: no job runs `{ISLAND}`; the dry run must rehearse W4's release leg "
                "(phase-567 W3)"]
    needs = jobs.get("build", {}).get("needs", [])
    needs = [needs] if isinstance(needs, str) else list(needs)
    if not any(r in needs for r in runners):
        return [f"{fname}: `jobs.build.needs` names none of {', '.join(runners)}, so "
                f"`Publish the release` does not wait for `{ISLAND}`"]
    return []


def self_test(trees):
    """Each mutation must be refused; the real trees are the baseline."""
    fails = []
    rel, star = trees["release-nros.yml"], trees["starter-image.yml"]
    w_rel, a_rel = RULES[0][1], RULES[0][2]
    w_star, a_star = RULES[1][1], RULES[1][2]

    def refused(tree, word, allowed, name):
        if not problems(tree, word, allowed, "mutant"):
            fails.append(f"negative control not refused: {name}")

    m = copy.deepcopy(rel)
    m["jobs"]["starter-image"]["if"] = "${{ inputs.publish }}"
    refused(m, w_rel, a_rel, "the starter-image job gated on publish (the pre-W3 shape)")
    m = copy.deepcopy(rel)
    m["jobs"]["build"]["steps"][0]["if"] = "${{ inputs.publish }}"
    refused(m, w_rel, a_rel, "a build step other than `Publish the release` gated on publish")
    m = copy.deepcopy(rel)
    m["jobs"]["build"]["steps"].append({"name": "x", "run": 'if [ "${{ inputs.publish }}" = true ]; then :; fi'})
    refused(m, w_rel, a_rel, "a `run:` that branches on publish")
    m = copy.deepcopy(star)
    m["jobs"]["build"]["steps"][0]["if"] = "${{ inputs.push != false }}"
    refused(m, w_star, a_star, "a starter-image step other than `Push` gated on push")
    m = copy.deepcopy(star)
    for st in m["jobs"]["build"]["steps"]:
        if st.get("name") == "Push":
            st.pop("if")
    refused(m, w_star, a_star, "the `Push` step no longer gated (a dry run would push)")

    if island_problems(rel):
        fails.append("baseline: the real release-nros.yml fails the island-shaped rule, so its "
                     "negative controls prove nothing")
    m = copy.deepcopy(rel)
    for job in m["jobs"].values():
        job["steps"] = [st for st in job.get("steps", []) if ISLAND not in (st.get("run") or "")]
    if not island_problems(m):
        fails.append("negative control not refused: the `just check island-shaped-workspace` step removed")
    m = copy.deepcopy(rel)
    needs = m["jobs"]["build"].get("needs", [])
    needs = [needs] if isinstance(needs, str) else list(needs)
    m["jobs"]["build"]["needs"] = [n for n in needs if n != "island-shaped-workspace"]
    if not island_problems(m):
        fails.append("negative control not refused: `build` no longer needs the island-shaped job")

    if rehearse_problems(rel):
        fails.append("baseline: the real release-nros.yml fails the rehearse rule, so its "
                     "negative controls prove nothing")

    def r_refused(m, name):
        if not rehearse_problems(m):
            fails.append(f"negative control not refused: {name}")

    m = copy.deepcopy(rel)
    m["jobs"]["lanes"]["steps"] = [st for st in m["jobs"]["lanes"]["steps"] if st.get("name") != GUARD]
    r_refused(m, "rehearse=true with publish=true not refused (the guard step removed)")
    m = copy.deepcopy(rel)
    for st in m["jobs"]["lanes"]["steps"]:
        if st.get("name") == GUARD:
            st["if"] = "${{ inputs.rehearse }}"
    r_refused(m, "the guard no longer reads publish")
    m = copy.deepcopy(rel)
    st = next(st for st in m["jobs"]["build"]["steps"] if st.get("name") == "Publish the release")
    st["if"] = "${{ inputs.publish }}"
    r_refused(m, "`Publish the release` reachable without releasable")
    m = copy.deepcopy(rel)
    m["jobs"]["starter-image"]["with"]["push"] = "${{ inputs.publish }}"
    r_refused(m, "the image push reachable without releasable")
    m = copy.deepcopy(rel)
    m["jobs"]["starter-image"]["needs"] = "build"
    r_refused(m, "starter-image no longer needs lanes")
    m = copy.deepcopy(rel)
    v = next(st for st in m["jobs"]["lanes"]["steps"] if st.get("id") == "verdict")
    v["run"] = "\n".join(l for l in v["run"].splitlines() if "## REHEARSAL" not in l)
    r_refused(m, "a rehearsal summary missing the REFUSED line")
    m = copy.deepcopy(rel)
    m["jobs"]["lanes"]["steps"][-1]["continue-on-error"] = True
    r_refused(m, "a lanes step other than W2/W1 carries on past a refusal")
    m = copy.deepcopy(rel)
    st = next(st for st in m["jobs"]["lanes"]["steps"] if st.get("id") == "lanes-green")
    st["continue-on-error"] = True
    r_refused(m, "W1 carries on past a refusal without a rehearsal")
    return fails


def main():
    trees = {f: yaml.safe_load((WF / f).read_text()) for f, _, _ in RULES}
    fails = self_test(trees)
    for f in fails:
        print(f"check-release-rehearsal: self-test FAIL, {f}", file=sys.stderr)
    if fails:
        return 1
    probs = []
    for f, word, allowed in RULES:
        probs += problems(trees[f], word, allowed, f)
    probs += island_problems(trees["release-nros.yml"])
    probs += rehearse_problems(trees["release-nros.yml"])
    for p in probs:
        print(f"check-release-rehearsal: {p}", file=sys.stderr)
    if probs:
        return 1
    print("check-release-rehearsal: OK (a dry run differs from a publish only by the tag + release "
          "step and the image push, runs the island-shaped workspace before publishing, and a "
          "rehearsal is refused with publish and locked out of both by `releasable`; "
          "15 negative controls refused)")
    return 0


if __name__ == "__main__":
    sys.exit(main())

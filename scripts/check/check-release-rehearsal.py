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

On every run: the negative controls first (a job gated on publish, a step
gated on publish, a `run:` reading it, a push-gated step that is not `Push`),
then the real workflows. ~0.1 s.
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
     {"jobs.build.steps[Publish the release].if", "jobs.starter-image.with.push"}),
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
    for p in probs:
        print(f"check-release-rehearsal: {p}", file=sys.stderr)
    if probs:
        return 1
    print("check-release-rehearsal: OK (a dry run differs from a publish only by the tag + release "
          "step and the image push; 5 negative controls refused)")
    return 0


if __name__ == "__main__":
    sys.exit(main())

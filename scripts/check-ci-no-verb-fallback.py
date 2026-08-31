#!/usr/bin/env python3
"""A CI step must not fall back from one `just` verb to another.

phase-411 — "named must work; unnamed may skip, and is reported."

## The shape

`nightly.yml` ran, for every platform cell:

    just <plat> build-all || just <plat> build-examples || just <plat> build

Every platform module defines all four verbs, so the fallbacks could never fire
for a MISSING recipe. They fired only when the previous verb FAILED — and the
step then built something smaller and reported success.

That is the "skip and report success" defect phase-411 exists to remove,
expressed in shell instead of in a skip helper. It defeated the mechanism that
was already working: nightly sets no `NROS_LANE_INCLUDED`, so its platforms are
NAMED, and a named platform whose prerequisite is missing fails loudly with
`NROS_LANE_NAMED_FAIL`. The `||` chain caught that failure and downgraded it.
The missing prerequisite surfaced two stages later as nine skipped tests and a
`_check-skip-budget` red — twenty minutes after the point of decision, naming
the gate rather than the missing tool.

## What is checked

Inside a workflow `run:` block, a command whose head is `just` must not be the
left operand of `||` where the right operand is also a `just` command.

## What is deliberately allowed

* `just … || echo "…"` and `just … || true` — a step choosing to CONTINUE past a
  known-optional command, which is a different decision and is visible in the
  log. `report-interlock-coverage.sh` exists for the cases where that needs
  saying out loud.
* `cmd || just …` where the left side is not `just` — a fallback INTO the
  toolchain, not between two of its verbs.
* `&&` chains — sequencing, not fallback.
* Comment lines and heredoc bodies, for the reason `check-workflow-repo-env`
  documents: a gate that cannot tell a command from a sentence about a command
  is worse than no gate.

Run: python3 scripts/check-ci-no-verb-fallback.py [--self-test]
"""

import argparse
import re
import sys
from pathlib import Path

REPO = Path(__file__).resolve().parent.parent
WORKFLOWS = REPO / ".github" / "workflows"

# `just …` on the left of `||`, with another `just` as the right operand.
# `[^|]*` keeps the match inside one command rather than spanning a pipeline.
FALLBACK = re.compile(r"\bjust\s[^|]*\|\|\s*just\s")

HEREDOC = re.compile(r"<<-?\s*(['\"]?)([A-Za-z_][A-Za-z0-9_]*)\1")


def command_lines(run: str):
    """Lines of a `run:` body that are commands — not comments, not heredocs."""
    out, terminator = [], None
    for line in (run or "").split("\n"):
        if terminator is not None:
            if line.strip() == terminator:
                terminator = None
            continue
        if not line.strip() or line.strip().startswith("#"):
            continue
        out.append(line)
        m = HEREDOC.search(line)
        if m:
            terminator = m.group(2)
    return out


def offenders(docs):
    bad = []
    for path, doc in docs:
        for job_name, job in (doc.get("jobs") or {}).items():
            for step in job.get("steps", []) or []:
                for line in command_lines(step.get("run") or ""):
                    if FALLBACK.search(line):
                        bad.append(
                            (path, job_name, step.get("name") or "(unnamed)", line.strip())
                        )
    return bad


def load():
    import yaml

    return [
        (p.relative_to(REPO), yaml.safe_load(p.read_text()))
        for p in sorted(WORKFLOWS.glob("*.yml"))
    ]


def self_test() -> int:
    cases = [
        ("just a build-all || just a build-examples", True),
        ("just a build-all || just a build-examples || just a build", True),
        # continuing past a known-optional command is a different decision
        ('just a setup || echo "optional"', False),
        ("just a setup || true", False),
        # a fallback INTO just, not between its verbs
        ("cmake --build . || just a build", False),
        # sequencing
        ("just a setup && just a build", False),
        ("just a build", False),
    ]
    failures = 0
    for text, want in cases:
        got = bool(FALLBACK.search(text))
        if got != want:
            print(f"  self-test FAIL: {text!r} -> {got}, want {want}")
            failures += 1

    # comments and heredocs are not commands
    if command_lines("# just a || just b\njust c\n") != ["just c"]:
        print("  self-test FAIL: comment line was read as a command")
        failures += 1
    if command_lines("cat <<EOF\njust a || just b\nEOF\njust c\n") != ["cat <<EOF", "just c"]:
        print("  self-test FAIL: heredoc body was read as commands")
        failures += 1

    if failures:
        print(f"check-ci-no-verb-fallback self-test: {failures} case(s) FAILED")
        return 1
    print(f"check-ci-no-verb-fallback self-test: OK ({len(cases)} cases + extraction)")
    return 0


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--self-test", action="store_true")
    args = ap.parse_args()
    if args.self_test:
        return self_test()
    if self_test() != 0:
        return 1

    docs = load()
    bad = offenders(docs)
    if bad:
        print("check-ci-no-verb-fallback: CI step(s) fall back from one `just` verb to another:")
        for path, job, step, line in bad:
            print(f"  {path}  [{job}] {step}")
            print(f"      {line[:100]}")
        print()
        print("  A fallback makes a FAILED verb build something smaller and report")
        print("  success — the shape phase-411 removes. Name one verb per path.")
        print("  To continue past a known-optional command, say so: `|| echo …`.")
        return 1

    steps = sum(len(j.get("steps", []) or []) for _, d in docs for j in (d.get("jobs") or {}).values())
    print(
        f"check-ci-no-verb-fallback: OK — {len(docs)} workflow(s), {steps} step(s); "
        "no `just` verb falls back to another."
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())

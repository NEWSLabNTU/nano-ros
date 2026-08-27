#!/usr/bin/env python3
"""A CI doc must not cite a workflow file that does not exist — phase-395.

`docs/development/ci-conventions.md` named six workflow files, and all six had
been consolidated away by the phase-253 reorg. Its GUIDANCE was still correct —
that is what made it worth keeping and also what made it misleading: a reader
who cannot open `zephyr-dual-line.yml` goes looking for the file instead of for
the rule it illustrated, and concludes the page is abandoned.

This is the same class as `check-just-recipe-refs` (a doc naming a recipe that
does not resolve) and as issue 0743 (a nextest override naming a deleted
binary): a reference that silently stops resolving, in a place that still reads
as current.

SCOPE, AND WHY IT IS NARROW

Only the docs that are ABOUT this repo's CI are scanned. A `.yml` elsewhere in
the tree is usually somebody else's file format — `west.yml`, `module.yml`,
`idf_component.yml`, `sample.yaml` — and a blanket rule over all of `docs/`
would be mostly false positives, which is how a gate gets disabled.

A cited name passes if it is a real file under `.github/workflows/`, or if the
doc declares it under a "Historical workflow names" heading, which is the
honest way to keep a lesson whose file is gone.

Usage::

    check-ci-doc-workflow-refs.py [--selftest]
"""

import os
import re
import subprocess
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
WORKFLOWS = os.path.join(ROOT, ".github", "workflows")

DOCS = (
    "docs/development/ci-conventions.md",
    "docs/development/ci-workflow-reorg.md",
    "docs/development/multi-agent-ci-workflow.md",
)

REF = re.compile(r"`([a-z0-9][a-z0-9._-]*\.ya?ml)`")
HISTORICAL_HEAD = re.compile(r"^#+\s*Historical workflow names\s*$", re.M | re.I)

# Names that are a FORMAT, not one of our workflows. Kept explicit rather than
# inferred: an inferred exemption is one nobody can audit.
NOT_OURS = {
    "west.yml", "module.yml", "idf_component.yml", "sample.yaml",
    "system_model.yaml", "patches.yml", "action.yml", "docker-compose.yml",
}


def historical_names(text):
    """Names declared under a `Historical workflow names` heading, to the next heading."""
    m = HISTORICAL_HEAD.search(text)
    if not m:
        return set()
    rest = text[m.end():]
    nxt = re.search(r"^#+\s", rest, re.M)
    block = rest[: nxt.start()] if nxt else rest
    return set(REF.findall(block))


def scan():
    existing = set(os.listdir(WORKFLOWS)) if os.path.isdir(WORKFLOWS) else set()
    problems = []
    checked = 0
    for rel in DOCS:
        path = os.path.join(ROOT, rel)
        if not os.path.exists(path):
            continue
        with open(path, encoding="utf8") as fh:
            text = fh.read()
        allowed = historical_names(text) | NOT_OURS
        for name in sorted(set(REF.findall(text))):
            if name in NOT_OURS:
                continue
            checked += 1
            if name in existing or name in allowed:
                continue
            problems.append((rel, name))
    return problems, checked, existing


def main():
    if "--selftest" in sys.argv:
        return selftest()
    problems, checked, existing = scan()
    if problems:
        print("check-ci-doc-workflow-refs: doc(s) cite a workflow that does not exist:\n",
              file=sys.stderr)
        for rel, name in problems:
            print(f"  {rel}: `{name}`", file=sys.stderr)
        print(
            "\n  Either the file was renamed — update the citation — or it was\n"
            "  consolidated away, in which case list it under a\n"
            "  `### Historical workflow names` heading with what replaced it.\n"
            "  A doc naming a workflow nobody can open sends the reader looking\n"
            "  for a file instead of for the rule it was illustrating.",
            file=sys.stderr,
        )
        return 1
    print(
        f"check-ci-doc-workflow-refs OK — {checked} citation(s) across "
        f"{len(DOCS)} CI doc(s), against {len(existing)} workflow file(s)."
    )
    return 0


def selftest():
    """Prove it can fail. A gate that cannot fail reads as coverage."""
    import tempfile
    global DOCS, ROOT
    ok = fail = 0

    def check(desc, cond):
        nonlocal ok, fail
        print(f"  {'ok   ' if cond else 'FAIL '} {desc}")
        if cond:
            ok += 1
        else:
            fail += 1

    real_root, real_docs = ROOT, DOCS
    with tempfile.TemporaryDirectory() as d:
        os.makedirs(os.path.join(d, ".github", "workflows"))
        open(os.path.join(d, ".github", "workflows", "real.yml"), "w").close()
        os.makedirs(os.path.join(d, "docs", "development"))
        doc = os.path.join(d, "docs", "development", "t.md")
        globals()["ROOT"] = d
        globals()["WORKFLOWS"] = os.path.join(d, ".github", "workflows")
        globals()["DOCS"] = ("docs/development/t.md",)

        with open(doc, "w") as fh:
            fh.write("see `real.yml`\n")
        check("a citation of an EXISTING workflow passes", not scan()[0])

        with open(doc, "w") as fh:
            fh.write("see `gone.yml`\n")
        check("a citation of a MISSING workflow FAILS", bool(scan()[0]))

        with open(doc, "w") as fh:
            fh.write("see `gone.yml`\n\n### Historical workflow names\n\n"
                     "| `gone.yml` | replaced by `real.yml` |\n")
        check("...unless declared under `Historical workflow names`", not scan()[0])

        with open(doc, "w") as fh:
            fh.write("### Historical workflow names\n\n| `a.yml` |\n\n"
                     "## Later section\n\nsee `b.yml`\n")
        probs = [n for _, n in scan()[0]]
        check("the historical block ENDS at the next heading (`b.yml` still fails)",
              probs == ["b.yml"])

        with open(doc, "w") as fh:
            fh.write("a Zephyr manifest is `west.yml`\n")
        check("a foreign format (`west.yml`) is not treated as our workflow",
              not scan()[0])

    globals()["ROOT"], globals()["DOCS"] = real_root, real_docs
    globals()["WORKFLOWS"] = os.path.join(real_root, ".github", "workflows")
    print(f"\n{ok} passed, {fail} failed")
    return 1 if fail else 0


if __name__ == "__main__":
    sys.exit(main())

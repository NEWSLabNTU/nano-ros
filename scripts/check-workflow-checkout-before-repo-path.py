#!/usr/bin/env python3
"""A workflow step that reaches into the repository must come after a checkout.

Issue 1843.

## Why

A GitHub Actions job starts in an EMPTY workspace. Nothing from the repository
exists until a step checks it out, so a step that runs `scripts/…`, `just`,
`./activate.sh` or a local action `./.github/actions/<x>` before that point is
running a file that is not there.

That is a loud failure in most shells, and it was SILENT in the one that
mattered: `queue-notify.yml` had no checkout at all and ran

    if triage="$(bash scripts/ci/queue-triage.sh 2>/dev/null)"; then

so the missing file was an `if` that took the else branch, the verdict was
`UNKNOWN` on every run, and the INFRA notice phase-396 W4 wrote ("this is not
your pull request") was unreachable from the day it landed. Every surrounding
gate was green, because each asks what a step's commands SAY, never whether
the files they name exist when the step runs.

## What counts

A step needs the checkout when it

* `uses: ./…` — a local action is a file in the repository;
* runs `just` in command position (its justfile is the repository's);
* names a repo-relative path in a command — a token starting with `./`, or
  with one of the repository's TRACKED top-level directories followed by `/`
  (`scripts/ci/x.sh`, `tools/…`). The directory list is read from git, not
  written here, so a new top-level directory is covered without an edit.

Comments and heredoc bodies are not commands — ONE spelling of that,
`scripts/lib/workflow_commands.py` (the same reason `check-workflow-repo-env`
gives: `queue-notify.yml`'s comment TEXT tells an author to run `just …`).

A checkout is `uses: actions/checkout@…`. A job with no steps (a reusable
workflow call) is skipped. Composite actions under `.github/actions/` are NOT
read: one is reached only through `uses: ./.github/actions/<x>`, which is
itself a local path this gate requires a checkout before.

Run: python3 scripts/check-workflow-checkout-before-repo-path.py [--self-test]
"""

import argparse
import re
import subprocess
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent / "lib"))

from workflow_commands import REPO, command_lines, invoke_re, load_workflows  # noqa: E402

JUST = invoke_re(("just",))
CHECKOUT = re.compile(r"^actions/checkout@")


def top_level_dirs(repo=REPO):
    """The repository's tracked top-level directories (`.github`, `scripts`, …)."""
    out = subprocess.run(
        ["git", "-C", str(repo), "ls-tree", "--name-only", "-d", "HEAD"],
        capture_output=True,
        text=True,
        check=True,
    ).stdout
    dirs = [d for d in out.split() if d]
    if not dirs:
        raise SystemExit("check-workflow-checkout-before-repo-path: git listed no directories")
    return dirs


def path_re(dirs):
    """A token that is a repo-relative path: `./x` or `<tracked top dir>/…`.

    The token must START at a word boundary a shell would split on (start,
    whitespace, quote, `(`, `=`, `;`, `&`, `|`), so `$HOME/scripts/x` and
    `https://…/scripts/` are not read as the repository's `scripts/`.
    """
    alt = "|".join(re.escape(d) for d in sorted(dirs, key=len, reverse=True))
    return re.compile(rf"""(?:^|[\s"'(=;&|])(?:\./[A-Za-z._]|(?:\./)?(?:{alt})/)""")


def needs_checkout(step, paths):
    """The reason this step needs the repository, or None."""
    uses = step.get("uses") or ""
    if uses.startswith("./"):
        return f"uses: {uses}"
    for line in command_lines(step.get("run") or ""):
        if JUST.search(line) or paths.search(line):
            return line.strip()
    return None


def offenders(docs, dirs):
    paths = path_re(dirs)
    bad = []
    for path, doc in docs:
        for job_name, job in ((doc or {}).get("jobs") or {}).items():
            checked_out = False
            for step in (job or {}).get("steps") or []:
                if CHECKOUT.match(step.get("uses") or ""):
                    checked_out = True
                    continue
                if checked_out:
                    continue
                why = needs_checkout(step, paths)
                if why:
                    bad.append((path, job_name, step.get("name") or step.get("uses") or "(unnamed)", why))
    return bad


def self_test():
    dirs = ["scripts", "tools", ".github", "just"]
    co = {"uses": "actions/checkout@v4"}
    cases = [
        # (name, steps, expected offender?)
        # The measured defect: queue-notify.yml, no checkout, runs a repo script.
        (
            "repo script, no checkout (queue-notify)",
            [{"name": "notify", "run": 'if t="$(bash scripts/ci/queue-triage.sh 2>/dev/null)"; then :; fi\n'}],
            True,
        ),
        ("repo script after checkout", [co, {"run": "bash scripts/ci/x.sh\n"}], False),
        ("repo script BEFORE checkout", [{"run": "bash scripts/ci/x.sh\n"}, co], True),
        ("just, no checkout", [{"run": "just check fast\n"}], True),
        ("source ./activate.sh, no checkout", [{"run": "source ./activate.sh\n"}], True),
        ("dot-slash script, no checkout", [{"run": "./tools/x\n"}], True),
        ("local action, no checkout", [{"uses": "./.github/actions/setup"}], True),
        ("local action after checkout", [co, {"uses": "./.github/actions/setup"}], False),
        ("marketplace action, no checkout", [{"uses": "actions/setup-python@v5"}], False),
        ("gh api only, no checkout", [{"run": "gh api repos/x/y/rules/branches/main\n"}], False),
        # Prose: comment text and heredoc bodies name repo paths without running them.
        ("comment names a script", [{"run": "# see scripts/ci/x.sh\ngh pr comment 1\n"}], False),
        (
            "heredoc text names just + a script",
            [{"run": 'b="$(cat <<MSG\njust queue-triage 1\nscripts/ci/x.sh\nMSG\n)"\ngh pr comment\n'}],
            False,
        ),
        # A path that is NOT the repository's.
        ("a path under $HOME", [{"run": "ls $HOME/scripts/x\n"}], False),
        ("a URL", [{"run": "curl https://example.com/scripts/x\n"}], False),
        ("an untracked top dir", [{"run": "ls build/x\n"}], False),
    ]
    failures = 0
    for name, steps, expect in cases:
        doc = {"jobs": {"j": {"steps": steps}}}
        got = bool(offenders([(Path("selftest.yml"), doc)], dirs))
        if got != expect:
            print(f"  self-test FAIL: {name}: expected offender={expect}, got {got}")
            failures += 1
    if failures:
        print(f"check-workflow-checkout-before-repo-path self-test: {failures} case(s) FAILED")
        return 1
    print(f"check-workflow-checkout-before-repo-path self-test: OK ({len(cases)} cases)")
    return 0


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--self-test", action="store_true")
    args = ap.parse_args()
    if args.self_test:
        return self_test()
    if self_test() != 0:
        return 1

    docs = load_workflows(include_actions=False)
    bad = offenders(docs, top_level_dirs())
    if bad:
        print("check-workflow-checkout-before-repo-path: step(s) reach the repository before any checkout:")
        for path, job, step, why in bad:
            print(f"  {path}  [{job}] {step}")
            print(f"      {why[:96]}")
        print()
        print("  A job starts in an EMPTY workspace. Add, before the step:")
        print("      - uses: actions/checkout@v4")
        print("        with:")
        print("          persist-credentials: false")
        print("  A missing script inside `if`/`$(…)`/`|| true` fails SILENTLY (issue 1843).")
        return 1

    jobs = sum(len((d or {}).get("jobs") or {}) for _, d in docs)
    print(
        f"check-workflow-checkout-before-repo-path: OK — {len(docs)} workflow(s), {jobs} job(s); "
        "every step that reaches the repository runs after a checkout."
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())

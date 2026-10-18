#!/usr/bin/env python3
"""A workflow job that reaches `check-submodule-pins` provides its baseline first.

Issue 1959. `check-submodule-pins` is network-free by the check-fast contract,
so it cannot fetch the commit it compares against. In CI it FAILS when that
commit does not resolve (a silent skip there reads exactly like a green check),
and the workflow provides it with one step:

    - name: Fetch the pin baseline for check-submodule-pins
      run: bash scripts/ci/pin-baseline.sh provide

Issue 1912 moved the baseline from `origin/main` to the tested commit's own
(the first parent, on schedule and dispatch), and added that step to every job
in `gate.yml` and `host-tests.yml`. The self-hosted fixture lanes reach the
same gate through `just build <lane>` (`build-test-fixtures` depends on
`check::fast`) and got no step. `origin/main` had resolved in their persistent
fixture tree; a depth-1 tree's first parent does not. The next scheduled run
went red in tier 1, tier 2 and the tier-2 nightly, each before any cell ran.

So the rule is about REACH, not about which file a job lives in: every job
with a `just` invocation whose recipe closure contains a pin-gate entry point
must run `pin-baseline.sh provide` in an EARLIER step.

The closure comes from just's own dump (`scripts/lib/just_closure.py`) and is
an upper bound: `just build linux` and `just build tier1` are one recipe. A job
flagged here that never reaches the gate at run time pays one depth-1 fetch,
which is cheap; a job missed here is a red lane with no verdict.

`check::fast` runs its gates through `run-gates-parallel.sh`, which no just edge
records, so the entry points are named rather than derived: the fast line in
both spellings and the gate itself.
"""

from __future__ import annotations

import re
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent / "lib"))
import just_closure  # noqa: E402
import workflow_commands  # noqa: E402

ENTRY_POINTS = ("check::fast", "check::fast-serial", "check::submodule-pins")
PROVIDE = re.compile(r"(?:^|[;&|\s])(?:bash\s+)?(?:\./)?scripts/ci/pin-baseline\.sh\s+provide\b")
JUST_INVOKE = workflow_commands.invoke_re(("just",))
JUST_ARGS = re.compile(r"\bjust\s+((?:--?[A-Za-z][\w-]*(?:[ =]\S+)?\s+)*)(\S+)(?:\s+(\S+))?")


def reaching(dump) -> set[str]:
    """Every recipe whose closure contains an entry point."""
    table = just_closure.recipe_table(dump)
    out = set()
    for name in table:
        if any(e in just_closure.closure(dump, name) for e in ENTRY_POINTS):
            out.add(name)
    return out


def invoked_recipes(line: str, dump) -> list[str]:
    table = just_closure.recipe_table(dump)
    modules = set(dump.get("modules", {}))
    names = []
    for m in JUST_INVOKE.finditer(line):
        a = JUST_ARGS.match(line, m.start() + m.group(0).rfind("just"))
        if not a:
            continue
        names.extend(just_closure.resolve_call(a.group(2), a.group(3), "", table, modules))
    return names


def judge(workflows, dump) -> list[str]:
    hits = reaching(dump)
    bad = []
    for path, doc in workflows:
        for job_id, job in ((doc or {}).get("jobs") or {}).items():
            provided = False
            for i, step in enumerate((job or {}).get("steps") or []):
                run = step.get("run") or ""
                lines = workflow_commands.logical_lines(run)
                for line in lines:
                    if not provided:
                        for r in invoked_recipes(line, dump):
                            if r in hits:
                                bad.append(
                                    f"{path}: job `{job_id}`, step "
                                    f"`{step.get('name') or f'#{i}'}` runs `just {r.replace('::', ' ')}`, "
                                    f"which reaches check-submodule-pins, with no earlier "
                                    f"`bash scripts/ci/pin-baseline.sh provide` step"
                                )
                                break
                if any(PROVIDE.search(line) for line in lines):
                    provided = True
    return bad


def selftest():
    dump = {
        "recipes": {
            "build": {"dependencies": [{"recipe": "build-test-fixtures"}], "body": []},
            "build-test-fixtures": {"dependencies": [{"recipe": "check::fast"}], "body": []},
            "lint": {"dependencies": [], "body": [["echo hi"]]},
            "wrap": {"dependencies": [], "body": [["just check fast"]]},
            # Usage text that NAMES a reaching recipe and runs nothing.
            "help": {"dependencies": [], "body": [["printf '%s\\n' \\"], ['    "  just build tier1   build it" \\']]},
        },
        "modules": {"check": {"recipes": {"fast": {"dependencies": [], "body": []}}, "modules": {}}},
    }
    provide = {"name": "p", "run": "bash scripts/ci/pin-baseline.sh provide"}

    def wf(*steps):
        return [(Path("w.yml"), {"jobs": {"j": {"steps": list(steps)}}})]

    cases = [
        ("a lane build with no provide step", wf({"run": "just build tier1"}), 1),
        ("a body call `just check fast`", wf({"run": "just wrap"}), 1),
        ("the module form", wf({"run": "source ./activate.sh\njust check fast"}), 1),
        ("provide BEFORE the build", wf(provide, {"run": "just build tier1"}), 0),
        ("provide AFTER the build is too late", wf({"run": "just build tier1"}, provide), 1),
        ("a recipe that reaches nothing", wf({"run": "just lint"}), 0),
        ("help text naming a lane build", wf({"run": "just help"}), 0),
        ("prose is not an invocation", wf({"run": "# just build tier1\necho"}), 0),
    ]
    fails = 0
    for label, w, want in cases:
        got = len(judge(w, dump))
        if got != want:
            print(f"check-workflow-pin-baseline SELFTEST FAILED: {label}: {got} finding(s), want {want}")
            fails += 1
    if fails:
        sys.exit(1)


def main():
    selftest()
    dump = just_closure.dump()
    hits = reaching(dump)
    missing = [e for e in ENTRY_POINTS if e not in just_closure.recipe_table(dump)]
    if missing:
        sys.exit(f"check-workflow-pin-baseline: entry point(s) {missing} are not recipes any more; update ENTRY_POINTS")
    bad = judge(workflow_commands.load_workflows(include_actions=False), dump)
    if bad:
        print("check-workflow-pin-baseline: FAILED (issue 1959)\n")
        for b in bad:
            print(f"  {b}")
        print(
            "\n  check-submodule-pins cannot fetch its baseline (check-fast is network-free) and"
            "\n  fails in CI when it does not resolve. Add, before that step:"
            "\n\n    - name: Fetch the pin baseline for check-submodule-pins"
            "\n      run: bash scripts/ci/pin-baseline.sh provide"
        )
        sys.exit(1)
    print(f"check-workflow-pin-baseline: OK — every job reaching the pin gate ({len(hits)} recipe(s)) provides its baseline first")


if __name__ == "__main__":
    main()

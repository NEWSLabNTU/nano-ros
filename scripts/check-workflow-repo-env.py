#!/usr/bin/env python3
"""Every CI step that invokes `just`/`nros`/`west` must source the repo environment.

Issue 0933.

## Why

`activate.sh` is the activation SSoT. It exports `NROS_REPO_DIR` and
`nano_ros_ROOT`, puts the in-tree CLI and the toolchains on PATH, and sources
ROS itself (picking the file the current shell can read, nounset-guarded —
issue 0639). A step that skips it gets a shell that looks fine and is missing
the one variable the build needs.

`nano_rosConfig.cmake` lives at the CHECKOUT ROOT and is located via
`nano_ros_ROOT`. A generated workspace root sets no CMake prefix on purpose —
its paths stay relative so they are byte-identical across machines — so the
environment is the only channel that can carry it. Without it:

    CMake Error at CMakeLists.txt:23 (find_package):
      ... asked CMake to find a package configuration file provided by
      "nano_ros", but CMake did not find one.

That failure took down the host-tests workspace fixture build (#92) and, in a
different workflow, four Zephyr 3.7 cells (#105). Both were invisible locally in
BOTH directions: a developer shell has always sourced `activate.sh`, and a warm
build directory carries `nano_ros_DIR:PATH=<checkout>` in its `CMakeCache.txt`,
so even an unsourced re-configure succeeds on a tree that once worked.

`just doctor` enforces this for a developer's shell. Nothing enforced it for a
CI step, and a CI step is the one place the shell is fresh every time.

## What counts as an invocation

Two kinds of false positive would make this gate worse than nothing, and both
were found in the tree while writing it:

* **Prose.** Comment lines are skipped. `nightly.yml`'s CLI-build step has a
  comment about `nros sync` and does not run it. Counting words in comments is
  the mistake that makes a grep-based gate useless.

* **Command text inside a heredoc.** `queue-notify.yml` builds a pull-request
  comment with `body="$(cat <<MSG ... MSG)"` whose text tells the author to run
  `just queue-triage` and `just ci l1`. Those are STRINGS being posted to
  GitHub, not commands, and "fixing" that step would source an environment
  nothing in it uses. Heredoc bodies are skipped.

A line counts when `just`, `nros` or `west` stands in command position: at the
start, after `&&`, `||`, `;` or `then`, optionally behind `KEY=value` prefixes.

Run: python3 scripts/check-workflow-repo-env.py [--self-test]
"""

import argparse
import re
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent / "lib"))

# ONE spelling of "is this line a command, or a sentence about one". Phase-466
# W2 moved it into `scripts/lib/` when `check-workflow-just-provisioning` needed
# the same answer: two copies of a detector drift in the direction that reads
# green, because a detector that misses an invocation reports OK.
from workflow_commands import (  # noqa: E402
    INVOKE,
    REPO,
    TOOLS,
    WORKFLOWS,
    command_lines,
    load_workflows,
)

# A real ACTIVATION COMMAND: `source` or `.` in command position, naming the
# repo's `activate.sh` (any path spelling) or the legacy `./setup.bash` shim.
#
# Issue 1548: this used to be `"activate.sh" in run` — a substring test over
# the whole body, comments included, so a step whose COMMENT said "unlike the
# others this does not source activate.sh" was exempt. The same prose-vs-command
# distinction this file's header insists on for `just`, applied to the
# exemption as well as the detection. A ROS `…/setup.bash` does not count: it
# exports no `nano_ros_ROOT`, which is the whole point (issue 0933).
ACTIVATION = re.compile(
    r"(?:^|&&\s*|\|\|\s*|;\s*|\bthen\s+)\s*"
    r"(?:source|\.)\s+"
    r"""(?:["']?\S*/)?(?:activate\.sh|(?<![\w/])\./setup\.bash)["']?(?:\s|;|&|$)"""
)


def offenders(docs):
    """A step whose first just/nros/west invocation precedes any activation.

    ORDER matters, not just presence: `just x; source ./activate.sh` ran `just`
    in the bare shell.
    """
    bad = []
    for path, doc in docs:
        for job_name, job in (doc.get("jobs") or {}).items():
            for step in job.get("steps", []) or []:
                run = step.get("run") or ""
                if not run:
                    continue
                for line in command_lines(run):
                    act, inv = ACTIVATION.search(line), INVOKE.search(line)
                    if act and (not inv or act.start() <= inv.start()):
                        break
                    if inv:
                        bad.append(
                            (path, job_name, step.get("name") or "(unnamed)", line.strip())
                        )
                        break
    return bad


def self_test():
    """The two false positives that were live in the tree, plus a true positive.

    A gate for this class is only worth having if it can tell a command from a
    sentence about a command.
    """
    cases = [
        # (name, run body, expected offender?)
        ("bare invocation", "just check fast\n", True),
        ("env-prefixed", "NROS_ZEPHYR_VERSION=3.7 just zephyr setup\n", True),
        ("chained", "cd x && nros sync .\n", True),
        ("already sourced", "source ./activate.sh\njust check fast\n", False),
        ("legacy shim sourced", "source ./setup.bash\njust test-unit\n", False),
        ("comment prose only", "# `nros sync` refuses an unresolved model\ncargo build\n", False),
        (
            "heredoc text",
            'body="$(cat <<MSG\nRun: just queue-triage 12\nMSG\n)"\ngh pr comment\n',
            False,
        ),
        ("quoted heredoc text", "cat <<'EOF'\njust ci l1\nEOF\n", False),
        ("no tool at all", "cargo build --release\n", False),
        # A heredoc must not swallow the rest of the file: a real invocation
        # after the terminator still counts.
        ("invocation after heredoc", "cat <<EOF\njust ci l1\nEOF\njust check fast\n", True),
        # Issue 1548 — the exemption must be a COMMAND, not a word.
        ("activate.sh only in a comment", "# unlike the rest, no activate.sh here\njust check fast\n", True),
        ("activate.sh only in an echo", "echo 'run source ./activate.sh'\njust check fast\n", True),
        ("sourced AFTER the invocation", "just check fast\nsource ./activate.sh\n", True),
        ("ROS setup is not the repo env", 'source "$ROS_PREFIX/setup.bash"\njust check fast\n', True),
        ("dot-sourced", ". ./activate.sh\njust check fast\n", False),
        ("sourced by absolute path", 'source "$GITHUB_WORKSPACE/activate.sh"\njust check fast\n', False),
        ("sourced in a chain", "cd x && source ../activate.sh && just check fast\n", False),
        ("strict-mode prologue", "set -euo pipefail\nsource ./activate.sh\njust qemu setup-qemu\n", False),
    ]
    failures = 0
    for name, run, expect in cases:
        doc = {"jobs": {"j": {"steps": [{"name": name, "run": run}]}}}
        got = bool(offenders([(Path("selftest.yml"), doc)]))
        if got != expect:
            print(f"  self-test FAIL: {name}: expected offender={expect}, got {got}")
            failures += 1
    if failures:
        print(f"check-workflow-repo-env self-test: {failures} case(s) FAILED")
        return 1
    print(f"check-workflow-repo-env self-test: OK ({len(cases)} cases)")
    return 0


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--self-test", action="store_true")
    args = ap.parse_args()

    if args.self_test:
        return self_test()

    if self_test() != 0:
        return 1

    # Composite actions too (issue 1548): `setup-qemu-patched` ran
    # `just qemu setup-qemu` unsourced, outside this gate's old reach.
    docs = load_workflows(include_actions=True)
    bad = offenders(docs)
    if bad:
        print("check-workflow-repo-env: step(s) invoking just/nros/west without the repo environment:")
        for path, job, step, line in bad:
            print(f"  {path}  [{job}] {step}")
            print(f"      {line[:96]}")
        print()
        print("  Add `source ./activate.sh` as the first line of the step's `run:`.")
        # Do NOT spell the ROS setup path here. `check-ros-env-spelling` scans
        # tracked sources for hand-rolled ROS setup and cannot tell a command
        # from a sentence about one — the same distinction this gate makes for
        # `just`. Naming it in a help string made THIS file an offender.
        print("  It is a strict superset of sourcing the ROS setup script directly:")
        print("  it sources ROS itself and additionally exports `nano_ros_ROOT`,")
        print("  which is how `find_package(nano_ros)` resolves (issue 0933).")
        print("  It only ever PREPENDS to PATH, so `$GITHUB_PATH` entries survive.")
        return 1

    steps = sum(len(j.get("steps", []) or []) for _, d in docs for j in (d.get("jobs") or {}).values())
    print(
        f"check-workflow-repo-env: OK — {len(docs)} workflow(s)/composite action(s), {steps} step(s); "
        "every just/nros/west invocation sources the repo environment."
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())

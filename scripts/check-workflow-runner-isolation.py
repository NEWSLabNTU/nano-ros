#!/usr/bin/env python3
"""Gate: a self-hosted job must be unreachable from a fork's pull request.

nano-ros is a PUBLIC repo, and the self-hosted runners sit on machines that also
carry unrelated research work. `pull_request` from a fork runs contributor-authored
code; `pull_request_target` is worse, since it runs with a WRITE token. Either one
reaching a self-hosted runner hands an arbitrary contributor a shell on those
machines.

The safe triggers are the ones that require the code to already be trusted:
`push` and `merge_group` (post-review), `schedule` and `workflow_dispatch`
(repo-controlled).

This was TRUE but ungated until phase-407 — the property held by the care of
whoever last edited a workflow, which is the vacuous-gate class this repo keeps
finding. A `runs-on` line is one word away from unsafe and the failure is silent:
the workflow simply runs, on the wrong machine, for anyone who opens a PR.

Related: docs/development/multi-agent-ci-workflow.md ("Security — this is a
PUBLIC repo") and scripts/ci/runner-container.sh, which bounds what such a job
could reach but does NOT make it safe to admit one.
"""
import sys
import pathlib

try:
    import yaml
except ImportError:
    print("check-workflow-runner-isolation: PyYAML missing; run `just dev-tools --install`")
    sys.exit(0)

UNSAFE = {"pull_request", "pull_request_target"}


def _is_self_hosted(runs_on) -> bool:
    """A job is self-hosted if `runs-on` names the `self-hosted` label.

    Matching on our own `nros-*` labels too would be tempting, but they only
    ever appear alongside `self-hosted`, and a substring test would also fire
    on unrelated strings containing `nros-`.
    """
    if isinstance(runs_on, str):
        return runs_on == "self-hosted"
    if isinstance(runs_on, list):
        return "self-hosted" in runs_on
    if isinstance(runs_on, dict):  # `runs-on: {group: ..., labels: [...]}`
        labels = runs_on.get("labels") or []
        labels = [labels] if isinstance(labels, str) else labels
        return "self-hosted" in labels or bool(runs_on.get("group"))
    return False


def _triggers(doc) -> set:
    # PyYAML parses a bare `on:` key as the BOOLEAN True (YAML 1.1), so a
    # plain doc["on"] misses it and the gate would pass on every file.
    raw = doc.get(True, doc.get("on"))
    if isinstance(raw, dict):
        return set(raw.keys())
    if isinstance(raw, list):
        return set(raw)
    return {str(raw)} if raw is not None else set()


def check(workflow_dir: pathlib.Path):
    violations = []
    checked = 0
    for path in sorted(workflow_dir.glob("*.y*ml")):
        try:
            doc = yaml.safe_load(path.read_text()) or {}
        except yaml.YAMLError as exc:
            violations.append(f"{path}: unparseable ({exc.__class__.__name__})")
            continue
        unsafe = _triggers(doc) & UNSAFE
        for name, job in (doc.get("jobs") or {}).items():
            if not isinstance(job, dict) or not _is_self_hosted(job.get("runs-on")):
                continue
            checked += 1
            if unsafe:
                violations.append(
                    f"{path.name}: job '{name}' runs on a self-hosted runner and the "
                    f"workflow triggers on {sorted(unsafe)} — a fork's PR would run "
                    f"contributor code on our hardware. Move the job to a hosted "
                    f"runner, or drop the trigger."
                )
    return violations, checked


def _selftest(verbose: bool = True) -> int:
    """The gate must FAIL on the thing it exists to catch, not merely pass today."""
    import tempfile
    import textwrap

    safe = textwrap.dedent("""
        on: [push, merge_group]
        jobs:
          l3:
            runs-on: [self-hosted, linux, nros-big]
            steps: [{run: echo}]
    """)
    unsafe = textwrap.dedent("""
        on:
          pull_request:
            branches: [main]
        jobs:
          l3:
            runs-on: [self-hosted, linux, nros-big]
            steps: [{run: echo}]
    """)
    hosted_pr = textwrap.dedent("""
        on: [pull_request]
        jobs:
          gate:
            runs-on: ubuntu-latest
            steps: [{run: echo}]
    """)
    ok = True
    with tempfile.TemporaryDirectory() as d:
        for label, body, want_violation, want_checked in (
            ("safe self-hosted", safe, False, 1),
            ("self-hosted on pull_request", unsafe, True, 1),
            ("hosted on pull_request", hosted_pr, False, 0),
        ):
            p = pathlib.Path(d) / "w"
            p.mkdir(exist_ok=True)
            for stale in p.glob("*.yml"):
                stale.unlink()
            (p / "wf.yml").write_text(body)
            v, checked = check(p)
            got = bool(v)
            if got != want_violation or checked != want_checked:
                print(f"  selftest FAIL [{label}]: violation={got} (want {want_violation}), "
                      f"checked={checked} (want {want_checked})")
                ok = False
            elif verbose:
                print(f"  selftest ok [{label}]")
    return 0 if ok else 1


def main() -> int:
    if "--selftest" in sys.argv:
        return _selftest()
    # The selftest runs on the NORMAL path, every time. A negative control kept
    # behind a flag is one nobody runs, and it decays into a comment — which is
    # the same vacuous-pass class this gate itself guards against.
    if _selftest(verbose=False) != 0:
        print("check-workflow-runner-isolation: the gate's own selftest failed; "
              "its verdict below cannot be trusted")
        return 1
    root = pathlib.Path(__file__).resolve().parent.parent
    violations, checked = check(root / ".github" / "workflows")
    if violations:
        print("check-workflow-runner-isolation: FAIL")
        for v in violations:
            print(f"  {v}")
        return 1
    print(f"check-workflow-runner-isolation: ok ({checked} self-hosted job(s), "
          f"none reachable from a fork PR)")
    return 0


if __name__ == "__main__":
    sys.exit(main())

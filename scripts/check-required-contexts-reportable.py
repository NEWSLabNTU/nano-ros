#!/usr/bin/env python3
"""Gate: a REQUIRED status check must be able to report on a pull request.

Issue 0975. `enable-merge-queue.sh` required `L3 (cross build + link)`, whose
job triggers only on `merge_group`. It therefore never reported against a PR,
so no PR could satisfy the required set, so none entered the queue — and
because none entered the queue, the `merge_group` event that would have run it
never fired. The deadlock sustains itself, and it presents as NOTHING: no red
check, no message, just PRs that quietly never merge. Seven sat in it.

Two rules, both about a check that produces no verdict:

  1. Every context the script can require must be produced by a job whose
     workflow triggers on `pull_request`.
  2. That job's `if:` must not depend on `vars.` or `secrets.`. Those are
     invisible from the repo — flipping a variable in Settings silently stops a
     REQUIRED check from reporting, and nothing in a diff shows it. This is
     issue 0883's shape one level down.

Deliberately static: it reads the SCRIPT's declared lists, not the live ruleset.
A gate that needs the network is one that gets skipped, and the script is where
the decision is authored anyway.

  3. That workflow's `pull_request` trigger carries NO `branches` /
     `branches-ignore` / `paths` / `paths-ignore` filter. PR #71 deadlocked on
     `branches: [main]` (a stacked PR's events were filtered out, and the
     retarget emits `edited`, which dispatches nothing); a `paths` filter does
     the same to any PR touching only other paths. The trigger NAME being
     present is not the trigger FIRING. CLAUDE.md: "never path-filter a
     required workflow".

phase-472 W4 — this gate was OFF in two ways while printing ok: a missing
PyYAML exited 0, and an empty `HOSTED_CHECKS=()` examined zero contexts. Both
fail now; the count examined is printed (`scripts/lib/population.py`), and an
unparseable workflow is an error rather than a file silently skipped (it may
be the one producing the required context).

Related: CLAUDE.md ("ONE required check, the aggregator CI — never add a job
name to the required set"), issues 0883, 0975.
"""
import io
import re
import sys
import pathlib

_ROOT = pathlib.Path(__file__).resolve().parent.parent
sys.path.insert(0, str(_ROOT / "scripts" / "lib"))
from population import require_population  # noqa: E402

try:
    import yaml
except ImportError:
    print("check-required-contexts-reportable: FAILED — PyYAML is missing, so no\n"
          "  workflow was read and nothing was verified. Install it:\n"
          "  `just dev-tools --install`.", file=sys.stderr)
    sys.exit(1)

# A filter on `pull_request` that can make a PR's event dispatch nothing.
PR_FILTERS = ("branches", "branches-ignore", "paths", "paths-ignore")

SCRIPT = "scripts/ci/enable-merge-queue.sh"
# Only arrays that actually feed the required set. SELF_HOSTED_CHECKS is
# deliberately NOT here: since 0975 it is descriptive, and including it would
# fail this gate for a list that no longer gates anything.
REQUIRING_ARRAYS = ["HOSTED_CHECKS"]
COND_VAR = re.compile(r"\b(vars|secrets)\.", re.I)


def parse_bash_array(text: str, name: str):
    m = re.search(rf"^{name}=\((.*?)^\)", text, re.S | re.M)
    if not m:
        return None
    return re.findall(r'"([^"]+)"', m.group(1))


def workflow_jobs(root: pathlib.Path, unparseable=None):
    """name -> list of (workflow, triggers, if-condition, pr-filters) producing
    that context. `unparseable`, when a list, receives workflows YAML refused."""
    out = {}
    for path in sorted((root / ".github" / "workflows").glob("*.y*ml")):
        try:
            doc = yaml.safe_load(path.read_text()) or {}
        except yaml.YAMLError:
            if unparseable is not None:
                unparseable.append(path.name)
            continue
        # PyYAML turns a bare `on:` key into the boolean True (YAML 1.1).
        raw = doc.get(True, doc.get("on"))
        trig = set(raw) if isinstance(raw, (dict, list)) else ({str(raw)} if raw else set())
        pr = raw.get("pull_request") if isinstance(raw, dict) else None
        filters = sorted(k for k in (pr or {}) if k in PR_FILTERS) if isinstance(pr, dict) else []
        for jid, job in (doc.get("jobs") or {}).items():
            if not isinstance(job, dict):
                continue
            out.setdefault(job.get("name", jid), []).append(
                (path.name, trig, str(job.get("if", "")), filters)
            )
    return out


def check(root: pathlib.Path):
    violations = []
    script = (root / SCRIPT)
    if not script.exists():
        return [f"{SCRIPT} is missing — this gate cannot verify anything"], 0
    text = script.read_text()
    unparseable = []
    jobs = workflow_jobs(root, unparseable)
    for name in unparseable:
        violations.append(
            f"{name}: not valid YAML — it may be the workflow that produces a "
            f"required context, so it cannot be skipped")
    checked = 0

    for arr in REQUIRING_ARRAYS:
        contexts = parse_bash_array(text, arr)
        if contexts is None:
            violations.append(f"{SCRIPT}: array {arr} not found — did it get renamed?")
            continue
        for ctx in contexts:
            checked += 1
            producers = jobs.get(ctx)
            if not producers:
                violations.append(
                    f"required context {ctx!r} ({arr}) is produced by NO job — "
                    f"it can never report, so every PR blocks forever")
                continue
            if not any("pull_request" in t for _, t, _, _ in producers):
                where = ", ".join(f"{w}({','.join(sorted(t))})" for w, t, _, _ in producers)
                violations.append(
                    f"required context {ctx!r} ({arr}) is produced only by: {where} — "
                    f"no `pull_request` trigger, so a PR can never satisfy it and "
                    f"can never enter the merge queue (issue 0975)")
                continue
            for wf, trig, cond, filters in producers:
                if "pull_request" in trig and filters:
                    violations.append(
                        f"required context {ctx!r} in {wf}: its `pull_request` trigger "
                        f"carries {', '.join(filters)} — a PR whose event the filter "
                        f"drops gets no check suite, and a required check that never "
                        f"reports blocks it forever (PR #71)")
                if "pull_request" in trig and COND_VAR.search(cond):
                    violations.append(
                        f"required context {ctx!r} in {wf} has `if:` depending on "
                        f"{cond.strip()!r} — a repo variable or secret can silently "
                        f"stop a REQUIRED check from reporting, and no diff shows it")
    return violations, checked


def _selftest(verbose: bool = True) -> int:
    """The gate must FAIL on 0975's actual shape, not merely pass on today's tree."""
    import tempfile
    import textwrap

    def build(d, script_body, wf):
        root = pathlib.Path(d)
        (root / "scripts" / "ci").mkdir(parents=True, exist_ok=True)
        (root / ".github" / "workflows").mkdir(parents=True, exist_ok=True)
        (root / SCRIPT).write_text(script_body)
        for stale in (root / ".github" / "workflows").glob("*.yml"):
            stale.unlink()
        (root / ".github" / "workflows" / "w.yml").write_text(wf)
        return root

    good_wf = textwrap.dedent("""
        on: [push, pull_request, merge_group]
        jobs:
          ci-ok: {name: CI, runs-on: ubuntu-latest, if: always(), steps: [{run: echo}]}
    """)
    mg_only_wf = textwrap.dedent("""
        on: [merge_group]
        jobs:
          ci-ok: {name: CI, runs-on: ubuntu-latest, steps: [{run: echo}]}
    """)
    var_gated_wf = textwrap.dedent("""
        on: [push, pull_request]
        jobs:
          ci-ok:
            name: CI
            runs-on: ubuntu-latest
            if: ${{ vars.SOMETHING == 'true' }}
            steps: [{run: echo}]
    """)
    script = 'HOSTED_CHECKS=(\n    "CI"\n)\n'

    branch_filtered_wf = textwrap.dedent("""
        on:
          push: {branches: [main]}
          pull_request: {branches: [main]}
        jobs:
          ci-ok: {name: CI, runs-on: ubuntu-latest, steps: [{run: echo}]}
    """)
    path_filtered_wf = textwrap.dedent("""
        on:
          pull_request:
            paths: ["packages/**"]
        jobs:
          ci-ok: {name: CI, runs-on: ubuntu-latest, steps: [{run: echo}]}
    """)
    cases = [
        ("reportable on PR", script, good_wf, False),
        ("pull_request with a branches filter (PR #71)", script, branch_filtered_wf, True),
        ("pull_request with a paths filter", script, path_filtered_wf, True),
        ("unparseable workflow", script, "on: [pull_request\njobs: {", True),
        ("merge_group-only (0975)", script, mg_only_wf, True),
        ("gated on a repo variable", script, var_gated_wf, True),
        ("context nothing produces", 'HOSTED_CHECKS=(\n    "Ghost"\n)\n', good_wf, True),
    ]
    ok = True
    with tempfile.TemporaryDirectory() as d:
        # phase-472 W4 — an empty required array examines zero contexts.
        _, n = check(build(d, "HOSTED_CHECKS=(\n)\n", good_wf))
        silent = io.StringIO()
        if require_population(n, "context(s)", gate="probe", out=silent, err=silent):
            print("  selftest FAIL [empty required array]: zero contexts read as a pass")
            ok = False
        elif verbose:
            print("  selftest ok [empty required array]")
        for label, body, wf, want in cases:
            v, _ = check(build(d, body, wf))
            if bool(v) != want:
                print(f"  selftest FAIL [{label}]: violation={bool(v)} want={want}")
                ok = False
            elif verbose:
                print(f"  selftest ok [{label}]")
    return 0 if ok else 1


def main() -> int:
    if "--selftest" in sys.argv:
        return _selftest()
    # The selftest runs on the NORMAL path: a negative control behind a flag is
    # one nobody runs, and it decays into a comment.
    if _selftest(verbose=False) != 0:
        print("check-required-contexts-reportable: own selftest failed; "
              "the verdict below cannot be trusted")
        return 1
    violations, checked = check(_ROOT)
    if not require_population(checked, "required context(s)",
                              gate="check-required-contexts-reportable"):
        return 1
    if violations:
        print("check-required-contexts-reportable: FAIL")
        for v in violations:
            print(f"  {v}")
        return 1
    print(f"check-required-contexts-reportable: ok ({checked} required context(s), "
          f"each reportable on a pull request)")
    return 0


if __name__ == "__main__":
    sys.exit(main())

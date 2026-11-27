#!/usr/bin/env python3
"""One workflow run's results, per registry board -- RFC-0114 D13 (phase-585 W2).

A board's evidence is what its tests resolved, never a hand-kept list: the
fixture resolver writes an evidence ledger line per fixture a test resolves
(`NROS_TEST_EVIDENCE_DIR`, phase-585 W1), and this joins those lines with the
run's junit into per-board outcomes. A job that never reached its tests leaves
no line, so each job is also classified by its failing STEP (the classifier
`nightly-triage.py` already uses, imported, not copied), and a board expected
in that job with no evidence reads `no-verdict` or `failed` from it.

Outcomes (D13.2): `passed`, `failed`, `no-verdict` (only CI-side failures, or a
cancelled job), `not-run` (expected here, no job ran for it).

Inputs, all local files (the workflow downloads them):
  --jobs <file>       `gh run view <id> --json jobs` output
  --artifacts <dir>   the run's downloaded artifacts: junit XML and ledger JSONL
  --run-meta k=v ...  workflow, event, run, url, sha, date (recorded verbatim)
Outputs:
  --out <file>        board-results.json (schema 1)
  --summary <file>    a Markdown table, appended ($GITHUB_STEP_SUMMARY)

A coordinate whose platform no registry row carries is an ERROR naming the test:
a result with no board must not be dropped.

Usage:
    board-results.py --jobs J --artifacts D --out O [--summary S] [--run-meta k=v ...]
    board-results.py --self-test
"""

from __future__ import annotations

import argparse
import importlib.util
import json
import re
import sys
import xml.etree.ElementTree as ET
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
STATUS_JSON = ROOT / "packages/boards/board-support-status.json"
MATRIX_RS = ROOT / "packages/testing/nros-tests/src/matrix.rs"
SCHEMA = 1

# Any skip marker, bare or classed (`[SKIPPED]`, `[SKIPPED:lane]`, …): a
# classed skip does not contain the bare one, so a literal match would read
# every classed skip as a failure (issue 0658). A skip is no evidence.
SKIP_MARKER = re.compile(r"\[SKIPPED(?::[\w-]+)?\]")


def _triage():
    """`nightly-triage.py`'s module (a dashed file name, so loaded by path and
    registered first, as its dataclass-free code still expects)."""
    path = Path(__file__).with_name("nightly-triage.py")
    spec = importlib.util.spec_from_file_location("nightly_triage", path)
    mod = importlib.util.module_from_spec(spec)
    sys.modules["nightly_triage"] = mod
    spec.loader.exec_module(mod)
    return mod


def platform_of_token(matrix_rs: str) -> dict[str, str]:
    """fixture token -> `PlatformId` variant, from `PlatformId::fixture_tokens`
    (the one place the token vocabulary is spelled)."""
    m = re.search(r"pub const fn fixture_tokens\(self\).*?\n    \}", matrix_rs, re.S)
    body = re.sub(r"//[^\n]*", "", m.group(0) if m else "")
    out = {}
    for variant, toks in re.findall(r"PlatformId::(\w+)\s*=>\s*&\[([^\]]*)\]", body):
        for t in re.findall(r'"([^"]+)"', toks):
            out[t] = variant
    return out


def rows_by_platform(status: dict) -> dict[str, list[dict]]:
    out: dict[str, list[dict]] = {}
    for r in status["rows"]:
        if r.get("matrix_platform") and not r.get("infra"):
            out.setdefault(r["matrix_platform"], []).append(r)
    return out


def artifact_of(root: Path, f: Path) -> str:
    """The artifact a downloaded file came from: its first directory under
    `root` (download-artifact puts each artifact in a directory of its name),
    or "" for a file at the top."""
    rel = f.relative_to(root).parts
    return rel[0] if len(rel) > 1 else ""


def read_ledgers(root: Path) -> list[dict]:
    """Every ledger line, each tagged `_artifact` with the artifact (the JOB)
    it came from, so its verdict is read from that job's junit."""
    lines = []
    for f in sorted(root.rglob("*.jsonl")):  # walk-ok: one run's downloaded artifacts
        for raw in f.read_text().splitlines():
            if raw.strip():
                lines.append({**json.loads(raw), "_artifact": artifact_of(root, f)})
    return lines


def read_junit(root: Path) -> dict[tuple[str, str, str], str]:
    """(artifact, binary, test) -> passed|failed|skipped, from every junit
    under `root`. Keyed by ARTIFACT (issue 2997): one test name runs in
    several jobs, lane-skipped in all but its own, so a run-wide (binary,
    test) key let the last job read overwrite the job that ran it. NuttxRiscv's
    lane skips of the `rtos_e2e` Nuttx cases erased NuttxArm's passes every
    night, and the board read "no verdict".
    A `junit-real.xml` (lane skips rewritten as skips) wins over its sibling
    `junit.xml`; a failure whose message is a lane skip is a skip."""
    files = sorted(root.rglob("junit*.xml"))  # walk-ok: one run's downloaded artifacts
    real_dirs = {f.parent for f in files if f.name == "junit-real.xml"}
    out = {}
    for f in files:
        if f.name == "junit.xml" and f.parent in real_dirs:
            continue
        for case in ET.parse(f).getroot().iter("testcase"):
            binary = (case.get("classname") or "").split("::")[-1]
            name = case.get("name") or ""
            fail = case.find("failure")
            if fail is None:
                fail = case.find("error")
            if case.find("skipped") is not None:
                verdict = "skipped"
            elif fail is not None:
                text = (fail.get("message") or "") + (fail.text or "")
                verdict = "skipped" if SKIP_MARKER.search(text) else "failed"
            else:
                verdict = "passed"
            out[(artifact_of(root, f), binary, name)] = verdict
    return out


def expected_rows(status: dict, workflow: str, jobs: list[dict]) -> dict[str, str]:
    """row key -> the job name that runs its cells in this workflow."""
    names = {j.get("name", "") for j in jobs}
    out = {}
    for r in status["rows"]:
        if r.get("infra"):
            continue
        if workflow == "nightly.yml" and r.get("nightly_job"):
            out[r["key"]] = r["nightly_job"]
        elif workflow == "weekly.yml":
            for t in r.get("probe_tracks") or []:
                job = f"book probe — {t}"
                if job in names:
                    out.setdefault(r["key"], job)
    return out


def collect(jobs: list[dict], ledgers: list[dict], junit: dict, status: dict,
            tokens: dict[str, str], workflow: str) -> tuple[dict, list[str]]:
    triage = _triage()
    by_platform = rows_by_platform(status)
    boards: dict[str, dict] = {}
    errors = []

    def board(key):
        return boards.setdefault(key, {"cells": []})

    seen = set()
    for ln in ledgers:
        variant = tokens.get(ln.get("platform", ""))
        rows = by_platform.get(variant or "", [])
        if not rows:
            errors.append(
                f"test `{ln.get('binary')}::{ln.get('test')}` resolved fixture row "
                f"`{ln.get('row')}` on platform `{ln.get('platform')}`, which no "
                "registry row carries: give the board a row in "
                "packages/boards/board-support.toml")
            continue
        verdict = junit.get(
            (ln.get("_artifact", ""), ln.get("binary", ""), ln.get("test", "")), "unknown")
        if verdict == "skipped":
            continue
        cell = f"{ln['platform']}/{ln['lang']}/{ln['rmw']}"
        for r in rows:
            k = (r["key"], cell, ln.get("binary"), ln.get("test"))
            if k in seen:
                continue
            seen.add(k)
            board(r["key"])["cells"].append({
                "cell": cell, "test": f"{ln.get('binary')}::{ln.get('test')}",
                "verdict": verdict})

    by_job = {j.get("name", ""): j for j in jobs}
    for key, job_name in expected_rows(status, workflow, jobs).items():
        b = board(key)
        job = by_job.get(job_name)
        if job is None:
            b.update(job=job_name, **{"class": "not-run"})
            continue
        cls, step = triage.classify(job)
        b.update(job=job_name, step=step, **{"class": cls})

    for key, b in boards.items():
        verdicts = {c["verdict"] for c in b["cells"]}
        cls = b.get("class")
        if "failed" in verdicts:
            b["outcome"] = "failed"
        elif b["cells"]:
            b["outcome"] = "passed" if cls in (None, "pass") else (
                "failed" if cls == "verdict" else "passed")
        elif cls == "verdict":
            b["outcome"] = "failed"
        elif cls in ("no-verdict", "cancelled"):
            b["outcome"] = "no-verdict"
        elif cls == "pass":
            # The job passed and resolved none of the board's fixtures: no
            # evidence that its cells ran.
            b["outcome"] = "no-verdict"
            b["step"] = "the job passed, and its tests resolved no fixture of this board"
        else:
            b["outcome"] = "not-run"
    return boards, errors


MARK = {"passed": "✓", "failed": "✗", "no-verdict": "–", "not-run": "–"}


def summary_md(doc: dict) -> str:
    rows = ["## Board results", "",
            "| Board | Outcome | Cells | Failing | Why |",
            "| --- | --- | --- | --- | --- |"]
    for key, b in sorted(doc["boards"].items()):
        cells = b["cells"]
        failing = [c["cell"] for c in cells if c["verdict"] == "failed"]
        n_ok = sum(c["verdict"] == "passed" for c in cells)
        why = ""
        if b["outcome"] == "failed":
            why = f"verdict: {b.get('step') or 'a cell failed'}"
        elif b["outcome"] == "no-verdict" and b.get("class") == "pass":
            why = f"no evidence: {b.get('step')}"
        elif b["outcome"] == "no-verdict":
            why = f"did not run: {b.get('step') or 'a CI problem'}"
        elif b["outcome"] == "not-run":
            why = "no job ran it"
        rows.append(f"| `{key}` | {MARK[b['outcome']]} {b['outcome']} | "
                    f"{n_ok}/{len(cells)} | {', '.join(failing) or '-'} | {why or '-'} |")
    return "\n".join(rows) + "\n"


def main(argv=None) -> int:
    ap = argparse.ArgumentParser(description=__doc__.split("\n")[0])
    ap.add_argument("--jobs", type=Path)
    ap.add_argument("--artifacts", type=Path)
    ap.add_argument("--out", type=Path)
    ap.add_argument("--summary", type=Path)
    ap.add_argument("--run-meta", nargs="*", default=[])
    ap.add_argument("--self-test", action="store_true")
    ap.add_argument("--check", action="store_true",
                    help="the gate: the self-test, then every registry platform maps to "
                         "a fixture token and back")
    a = ap.parse_args(argv)
    if a.self_test:
        return self_test()
    if a.check:
        return check()
    if not (a.jobs and a.artifacts and a.out):
        ap.error("--jobs, --artifacts and --out are required")
    meta = dict(kv.split("=", 1) for kv in a.run_meta)
    jobs = json.loads(a.jobs.read_text()).get("jobs", [])
    status = json.loads(STATUS_JSON.read_text())
    boards, errors = collect(jobs, read_ledgers(a.artifacts), read_junit(a.artifacts),
                             status, platform_of_token(MATRIX_RS.read_text()),
                             meta.get("workflow", ""))
    doc = {"schema": SCHEMA, "source": "ci", **meta, "boards": boards}
    a.out.write_text(json.dumps(doc, indent=2) + "\n")
    if a.summary:
        with a.summary.open("a") as f:
            f.write(summary_md(doc))
    for e in errors:
        print(f"board-results: {e}", file=sys.stderr)
    return 1 if errors else 0


def check() -> int:
    """The gate (`check lane-stage-reporting`): the self-test on the normal
    path, then RFC-0114 D13.8's mapping both ways. A registry platform with no
    fixture token can never be evidenced; a token variant with no registry row
    would make every result on it a refused, boardless result."""
    if self_test():
        return 1
    tok = set(platform_of_token(MATRIX_RS.read_text()).values())
    status = json.loads(STATUS_JSON.read_text())
    plats = {r["matrix_platform"] for r in status["rows"]
             if r.get("matrix_platform") and not r.get("infra")}
    bad = [f"registry platform `{p}` has no `PlatformId::fixture_tokens` entry, so no "
           "test can evidence it" for p in sorted(plats - tok)]
    bad += [f"`PlatformId::{p}` has fixture tokens but no registry row, so a result on it "
            "has no board: add its row to packages/boards/board-support.toml"
            for p in sorted(tok - plats)]
    for b in bad:
        print(f"board-results --check: {b}", file=sys.stderr)
    print(f"board-results --check: {len(plats)} registry platforms, "
          f"{len(bad)} problem(s)")
    return 1 if bad else 0


def self_test() -> int:
    import tempfile
    fails = []

    def check(name, cond):
        if not cond:
            fails.append(name)

    status = {"rows": [
        {"key": "tx@ThreadxLinux", "matrix_platform": "ThreadxLinux",
         "nightly_job": "ThreadxLinux", "probe_tracks": ["threadx-linux"]},
        {"key": "fr@FreertosMps2", "matrix_platform": "FreertosMps2",
         "nightly_job": "FreertosMps2", "probe_tracks": []},
        {"key": "lx@Linux", "matrix_platform": "Linux", "nightly_job": None,
         "probe_tracks": []},
        {"key": "nx@NuttxArm", "matrix_platform": "NuttxArm",
         "nightly_job": "NuttxArm", "probe_tracks": []},
        {"key": "rv@ThreadxRiscv64", "matrix_platform": "ThreadxRiscv64",
         "nightly_job": "ThreadxRiscv64", "probe_tracks": []},
        {"key": "infra", "infra": True, "matrix_platform": None},
    ]}
    tokens = {"threadx-linux": "ThreadxLinux", "freertos": "FreertosMps2",
              "linux": "Linux", "nuttx": "NuttxArm", "threadx-riscv64": "ThreadxRiscv64"}
    step = lambda n, c: {"name": n, "conclusion": c}  # noqa: E731
    jobs = [
        {"name": "ThreadxLinux", "conclusion": "failure",
         "steps": [step("Set up job", "success"), step("Run tests", "failure")]},
        {"name": "FreertosMps2", "conclusion": "success", "steps": []},
        {"name": "NuttxArm", "conclusion": "failure",
         "steps": [step("Set up Zephyr 3.7 workspace", "failure")]},
        # ThreadxRiscv64 has no job in this run.
    ]
    ledgers = [
        {"binary": "emulator", "test": "tx_pubsub", "row": "r1", "platform": "threadx-linux",
         "lang": "c", "rmw": "zenoh"},
        {"binary": "emulator", "test": "fr_pubsub", "row": "r2", "platform": "freertos",
         "lang": "c", "rmw": "zenoh"},
        # A board with no nightly job of its own, evidenced by another job.
        {"binary": "owned", "test": "linux_talker", "row": "r3", "platform": "linux",
         "lang": "c", "rmw": "zenoh"},
        {"binary": "emulator", "test": "skipped_one", "row": "r4", "platform": "freertos",
         "lang": "cpp", "rmw": "zenoh"},
    ]
    junit = {("", "emulator", "tx_pubsub"): "failed", ("", "emulator", "fr_pubsub"): "passed",
             ("", "owned", "linux_talker"): "passed", ("", "emulator", "skipped_one"): "skipped"}
    b, errs = collect(jobs, ledgers, junit, status, tokens, "nightly.yml")
    check("a failed cell fails its board", b["tx@ThreadxLinux"]["outcome"] == "failed")
    check("the failing cell is named",
          b["tx@ThreadxLinux"]["cells"][0]["cell"] == "threadx-linux/c/zenoh")
    check("a passing board passes", b["fr@FreertosMps2"]["outcome"] == "passed")
    check("a lane-skipped test is no evidence", len(b["fr@FreertosMps2"]["cells"]) == 1)
    check("a board riding another job is evidenced", b["lx@Linux"]["outcome"] == "passed")
    check("a set-up failure is no verdict", b["nx@NuttxArm"]["outcome"] == "no-verdict")
    check("an expected board with no job is not run",
          b["rv@ThreadxRiscv64"]["outcome"] == "not-run")
    check("no infra row is reported", "infra" not in b)
    check("nothing unmapped", errs == [])
    _, errs = collect([], [{"binary": "x", "test": "t", "row": "r", "platform": "nowhere",
                            "lang": "c", "rmw": "zenoh"}], {}, status, tokens, "nightly.yml")
    check("an unmapped coordinate is refused, naming the test",
          len(errs) == 1 and "x::t" in errs[0] and "nowhere" in errs[0])
    md = summary_md({"boards": b})
    check("the summary has a row per board, marks and why",
          "| `tx@ThreadxLinux` | ✗ failed |" in md and "did not run" in md)
    tok = platform_of_token('pub const fn fixture_tokens(self) -> &[&str] {\n'
                            '        match self {\n'
                            '            // a comment PlatformId::Nope => &["no"],\n'
                            '            PlatformId::ThreadxLinux => &["threadx-linux"],\n'
                            '            PlatformId::Native => &["linux", "native"],\n'
                            '        }\n    }')
    check("the token map reads fixture_tokens, not comments",
          tok == {"threadx-linux": "ThreadxLinux", "linux": "Native", "native": "Native"})
    check("the real token map names threadx-linux",
          platform_of_token(MATRIX_RS.read_text()).get("threadx-linux") == "ThreadxLinux")
    with tempfile.TemporaryDirectory() as d:
        p = Path(d)
        (p / "a").mkdir()
        (p / "a" / "junit.xml").write_text(
            '<testsuites><testsuite name="nros-tests::emulator">'
            '<testcase classname="nros-tests::emulator" name="ok"/>'
            '<testcase classname="nros-tests::emulator" name="bad"><failure message="x"/></testcase>'
            '<testcase classname="nros-tests::emulator" name="lane">'
            '<failure message="[SKIPPED:lane] out of lane"/></testcase>'
            '<testcase classname="nros-tests::emulator" name="bare">'
            '<failure message="[SKIPPED] gone"/></testcase>'
            '</testsuite></testsuites>')
        j = read_junit(p)
        check("junit: pass, fail and a lane skip read as such",
              j == {("a", "emulator", "ok"): "passed", ("a", "emulator", "bad"): "failed",
                    ("a", "emulator", "lane"): "skipped", ("a", "emulator", "bare"): "skipped"})
        (p / "a" / "1.jsonl").write_text('{"test": "ok"}\n\n{"test": "bad"}\n')
        check("ledgers: one record per line", len(read_ledgers(p)) == 2)

    # Issue 2997 -- one test name in two jobs: it RAN in NuttxArm and was
    # lane-skipped in NuttxRiscv, whose artifact sorts later. Each job's
    # ledger line reads its own job's verdict, so NuttxArm's pass stands.
    with tempfile.TemporaryDirectory() as d:
        p = Path(d)
        case = ('<testsuites><testsuite name="nros-tests::rtos_e2e">'
                '<testcase classname="nros-tests::rtos_e2e" name="t::Nuttx"{}</testsuite></testsuites>')
        for job, body in (("nightly-NuttxArm-junit", "/>"),
                          ("nightly-NuttxRiscv-junit",
                           '><failure message="[SKIPPED:lane] out of lane"/></testcase>')):
            (p / job / "nextest").mkdir(parents=True)
            (p / job / "nextest" / "junit.xml").write_text(case.format(body))
        (p / "nightly-NuttxArm-junit" / "1.jsonl").write_text(
            '{"binary": "rtos_e2e", "test": "t::Nuttx", "row": "r", "platform": "nuttx",'
            ' "lang": "c", "rmw": "zenoh"}\n')
        b, _ = collect([{"name": "NuttxArm", "conclusion": "success", "steps": []}],
                       read_ledgers(p), read_junit(p), status, tokens, "nightly.yml")
        check("a test lane-skipped in another job keeps the verdict of the job that ran it",
              b["nx@NuttxArm"]["outcome"] == "passed"
              and [c["verdict"] for c in b["nx@NuttxArm"]["cells"]] == ["passed"])
    for f in fails:
        print(f"board-results self-test FAIL: {f}", file=sys.stderr)
    print(f"board-results self-test: {'OK' if not fails else f'{len(fails)} failed'}")
    return 1 if fails else 0


if __name__ == "__main__":
    sys.exit(main())

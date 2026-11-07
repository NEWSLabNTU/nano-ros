#!/usr/bin/env python3
"""Every example a workflow builds with `just <platform> build-one` exists.

`just <platform> build-one <lang>/<name>` builds the leaf at
`examples/<platform>/<lang>/<name>`. A workflow names that leaf either
literally or through a matrix list (`build-one "${{ matrix.example }}"`), and
nothing checked the name against the tree: phase-535 W4 (#3233) deleted
`examples/zephyr/{c,cpp}/listener`, the nightly Zephyr matrix still listed
them, and the first anyone heard of it was four red nightly jobs reporting
`no example at …`.

This reads every `.github/workflows/*.yml`, finds each `build-one` call, and
resolves the example it names — the literal argument, or every value of the
matrix key it interpolates — to a directory.

    python3 scripts/check-workflow-build-one-paths.py
    python3 scripts/check-workflow-build-one-paths.py --self-test
"""
from __future__ import annotations

import re
import sys
import tempfile
from pathlib import Path

import yaml

ROOT = Path(__file__).resolve().parent.parent

# `just zephyr build-one "${{ matrix.example }}" zenoh`, `just zephyr build-one c/talker`
CALL = re.compile(
    r"\bjust\s+(?P<plat>[a-z0-9_-]+)\s+build-one\s+"
    r"(?:\"|')?(?:\$\{\{\s*matrix\.(?P<key>[A-Za-z0-9_-]+)\s*\}\}|(?P<lit>[a-z]+/[a-z0-9_-]+))"
)


def _matrix_values(job: dict, key: str) -> list[str] | None:
    matrix = ((job.get("strategy") or {}).get("matrix")) or {}
    if not isinstance(matrix, dict):
        return None
    vals = list(matrix.get(key) or [])
    for inc in matrix.get("include") or []:
        if isinstance(inc, dict) and key in inc:
            vals.append(inc[key])
    return [str(v) for v in vals] if vals else None


def scan(root: Path) -> list[str]:
    problems: list[str] = []
    for wf in sorted((root / ".github" / "workflows").glob("*.yml")):
        doc = yaml.safe_load(wf.read_text()) or {}
        rel = wf.relative_to(root)
        for job_id, job in (doc.get("jobs") or {}).items():
            if not isinstance(job, dict):
                continue
            for step in job.get("steps") or []:
                run = step.get("run") if isinstance(step, dict) else None
                if not run:
                    continue
                for m in CALL.finditer(run):
                    plat = m.group("plat")
                    if m.group("lit"):
                        names = [m.group("lit")]
                    else:
                        names = _matrix_values(job, m.group("key"))
                        if names is None:
                            problems.append(
                                f"{rel} job `{job_id}`: build-one reads matrix.{m.group('key')}, "
                                "which the job's strategy.matrix does not list"
                            )
                            continue
                    for name in names:
                        leaf = root / "examples" / plat / name
                        if not leaf.is_dir():
                            problems.append(
                                f"{rel} job `{job_id}`: `just {plat} build-one {name}` names "
                                f"examples/{plat}/{name}, which does not exist"
                            )
    return problems


def self_test() -> None:
    wf = """
jobs:
  ok:
    strategy:
      matrix:
        example: [c/talker]
    steps:
      - run: just zephyr build-one "${{ matrix.example }}" zenoh
  gone:
    strategy:
      matrix:
        example: [c/talker, c/listener]
    steps:
      - run: just zephyr build-one "${{ matrix.example }}" zenoh
  literal:
    steps:
      - run: just zephyr build-one cpp/listener zenoh
  unlisted:
    steps:
      - run: just zephyr build-one "${{ matrix.example }}"
"""
    with tempfile.TemporaryDirectory() as d:
        root = Path(d)
        (root / ".github" / "workflows").mkdir(parents=True)
        (root / ".github" / "workflows" / "t.yml").write_text(wf)
        (root / "examples" / "zephyr" / "c" / "talker").mkdir(parents=True)
        got = scan(root)
    want = [
        "job `gone`: `just zephyr build-one c/listener`",
        "job `literal`: `just zephyr build-one cpp/listener`",
        "job `unlisted`: build-one reads matrix.example",
    ]
    assert len(got) == 3, got
    for w in want:
        assert any(w in g for g in got), (w, got)
    assert not any("job `ok`" in g for g in got), got
    print("check-workflow-build-one-paths self-test: OK (4 cases)")


def main() -> int:
    if sys.argv[1:] == ["--self-test"]:
        self_test()
        return 0
    self_test()
    problems = scan(ROOT)
    if problems:
        print("check-workflow-build-one-paths: FAIL")
        for p in problems:
            print(f"  {p}")
        print("Point the entry at a leaf that exists, or drop it and say where its coverage went.")
        return 1
    print("check-workflow-build-one-paths: OK")
    return 0


if __name__ == "__main__":
    sys.exit(main())

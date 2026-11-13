#!/usr/bin/env python3
"""The nightly platform sweep's job matrix: one job per BOARD (phase-558).

User ruling, 2026-11-13: "Have results per board instead of per platform
family". The sweep used to run one job per `just` module, so the three
registry rows that share `nightly_token = "freertos"` had one verdict between
them, and one red board withdrew the daily evidence for all of them.

This derives the matrix from `packages/boards/board-support.toml` and never
from a second list:

  * a row is a job when it promises `cadence = "daily"`, names its board as
    `matrix_platform`, and its `nightly_token` (the `just` module whose verbs
    build and test it) is one of the modules the caller says it can run;
  * the job's NAME is the row's `matrix_platform`. That is the board's
    identity everywhere a verdict is read (`tier-health.py`,
    `record-lane-runs.py`, `nightly-triage.py`), so a red job names exactly one
    board;
  * the job runs its module's verbs narrowed to that board's coordinates
    (`NROS_SCOPE_BOARD`, read by `nros_scope_coords_file`).

Zephyr is not here: its rows' daily jobs are the 05:00 zephyr jobs, and a
module the caller does not list is simply not selected.

Usage:
    nightly-boards.py --modules "qemu freertos nuttx"     # JSON on stdout
    nightly-boards.py --self-test
"""

from __future__ import annotations

import argparse
import json
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent.parent / "lib"))
import board_registry  # noqa: E402

ROOT = Path(__file__).resolve().parents[2]
REGISTRY = ROOT / "packages" / "boards" / "board-support.toml"


def boards(rows: list[dict], modules: set[str]) -> list[dict]:
    """`[{"board": <matrix_platform>, "module": <nightly_token>}]`, sorted."""
    out = []
    for row in rows:
        if row.get("infra") or row.get("cadence") != "daily":
            continue
        board, module = row.get("matrix_platform"), row.get("nightly_token")
        if board and module in modules:
            out.append({"board": board, "module": module})
    out.sort(key=lambda e: (e["module"], e["board"]))
    seen = [e["board"] for e in out]
    dups = sorted({b for b in seen if seen.count(b) > 1})
    if dups:
        # Two daily rows on one board would be two jobs with one name, and a
        # verdict that names a board could no longer say which row it is.
        raise SystemExit(f"nightly-boards: board(s) {dups} appear on two daily rows")
    return out


def self_test() -> int:
    rows = [
        {"matrix_platform": "FreertosMps2", "nightly_token": "freertos", "cadence": "daily"},
        {"matrix_platform": "FreertosPosix", "nightly_token": "freertos", "cadence": "daily"},
        {"crate": "x", "nightly_token": "freertos", "runs": "none"},
        {"matrix_platform": "NuttxArm", "nightly_token": "nuttx", "cadence": "daily"},
        {"matrix_platform": "ZephyrNativeSim", "nightly_token": "zephyr", "cadence": "daily"},
        {"matrix_platform": "Linux", "cadence": "daily"},
        {"matrix_platform": "Fvp", "nightly_token": "zephyr", "cadence": "manual"},
    ]
    got = boards(rows, {"freertos", "nuttx"})
    want = [
        {"board": "FreertosMps2", "module": "freertos"},
        {"board": "FreertosPosix", "module": "freertos"},
        {"board": "NuttxArm", "module": "nuttx"},
    ]
    if got != want:
        print(f"nightly-boards self-test FAIL: {got} != {want}")
        return 1
    if boards(rows, set()):
        print("nightly-boards self-test FAIL: no runnable module selected a board")
        return 1
    try:
        boards(rows + [rows[0]], {"freertos"})
    except SystemExit:
        pass
    else:
        print("nightly-boards self-test FAIL: a duplicated board was not refused")
        return 1
    # The live registry splits at least one module into several boards, which
    # is what this file exists for.
    live = boards(board_registry.load(REGISTRY).get("board", []),
                  {"qemu", "freertos", "nuttx", "threadx_linux", "threadx_riscv64"})
    per_module: dict[str, int] = {}
    for e in live:
        per_module[e["module"]] = per_module.get(e["module"], 0) + 1
    if not any(n > 1 for n in per_module.values()):
        print(f"nightly-boards self-test FAIL: no module owns two daily boards: {live}")
        return 1
    print(f"nightly-boards self-test: OK ({len(live)} daily boards on the live registry)")
    return 0


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    ap.add_argument("--modules", default="", help="space-separated `just` modules this job set can run")
    ap.add_argument("--self-test", action="store_true")
    args = ap.parse_args()
    if args.self_test:
        return self_test()
    # The negative control runs on the normal path too, so the derivation the
    # nightly matrix comes from cannot stop being checked. Its report goes to
    # stderr: stdout is the matrix JSON the workflow reads.
    import contextlib
    with contextlib.redirect_stdout(sys.stderr):
        if self_test() != 0:
            return 1
    rows = board_registry.load(REGISTRY).get("board", [])
    print(json.dumps(boards(rows, set(args.modules.split())), separators=(",", ":")))
    return 0


if __name__ == "__main__":
    sys.exit(main())

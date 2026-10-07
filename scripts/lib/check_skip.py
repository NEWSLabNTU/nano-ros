#!/usr/bin/env python3
"""The Python spelling of `nros_check_unverified` — phase-472 F2.

A gate whose precondition is absent here (a tool, a build output, a store)
reaches issue 1043's middle outcome, NOT VERIFIED: never a quiet rc=0. This
records the skip in the SAME ledger the shell lanes read
(`scripts/build/check-skip.sh`), by calling the shell function — so the
ledger's path has exactly one derivation — and returns the exit code the gate
should use: 0, or 1 under `NROS_CHECK_SKIP_STRICT=1`.

    return check_skip.unverified("my-gate", "no store at ...")

(`NROS_CHECK_SKIP_LEDGER`, which two gates read, was never set by any lane —
issue 1345 — so a skip written there reached nobody.)
"""

from __future__ import annotations

import os
import subprocess
import sys
from pathlib import Path

REPO = Path(__file__).resolve().parents[2]
LIB = REPO / "scripts" / "build" / "check-skip.sh"


def unverified(name: str, reason: str) -> int:
    r = subprocess.run(
        ["bash", "-c", f'. "{LIB}" && nros_check_unverified "$0" "$1"', name, reason],
        cwd=REPO,
    )
    return 0 if r.returncode == 0 else 1


def self_test() -> None:
    r = subprocess.run(["bash", "-c", f'. "{LIB}" && nros_check_unverified_self_test'],
                       cwd=REPO, capture_output=True, text=True,
                       env={k: v for k, v in os.environ.items() if k != "NROS_CHECK_SKIP_STRICT"})
    if r.returncode != 0:
        raise SystemExit(f"check_skip self-test FAILED: {r.stderr.strip()}")
    env = dict(os.environ, NROS_CHECK_SKIP_STRICT="1")
    r = subprocess.run(
        ["bash", "-c", f'. "{LIB}" && nros_check_unverified "$0" "$1"', "x", "probe"],
        cwd=REPO, env=env, capture_output=True, text=True)
    if r.returncode == 0:
        raise SystemExit("check_skip self-test FAILED: strict mode passed")
    # The PYTHON entry point too, not only the shell function it wraps: the
    # 2026-10-07 re-audit short-circuited `unverified()` and nothing noticed.
    saved = os.environ.get("NROS_CHECK_SKIP_STRICT")
    os.environ["NROS_CHECK_SKIP_STRICT"] = "1"
    try:
        if unverified("check-skip-selftest", "probe") != 1:
            raise SystemExit("check_skip self-test FAILED: unverified() passed under strict mode")
    finally:
        if saved is None:
            del os.environ["NROS_CHECK_SKIP_STRICT"]
        else:
            os.environ["NROS_CHECK_SKIP_STRICT"] = saved


if __name__ == "__main__":
    self_test()
    print("check_skip self-test: OK")

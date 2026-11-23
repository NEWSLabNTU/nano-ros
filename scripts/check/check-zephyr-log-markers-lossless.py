#!/usr/bin/env python3
"""A Zephyr image's thread-context log messages wait for buffer space (issue 2847).

A test reads its evidence off the console, and on an image with deferred
logging every console line is a log-buffer message. `zephyr/Kconfig` makes a
thread whose message finds the buffer full wait for room instead of losing it
(`LOG_BLOCK_IN_THREAD`, bounded at `LOG_BLOCK_IN_THREAD_TIMEOUT_MS` = 10000),
so no marker a test reads can be dropped. This gate holds that in two places:

  1. `zephyr/Kconfig` still carries both defaults, with the condition
     `NROS && LOG_MODE_DEFERRED`;
  2. no tracked Kconfig fragment turns them back: a `.conf` that sets
     `CONFIG_LOG_BLOCK_IN_THREAD=n` (or `# ... is not set`), or a timeout
     other than 10000 or -1 (forever, which is lossless too).

Usage:
    check-zephyr-log-markers-lossless.py [--selftest]

The self-test runs on every call: each refusal is shown to fire on a
synthetic input, so a parser that matched nothing cannot read as green.
"""

from __future__ import annotations

import re
import subprocess
import sys
from pathlib import Path

REPO = Path(__file__).resolve().parents[2]
KCONFIG = REPO / "zephyr" / "Kconfig"
CONDITION = "NROS && LOG_MODE_DEFERRED"
TIMEOUT = "10000"

BLOCK_OFF = re.compile(
    r"^\s*(?:CONFIG_LOG_BLOCK_IN_THREAD\s*=\s*n\b|#\s*CONFIG_LOG_BLOCK_IN_THREAD is not set)"
)
TIMEOUT_SET = re.compile(r"^\s*CONFIG_LOG_BLOCK_IN_THREAD_TIMEOUT_MS\s*=\s*(-?\d+)")


def kconfig_problems(text: str) -> list[str]:
    """What is missing from the module Kconfig's two defaults."""
    problems = []
    wanted = {
        "LOG_BLOCK_IN_THREAD": f"default y if {CONDITION}",
        "LOG_BLOCK_IN_THREAD_TIMEOUT_MS": f"default {TIMEOUT} if {CONDITION}",
    }
    for symbol, default in wanted.items():
        block = re.search(
            rf"^config {symbol}\n((?:[ \t]+.*\n?)+)", text, flags=re.MULTILINE
        )
        if block is None:
            problems.append(f"zephyr/Kconfig: no second definition of {symbol}")
        elif not re.search(
            rf"^\s+{re.escape(default)}\s*$", block.group(1), flags=re.MULTILINE
        ):
            problems.append(f"zephyr/Kconfig: {symbol} lacks `{default}`")
    return problems


def conf_problems(path: str, text: str) -> list[str]:
    """Lines in one Kconfig fragment that let a thread's log message drop."""
    problems = []
    for number, line in enumerate(text.splitlines(), start=1):
        if BLOCK_OFF.match(line):
            problems.append(f"{path}:{number}: turns LOG_BLOCK_IN_THREAD off")
            continue
        found = TIMEOUT_SET.match(line)
        if found and found.group(1) not in (TIMEOUT, "-1"):
            problems.append(
                f"{path}:{number}: LOG_BLOCK_IN_THREAD_TIMEOUT_MS={found.group(1)}"
                f" (lossless needs {TIMEOUT} or -1)"
            )
    return problems


def tracked_confs() -> list[str]:
    out = subprocess.run(
        ["git", "-C", str(REPO), "ls-files", "-z", "--", "*.conf"],
        check=True,
        capture_output=True,
        text=True,
    ).stdout
    return [p for p in out.split("\0") if p and not p.startswith("third-party/")]


def selftest() -> list[str]:
    """Each refusal must fire on a case built to trip it."""
    failures = []
    good = (
        "config LOG_BLOCK_IN_THREAD\n"
        f"    default y if {CONDITION}\n\n"
        "config LOG_BLOCK_IN_THREAD_TIMEOUT_MS\n"
        f"    default {TIMEOUT} if {CONDITION}\n"
    )
    if kconfig_problems(good):
        failures.append("a correct Kconfig was refused")
    if not kconfig_problems(good.replace("default y", "default n")):
        failures.append("a Kconfig defaulting LOG_BLOCK_IN_THREAD to n passed")
    if not kconfig_problems(good.replace(TIMEOUT, "1000")):
        failures.append("a Kconfig with a 1000 ms timeout passed")
    if not kconfig_problems("config FOO\n    default y\n"):
        failures.append("a Kconfig without either definition passed")
    cases = {
        "CONFIG_LOG_BLOCK_IN_THREAD=n\n": True,
        "# CONFIG_LOG_BLOCK_IN_THREAD is not set\n": True,
        "CONFIG_LOG_BLOCK_IN_THREAD_TIMEOUT_MS=500\n": True,
        "CONFIG_LOG_BLOCK_IN_THREAD_TIMEOUT_MS=10000\n": False,
        "CONFIG_LOG_BLOCK_IN_THREAD_TIMEOUT_MS=-1\n": False,
        "CONFIG_LOG_BLOCK_IN_THREAD=y\n": False,
        "CONFIG_LOG_BUFFER_SIZE=4096\n": False,
    }
    for text, refused in cases.items():
        if bool(conf_problems("case.conf", text)) != refused:
            verb = "passed" if refused else "was refused"
            failures.append(f"{text.strip()!r} {verb}")
    return failures


def main() -> int:
    failures = selftest()
    if failures:
        print("check-zephyr-log-markers-lossless --selftest: FAILED")
        for failure in failures:
            print(f"  {failure}")
        return 1
    if "--selftest" in sys.argv[1:]:
        print("check-zephyr-log-markers-lossless --selftest: OK")
        return 0

    problems = kconfig_problems(KCONFIG.read_text())
    confs = tracked_confs()
    for path in confs:
        problems += conf_problems(path, (REPO / path).read_text(errors="replace"))
    if problems:
        print(
            "check-zephyr-log-markers-lossless: a thread's log message could be"
            " dropped, and a test marker with it (issue 2847):"
        )
        for problem in problems:
            print(f"  {problem}")
        return 1
    print(
        f"check-zephyr-log-markers-lossless: OK (both Kconfig defaults present;"
        f" {len(confs)} tracked .conf file(s), none opts out)"
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())

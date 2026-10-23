#!/usr/bin/env python3
"""The in-checkout Zephyr SDK path is ASKED of the ladder, never constructed.

Issue 1254 moved the Zephyr SDK into the store and gave it one resolver,
`scripts/lib/zephyr-sdk.sh` (override, then store, then the legacy in-checkout
copy). The four `just` recipes that built `$(pwd)/scripts/zephyr/sdk/
zephyr-sdk-<v>` by hand were moved onto it. A fifth was added later by issue
1279 (the per-user SDK registration in `just zephyr setup`), after that sweep,
and spelled the legacy path again. On a host provisioned into the store, or in
any fresh worktree, that path does not exist, `ensure-sdk-registered.sh` exits
0 on "nothing to do", and the store SDK is never registered: 1279's failure,
silently, through the arm 1254 had moved. Nothing reported it.

THE RULE. A line of code that names `scripts/zephyr/sdk/zephyr-sdk-` (the SDK
root a consumer would hand to cmake) lives in an OWNER. Everyone else asks
`scripts/lib/zephyr-sdk.sh resolve`. Comments are exempt: a comment that
records the old spelling is history, not a reader.

Scope is every tracked shell, `just`, Python and workflow file, read through
`git ls-files` (tracked files only, no tree walk).
"""

from __future__ import annotations

import re
import subprocess
import sys
from pathlib import Path

REPO = Path(__file__).resolve().parents[2]

NEEDLE = "scripts/zephyr/sdk/zephyr-sdk-"

# path -> why it may spell the legacy root itself.
OWNERS = {
    "scripts/lib/zephyr-sdk.sh":
        "the ladder: `nros_zephyr_sdk_checkout_dir` IS the legacy arm",
    "scripts/ci/runner-doctor.sh":
        "its own SDK ladder ends in the checkout default and self-tests that arm; "
        "it reads the store arm from the resolver",
    "scripts/check/check-zephyr-sdk-path-asked.py":
        "this gate: it names the needle and its self-test cases",
}

GLOBS = ["justfile", "*.just", "*.sh", "*.py", ".github/workflows/*.yml"]

COMMENT = re.compile(r"^\s*(#|//)")


def offenders(path: str, text: str) -> list[tuple[int, str]]:
    if path in OWNERS:
        return []
    out = []
    for n, line in enumerate(text.splitlines(), 1):
        if NEEDLE in line and not COMMENT.match(line):
            out.append((n, line.strip()))
    return out


def self_test() -> None:
    """Both directions, on synthetic input (`check-gate-selftests`)."""
    bad = 'bash x.sh \\\n    "$(pwd)/scripts/zephyr/sdk/zephyr-sdk-$v"\n'
    assert offenders("just/zephyr-setup.just", bad) == [
        (2, '"$(pwd)/scripts/zephyr/sdk/zephyr-sdk-$v"')
    ], "selftest: the 1254/1279 construction was not caught"
    assert offenders("just/x.just", "    # was `scripts/zephyr/sdk/zephyr-sdk-0.16.8`\n") == [], (
        "selftest: a comment recording the old spelling must not count"
    )
    assert offenders("scripts/lib/zephyr-sdk.sh", bad) == [], (
        "selftest: an owner must be allowed to spell its own arm"
    )
    assert offenders("just/x.just", 'd="$(bash scripts/lib/zephyr-sdk.sh resolve)"\n') == [], (
        "selftest: asking the resolver must pass"
    )


def main() -> int:
    files = subprocess.run(
        ["git", "ls-files", "--", *GLOBS],
        cwd=REPO, capture_output=True, text=True, check=True,
    ).stdout.split()
    if not files:
        print("check-zephyr-sdk-path-asked: git ls-files listed nothing; "
              "that is a broken gate, not a passing one.", file=sys.stderr)
        return 1
    for owner in OWNERS:
        if owner not in files:
            print(f"check-zephyr-sdk-path-asked: owner {owner} is not tracked any "
                  "more; drop it from OWNERS.", file=sys.stderr)
            return 1

    found = []
    for path in files:
        try:
            text = (REPO / path).read_text(errors="replace")
        except OSError:
            continue
        found += [(path, n, line) for n, line in offenders(path, text)]

    if found:
        print("check-zephyr-sdk-path-asked: FAILED (issue 1254) — the legacy "
              "in-checkout SDK path is constructed outside its owners:\n")
        for path, n, line in found:
            print(f"  {path}:{n}: {line}")
        print("\nAsk the ladder instead, which tries the override, then the store, "
              "then this path:\n"
              '  if sdk_dir="$(bash scripts/lib/zephyr-sdk.sh --version "$v" resolve)"; then …\n'
              "A constructed path names a directory that does not exist on a host "
              "provisioned into the store, and every consumer here treats an absent "
              "SDK dir as nothing to do.")
        return 1

    print(f"check-zephyr-sdk-path-asked: OK ({len(files)} tracked files, "
          f"{len(OWNERS)} owners)")
    return 0


if __name__ == "__main__":
    self_test()
    sys.exit(main())

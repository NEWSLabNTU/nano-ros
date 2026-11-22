#!/usr/bin/env python3
"""Every script that runs `west update` re-applies the line's patch set (issue 2764).

`west update` writes a workspace's west projects, so whatever in-place edits
nano-ros's patch set (`scripts/zephyr/patches/<line>.sh`) made there are no
longer guaranteed. A writer that does not re-apply the set leaves a workspace
that reads like any other until a configure fails, minutes in, with an error
about Zephyr internals. Two writers had each decided for themselves what to
re-apply:

- the manifest sync relied on its caller;
- `scripts/zephyr/setup.sh`, which documents itself as runnable on its own,
  applied two of the set's ~20 patches by name and recorded nothing.

The rule: a shell script, `just` file or workflow whose CODE runs `west update`
must also call `nros_zephyr_apply_patchset` (scripts/lib/zephyr-patchset.sh),
the one helper that drops the stale record, runs the set and lets it record
itself. Comments and echoed text are not code. `tests/` is out of scope: a
test stubs west and drives the scripts under test, which this gate covers.

Run:  python3 scripts/check-west-update-patchset.py [--self-test]
"""
from __future__ import annotations

import re
import subprocess
import sys
from pathlib import Path

REPO = Path(__file__).resolve().parent.parent
HELPER = "nros_zephyr_apply_patchset"
# `west update`, `python -m west update`, `"${west_cmd[@]}" update`.
WEST_UPDATE = re.compile(r'(?:\bwest\b|west_cmd\[@\]\}"?)\s+update\b')
CALLS_HELPER = re.compile(rf"(?:^|[\s;&|(]){HELPER}\b")
# A line whose first word only prints is text, not an invocation.
PRINTS = re.compile(r"^\s*(?:-\s+)?(?:run:\s*)?(?:echo|printf|log_\w+|nros_\w*log\w*)\b")


def code_lines(text: str):
    for n, raw in enumerate(text.splitlines(), 1):
        line = raw.strip()
        if not line or line.startswith("#"):
            continue
        if PRINTS.match(line):
            continue
        # Drop a trailing comment. A `#` inside quotes is rare in these files
        # and would only make the scan see LESS code, never invent a call.
        yield n, re.split(r"\s#", raw, maxsplit=1)[0]


def scan_text(text: str) -> tuple[list[int], bool]:
    """(lines that run `west update`, whether the helper is called)."""
    updates = [n for n, line in code_lines(text) if WEST_UPDATE.search(line)]
    helper = any(CALLS_HELPER.search(line) for _, line in code_lines(text))
    return updates, helper


def in_scope(path: str) -> bool:
    if path.startswith("tests/") or "/tests/" in path:
        return False
    if path == "scripts/lib/zephyr-patchset.sh":
        return False  # the helper's own definition
    return (
        (path.startswith("scripts/") and path.endswith(".sh"))
        or (path.startswith("just/") and path.endswith(".just"))
        or path == "justfile"
        or (path.startswith(".github/workflows/") and path.endswith((".yml", ".yaml")))
    )


def tracked() -> list[str]:
    out = subprocess.run(["git", "ls-files"], cwd=REPO, capture_output=True,
                         text=True, check=True).stdout
    return [p for p in out.splitlines() if in_scope(p)]


def self_test() -> None:
    bad = "west init -l x\nwest update\n"
    assert scan_text(bad) == ([2], False), scan_text(bad)
    good = bad + 'nros_zephyr_apply_patchset "$co" "$ws" setup || exit 1\n'
    assert scan_text(good) == ([2], True), scan_text(good)
    # the three invocation shapes
    for shape in ['"$py" -m west update', '"${west_cmd[@]}" update', '(cd "$ws" && west update) || rc=$?']:
        assert scan_text(shape + "\n")[0] == [1], shape
    # text and comments are not code
    for text in ['# run west update first', 'echo "running west update"',
                 'log_info "west update"', "  - run: echo west update"]:
        assert scan_text(text + "\n") == ([], False), text
    # a helper named only in a comment does not count
    assert scan_text("west update\n# nros_zephyr_apply_patchset later\n") == ([1], False)
    assert not in_scope("tests/zephyr-sync-workspace-manifest-tests.sh")
    assert in_scope("scripts/zephyr/setup.sh") and in_scope("just/zephyr-setup.just")
    print("check-west-update-patchset self-test: OK (11 cases)")


def main() -> int:
    self_test()
    if "--self-test" in sys.argv:
        return 0
    offenders = []
    writers = 0
    for path in tracked():
        p = REPO / path
        if not p.is_file():
            continue
        updates, helper = scan_text(p.read_text(errors="replace"))
        if updates:
            writers += 1
            if not helper:
                offenders.append((path, updates))
    if offenders:
        print("check-west-update-patchset: FAIL — a script runs `west update` and never "
              "re-applies the line's patch set:")
        for path, lines in offenders:
            print(f"  {path}: line(s) {', '.join(map(str, lines))}")
        print(f"  After the update, call `{HELPER} <checkout> <workspace> <who> || exit 1`\n"
              "  (scripts/lib/zephyr-patchset.sh). It drops the stale record, runs\n"
              "  scripts/zephyr/patches/<line>.sh and lets it record itself. Without it\n"
              "  the workspace reads like any other until a configure fails.")
        return 1
    print(f"check-west-update-patchset: OK — {writers} script(s) run `west update`, "
          f"and each re-applies the patch set through {HELPER}")
    return 0


if __name__ == "__main__":
    sys.exit(main())

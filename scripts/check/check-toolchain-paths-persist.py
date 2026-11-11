#!/usr/bin/env python3
"""No path under cmake/toolchain/ ever disappears from the tree.

## Why

CMake records the toolchain file's path in
`<build>/CMakeFiles/<version>/CMakeSystem.cmake` at a build dir's FIRST
configure, and includes that recorded path on every later configure -- even
when `-DCMAKE_TOOLCHAIN_FILE` names a different file. So renaming or deleting
a toolchain file strands every build dir configured against the old name at
"include could not find requested file". PR #3500 renamed
`armv7a-nuttx-eabi.cmake` to `armv7a-nuttx-eabihf.cmake` and stopped tier 2
that way; PR #3513 put the old name back as a one-line forward.

## The rule

Every path that has ever existed under cmake/toolchain/ is named in
`.config/cmake-toolchain-paths.txt`, and every named path is tracked: as the
real file, or as a FORWARD -- a file that includes the new one,

    include("${CMAKE_CURRENT_LIST_DIR}/<new-name>.cmake")

A path may leave the tree only with a `retired: <reason>` on its line.

Checked both ways: a tracked file the list does not name fails (a new
toolchain file is added to the list in the same change), and so does a named
path that is not tracked. Every `include("${CMAKE_CURRENT_LIST_DIR}/...")`
from one file in the directory to another must name a file that exists, so a forward to a missing
target (or a chain ending in one) fails too.

## Why a committed list and not `git log`

The verdict must not depend on how deep the clone is. CI runners check out
shallow, and a shallow history answers "nothing was deleted" for a rename it
cannot see (issue 1476: a negative from truncated history is not evidence).
The list holds the whole history in the tree, so the main half is exact in any
clone.

What the list alone cannot see is a change that deletes the file AND its list
line. That is the BASELINE half: the list as it stood at ONE baseline commit
is read (`git show <base>:.config/cmake-toolchain-paths.txt`), and every
path it names must still be named now (live, or `retired: <reason>`). The
baseline is the one `check-submodule-pins` compares against, through the same
channel: `$NROS_SUBMODULE_PIN_BASELINE` (CI exports it from
`scripts/ci/pin-baseline.sh provide`, which fetches that commit WITH its tree),
else `pin-baseline.sh resolve` (locally the merge-base with origin/main). So
the deletion is caught at the change that makes it.

No history walk. An earlier version walked `git log --diff-filter=DR --
cmake/toolchain/`, which in CI's treeless history (`--filter=tree:0
--unshallow`, scripts/ci/provision-gate-lane.sh) fetched a tree from the
network per commit and ran into the 40-minute nextest cap. One baseline is
two trees and one blob, and it is read bounded (no prompt, stdin closed, a timeout).

No baseline, a baseline commit not in the clone, or a read that fails or
times out: in CI (GITHUB_ACTIONS) a
FAILURE naming `pin-baseline.sh provide`, because every CI job that runs a gate
lane runs provide first (the same precondition `check-submodule-pins` already
fails hard on in CI), so there it is a provisioning defect, and the harness
would read a skip as red anyway; locally NOT VERIFIED (exit 78, the recipe
records it in the skip ledger). Never a pass.

## Scope

cmake/toolchain/ only, because that is the only directory a CMAKE_TOOLCHAIN_FILE
is taken from: every board descriptor's `toolchain_file` (checked here, so a
new descriptor cannot point elsewhere unnoticed) and every just recipe's
`toolchain=` name a file in it. cmake/board/*.cmake modules are include()d by a
CMakeLists.txt that is itself regenerated, so no build dir keeps their path.

Self-test on the normal path: planted cases, each must give its verdict.
"""

from __future__ import annotations

import os
import re
import subprocess
import sys
from pathlib import Path


REPO = Path(__file__).resolve().parents[2]
DIR = "cmake/toolchain"
LIST = ".config/cmake-toolchain-paths.txt"
INCLUDE = re.compile(r'include\(\s*"?\$\{CMAKE_CURRENT_LIST_DIR\}/([^")\s]+)"?')
DESCRIPTOR_TC = re.compile(r'^\s*toolchain_file\s*=\s*"([^"]+)"', re.M)

WHY = (
    "  CMake records the toolchain file's path in\n"
    "  <build>/CMakeFiles/<version>/CMakeSystem.cmake at the first configure and\n"
    "  includes that recorded path on every later configure, even when\n"
    "  -DCMAKE_TOOLCHAIN_FILE names another file. Every build dir configured\n"
    "  against a vanished path stops configuring.\n"
    "  Fix: keep the old name as a forwarding file containing\n"
    '    include("${CMAKE_CURRENT_LIST_DIR}/<new-name>.cmake")\n'
    f"  and keep its line in {LIST}."
)


def parse_list(text: str) -> tuple[dict[str, str | None], list[str]]:
    """{path: retired-reason or None}, plus format problems."""
    entries: dict[str, str | None] = {}
    problems: list[str] = []
    for n, raw in enumerate(text.splitlines(), 1):
        line = raw.strip()
        if not line or line.startswith("#"):
            continue
        path, sep, rest = line.partition(" ")
        reason = None
        rest = rest.strip()
        if rest:
            if not rest.startswith("retired:") or not rest[len("retired:"):].strip():
                problems.append(f"  {LIST}:{n}: after the path only `retired: <reason>` may follow")
                continue
            reason = rest[len("retired:"):].strip()
        if not path.startswith(DIR + "/"):
            problems.append(f"  {LIST}:{n}: `{path}` is not under {DIR}/")
            continue
        entries[path] = reason
    return entries, problems


def check(listed: dict[str, str | None], tracked: set[str],
          includes: dict[str, list[str]], exists) -> list[str]:
    """The list half. `includes` maps a tracked file to its include targets
    (already joined to the file's dir and normalised)."""
    problems = []
    for p in sorted(listed):
        if listed[p] is None and p not in tracked:
            problems.append(f"  {p}: listed, and no longer in the tree")
    for p in sorted(tracked - listed.keys()):
        problems.append(f"  {p}: in the tree, and not in {LIST} (add it)")
    for p in sorted(p for p, r in listed.items() if r is not None and p in tracked):
        problems.append(f"  {p}: marked retired but still in the tree (drop the `retired:`)")
    for src in sorted(includes):
        for tgt in includes[src]:
            if not exists(tgt):
                problems.append(f"  {src}: includes {tgt}, which does not exist")
    return problems


def include_targets(rel: str, text: str) -> list[str]:
    """Targets inside cmake/toolchain/ only: a file elsewhere is not one a
    build dir recorded, and its own directory's gates own it."""
    base = os.path.dirname(rel)
    out = [os.path.normpath(os.path.join(base, t)) for t in INCLUDE.findall(text)]
    return [t for t in out if t.startswith(DIR + "/")]


def selftest() -> None:
    fwd = 'include("${CMAKE_CURRENT_LIST_DIR}/new.cmake")\n'
    a, n = f"{DIR}/old.cmake", f"{DIR}/new.cmake"
    on_disk = {a, n}
    inc = {a: include_targets(a, fwd)}
    assert inc[a] == [n], inc
    assert include_targets(a, 'include("${CMAKE_CURRENT_LIST_DIR}/../X.cmake")') == []
    # A forward to an existing file passes.
    assert not check({a: None, n: None}, {a, n}, inc, on_disk.__contains__)
    # A deleted path still listed fails.
    assert check({a: None, n: None}, {n}, {}, on_disk.__contains__)
    # A forward whose target is gone fails.
    assert check({a: None, n: None}, {a}, inc, {a}.__contains__)
    # A new file not in the list fails.
    assert check({n: None}, {a, n}, {}, on_disk.__contains__)
    # Retired with a reason passes; retired yet present fails.
    assert not check({a: "board gone", n: None}, {n}, {}, on_disk.__contains__)
    assert check({a: "board gone", n: None}, {a, n}, {}, on_disk.__contains__)
    # The baseline half: a path the baseline's list named and HEAD's list
    # dropped (file and line deleted together) fails; turning it into
    # `retired: x` passes; a baseline with no list compares nothing.
    assert check_baseline({n: None}, {a: None, n: None})
    assert not check_baseline({a: "x", n: None}, {a: None, n: None})
    assert not check_baseline({n: None}, {})
    # Format: a bare word after the path, or an empty reason, is refused.
    assert parse_list(f"{a} because\n")[1]
    assert parse_list(f"{a} retired:\n")[1]
    assert parse_list(f"{a} retired: why\n")[0] == {a: "why"}


# Every git call is bounded: no credential prompt, stdin closed, a timeout.
# The baseline reads may fetch LAZILY in a treeless clone, on purpose: the
# path `<base>:.config/cmake-toolchain-paths.txt` is two trees (root, .config)
# and one blob, a handful of round trips. What made the old history walk
# unbounded was one tree per COMMIT, and there is no walk any more.
GIT_ENV = {**os.environ, "GIT_TERMINAL_PROMPT": "0"}
GIT_TIMEOUT_S = 60


def git(*args: str) -> str:
    return subprocess.run(["git", "-C", str(REPO), *args], check=True,
                          capture_output=True, text=True, env=GIT_ENV,
                          stdin=subprocess.DEVNULL, timeout=GIT_TIMEOUT_S).stdout


def resolve_baseline() -> str | None:
    """The baseline `check-submodule-pins` uses, by the same channel."""
    env = os.environ.get("NROS_SUBMODULE_PIN_BASELINE", "").strip()
    if env:
        return env
    try:
        r = subprocess.run(["bash", str(REPO / "scripts/ci/pin-baseline.sh"),
                            "resolve", "HEAD"], cwd=REPO, capture_output=True,
                           text=True, env=GIT_ENV, stdin=subprocess.DEVNULL,
                           timeout=GIT_TIMEOUT_S)
    except subprocess.TimeoutExpired:
        return None
    out = r.stdout.strip()
    return out if r.returncode == 0 and out else None


def check_baseline(listed: dict[str, str | None], base_listed: dict[str, str | None]) -> list[str]:
    """Every path the baseline's list names is still named (live or retired)."""
    return [f"  {p}: named in {LIST} at the baseline, and dropped from it now "
            f"(keep the line, or mark it `retired: <reason>`)"
            for p in sorted(base_listed.keys() - listed.keys())]


def baseline_list(base: str) -> tuple[dict[str, str | None] | None, str | None]:
    """(the list at <base>, None), or (None, why it cannot be read here).
    A baseline with no list yet gives ({}, None): nothing to compare."""
    try:
        git("rev-parse", "--verify", "--quiet", f"{base}^{{commit}}")
    except subprocess.CalledProcessError:
        return None, (f"the baseline {base} is not in this clone "
                      "(CI: `scripts/ci/pin-baseline.sh provide` fetches it)")
    except subprocess.TimeoutExpired:
        return None, f"resolving {base} did not finish in {GIT_TIMEOUT_S} s"
    try:
        try:
            git("rev-parse", "--verify", "--quiet", f"{base}:{LIST}")
        except subprocess.CalledProcessError:
            return {}, None  # the baseline predates the list: nothing to compare
        text = git("show", f"{base}:{LIST}")
    except subprocess.TimeoutExpired:
        return None, f"reading {LIST} at {base} did not finish in {GIT_TIMEOUT_S} s"
    except subprocess.CalledProcessError as e:
        return None, f"reading {LIST} at {base} failed: {e.stderr.strip()}"
    entries, _ = parse_list(text)
    return entries, None


def main() -> int:
    selftest()
    listed, problems = parse_list((REPO / LIST).read_text(encoding="utf-8"))
    tracked = set(git("ls-files", "--", DIR).split("\n")) - {""}
    includes = {}
    for f in tracked:
        try:
            includes[f] = include_targets(f, (REPO / f).read_text(encoding="utf-8"))
        except (UnicodeDecodeError, FileNotFoundError):
            continue
    problems += check(listed, tracked, includes, lambda p: (REPO / p).exists())
    for d in git("ls-files", "--", "*nros-board.toml").split("\n"):
        if not d:
            continue
        for tc in DESCRIPTOR_TC.findall((REPO / d).read_text(encoding="utf-8")):
            if not tc.startswith(DIR + "/"):
                problems.append(f"  {d}: toolchain_file `{tc}` is outside {DIR}/, "
                                "which this gate covers (move it, or widen the gate)")

    base = resolve_baseline()
    if base is None:
        base_entries, unverified = None, "no baseline commit resolves (pin-baseline.sh resolve)"
    else:
        base_entries, unverified = baseline_list(base)
    if base_entries is not None:
        problems += check_baseline(listed, base_entries)

    if problems:
        print(f"check-toolchain-paths-persist FAILED: {len(problems)} problem(s)")
        print("\n".join(problems))
        print(WHY)
        return 1
    if unverified:
        msg = (f"baseline half NOT VERIFIED: {unverified}, so a path deleted "
               f"from {DIR}/ together with its {LIST} line cannot be seen "
               f"({len(listed)} listed path(s) checked)")
        if os.environ.get("GITHUB_ACTIONS") == "true":
            # Every CI job that runs a gate lane runs `pin-baseline.sh provide`
            # first, so a missing baseline there is a provisioning defect.
            print(f"check-toolchain-paths-persist FAILED: {msg}. In CI the "
                  "baseline is provided by `bash scripts/ci/pin-baseline.sh "
                  "provide`, which must run before the gate lane.")
            return 1
        print(msg)
        return 78
    print(f"check-toolchain-paths-persist: OK ({len(listed)} path(s), "
          f"{len(tracked)} tracked, {len(base_entries)} at baseline {base[:12]})")
    return 0


if __name__ == "__main__":
    sys.exit(main())

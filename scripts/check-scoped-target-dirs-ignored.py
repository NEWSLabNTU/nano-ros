#!/usr/bin/env python3
"""Every scoped cargo target dir a recipe asks for must be ignored.

`nros_scoped_target_dir <suffix>` (scripts/build/cargo.sh) puts a gate's cargo
scratch at `$PWD/target-<suffix>` on a plain host -- a SIBLING of `target/`, so
the root `.gitignore`'s `/target/` line does not cover it. Those entries are
HAND-AUTHORED, one per suffix, and the map drifted: issue 1491 measured
`target-excluded-tests` at 329 MB, untracked, at the repo root, next to five
siblings that were ignored -- left by a gate on the merge-gating PR lane, which
is precisely the `git add -A` hazard CLAUDE.md names. `scripts/ci/disk-report.sh`
even asserted the dir was enumerated.

So the required entries are DERIVED from the call sites here rather than trusted
to a list somebody remembers to extend. One direction only: a suffix a recipe
asks for must be ignored. The reverse is not a defect -- an entry may outlive
its call site, and a stale ignore line costs nothing.

Self-tests run on the normal path (CLAUDE.md: a gate that works is not a gate
that runs).
"""

from __future__ import annotations

import os
import re
import subprocess
import sys
import tempfile
from pathlib import Path

# Issue 0986 — this file builds a throwaway repository with `git init`, and an
# inherited `GIT_DIR` makes that rewrite the CALLER's repository instead. Every
# sibling gate that plants a repo clears the environment first, and
# `check-hook-repo-side-effects` holds all of them to it.
sys.path.insert(0, str(Path(__file__).resolve().parent / "lib"))
from git_hook_env import nros_clear_inherited_git_env  # noqa: E402

CALL = re.compile(r"nros_scoped_target_dir\s+([A-Za-z0-9][A-Za-z0-9_.-]*)")
# The helper's own definition and its doc block spell the name; they are not calls.
DEFINING_FILE = "scripts/build/cargo.sh"
SCANNED_SUFFIXES = (".sh", ".just")
SCANNED_NAMES = ("justfile",)
# A tree with no scoped dir at all means the harvest broke, not that the tree is
# clean: three recipes use one today, and `check workspace-all` is one of them.
MIN_CALL_SITES = 3


def tracked_files(root: Path) -> list[Path]:
    out = subprocess.run(
        ["git", "ls-files", "-z"], cwd=root, capture_output=True, text=True, check=True
    ).stdout
    keep = []
    for rel in out.split("\0"):
        if not rel:
            continue
        if rel == DEFINING_FILE:
            continue
        name = rel.rsplit("/", 1)[-1]
        if rel.endswith(SCANNED_SUFFIXES) or name in SCANNED_NAMES:
            keep.append(Path(rel))
    return keep


def strip_comment(line: str) -> str:
    """Drop a trailing `#` comment. A `#` inside quotes is not one; be conservative
    and only strip when the hash is preceded by whitespace or starts the line."""
    out = []
    quote = None
    i = 0
    while i < len(line):
        ch = line[i]
        if quote:
            if ch == quote:
                quote = None
            out.append(ch)
        elif ch in "'\"":
            quote = ch
            out.append(ch)
        elif ch == "#" and (i == 0 or line[i - 1].isspace()):
            break
        else:
            out.append(ch)
        i += 1
    return "".join(out)


def call_sites(root: Path) -> list[tuple[str, int, str]]:
    found = []
    for rel in tracked_files(root):
        try:
            text = (root / rel).read_text(encoding="utf-8", errors="replace")
        except OSError:
            continue
        if "nros_scoped_target_dir" not in text:
            continue
        for n, line in enumerate(text.splitlines(), 1):
            code = strip_comment(line)
            for m in CALL.finditer(code):
                found.append((str(rel), n, m.group(1)))
    return found


def ignored_entries(root: Path) -> set[str]:
    path = root / ".gitignore"
    if not path.exists():
        return set()
    entries = set()
    for line in path.read_text(encoding="utf-8").splitlines():
        line = line.strip()
        if not line or line.startswith("#"):
            continue
        entries.add(line)
    return entries


def check(root: Path) -> tuple[list[str], int, int]:
    sites = call_sites(root)
    entries = ignored_entries(root)
    findings = []
    for rel, line, suffix in sorted(set(sites)):
        wanted = f"/target-{suffix}/"
        if wanted not in entries:
            findings.append(
                f"{rel}:{line}: `nros_scoped_target_dir {suffix}` writes "
                f"`target-{suffix}/` at the repo root, and the root `.gitignore` "
                f"does not enumerate `{wanted}` -- build output `git status` "
                f"reports as untracked (issue 1491)"
            )
    return findings, len(set(sites)), len({s for _, _, s in sites})


# --------------------------------------------------------------------------- #
# self-test
# --------------------------------------------------------------------------- #

def _plant(root: Path, gitignore: str, files: dict[str, str]) -> None:
    (root / ".gitignore").write_text(gitignore, encoding="utf-8")
    for rel, body in files.items():
        p = root / rel
        p.parent.mkdir(parents=True, exist_ok=True)
        p.write_text(body, encoding="utf-8")
    env = nros_clear_inherited_git_env(dict(os.environ))
    subprocess.run(["git", "init", "-q"], cwd=root, check=True, env=env)
    subprocess.run(["git", "add", "-A"], cwd=root, check=True, env=env)


def self_test() -> int:
    cases = 0
    failures = []

    def case(name: str, gitignore: str, files: dict[str, str], want: int):
        nonlocal cases
        cases += 1
        with tempfile.TemporaryDirectory() as td:
            root = Path(td)
            _plant(root, gitignore, files)
            findings, _, _ = check(root)
            if len(findings) != want:
                failures.append(f"{name}: expected {want} finding(s), got {len(findings)}: {findings}")

    ign_all = "/target/\n/target-embedded/\n/target-alpha/\n"
    case(
        "a call site with no ignore line is caught",
        "/target/\n",
        {"just/x.just": '    CARGO_TARGET_DIR="$(nros_scoped_target_dir alpha)" \\\n'},
        1,
    )
    case(
        "a call site WITH its ignore line is clean",
        ign_all,
        {"just/x.just": '    CARGO_TARGET_DIR="$(nros_scoped_target_dir alpha)" \\\n'},
        0,
    )
    case(
        "a commented-out call site is not a call site",
        "/target/\n",
        {"just/x.just": "    # CARGO_TARGET_DIR=$(nros_scoped_target_dir alpha)\n"},
        0,
    )
    case(
        "prose naming the helper is not a call site",
        "/target/\n",
        {"scripts/doc.sh": "# nros_scoped_target_dir puts scratch beside target/\n"},
        0,
    )
    case(
        "two suffixes, one missing",
        "/target/\n/target-embedded/\n",
        {
            "justfile": (
                'a: \n\texport CARGO_TARGET_DIR="$(nros_scoped_target_dir embedded)"\n'
                'b: \n\texport CARGO_TARGET_DIR="$(nros_scoped_target_dir beta)"\n'
            )
        },
        1,
    )
    case(
        "an ignore entry with no call site is NOT a finding",
        "/target/\n/target-embedded/\n/target-gone/\n",
        {"just/x.just": '  CARGO_TARGET_DIR="$(nros_scoped_target_dir embedded)"\n'},
        0,
    )
    case(
        "a hash inside a quoted string does not truncate the line",
        "/target/\n",
        {"just/x.just": '  echo "# not a comment" ; CARGO_TARGET_DIR="$(nros_scoped_target_dir alpha)"\n'},
        1,
    )
    case(
        "an untracked file is out of scope",
        "/target/\n",
        {".gitignore.keep": "x\n"},
        0,
    )

    for f in failures:
        print(f"  FAIL {f}", file=sys.stderr)
    print(
        f"check-scoped-target-dirs-ignored self-test: {cases - len(failures)} passed,"
        f" {len(failures)} failed"
    )
    return 1 if failures else 0


def main() -> int:
    if "--selftest" in sys.argv or "--self-test" in sys.argv:
        return self_test()

    if self_test() != 0:
        print("check-scoped-target-dirs-ignored: FAILED — its own self-test does not pass", file=sys.stderr)
        return 1

    root = Path(
        subprocess.run(
            ["git", "rev-parse", "--show-toplevel"], capture_output=True, text=True, check=True
        ).stdout.strip()
    )
    findings, sites, suffixes = check(root)

    if sites < MIN_CALL_SITES:
        print(
            f"check-scoped-target-dirs-ignored: FAILED — harvested only {sites} call site(s);"
            f" at least {MIN_CALL_SITES} recipes use a scoped target dir, so the scan is broken,"
            " not the tree",
            file=sys.stderr,
        )
        return 1

    if findings:
        print("check-scoped-target-dirs-ignored: FAILED", file=sys.stderr)
        for f in findings:
            print(f"    {f}", file=sys.stderr)
        print(
            "\n  Add the line beside its siblings in the root `.gitignore`."
            " The entries are derived from these call sites, not from the list.",
            file=sys.stderr,
        )
        return 1

    print(
        f"check-scoped-target-dirs-ignored: OK ({sites} call site(s) over"
        f" {suffixes} suffix(es); every scoped target dir is ignored)"
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())

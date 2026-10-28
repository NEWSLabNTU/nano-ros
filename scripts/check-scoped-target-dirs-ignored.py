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

A suffix built from an interpolation (`board-transport-variants-{name}` in a
Python f-string, `foo-${x}` in shell) names a FAMILY of dirs, so it is
satisfied by a glob entry covering it (`/target-board-transport-variants-*/`).
Python callers are scanned too: issue 2116's gate reached the helper from a
`.py` file, which this scan could not see, so its ignore line was hand-added.

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
import sys as _w3_sys  # noqa: E402
from pathlib import Path as _W3Path  # noqa: E402
_w3_sys.path.insert(0, str(_W3Path(__file__).resolve().parent / "lib"))
import comments  # noqa: E402  phase-472 W3 — the one comment stripper
import fnmatch

# Issue 0986 — this file builds a throwaway repository with `git init`, and an
# inherited `GIT_DIR` makes that rewrite the CALLER's repository instead. Every
# sibling gate that plants a repo clears the environment first, and
# `check-hook-repo-side-effects` holds all of them to it.
sys.path.insert(0, str(Path(__file__).resolve().parent / "lib"))
from git_hook_env import nros_clear_inherited_git_env  # noqa: E402

CALL = re.compile(r"nros_scoped_target_dir\s+([A-Za-z0-9][A-Za-z0-9_.{}$-]*)")
# An interpolation in a suffix -- `{expr}` (Python f-string), `${var}` or `$var`
# (shell) -- stands for any text, so the suffix becomes a glob.
INTERPOLATION = re.compile(r"\$\{[^}]*\}|\$[A-Za-z_][A-Za-z0-9_]*|\{[^}]*\}")
# The helper's own definition and its doc block spell the name; they are not calls.
# Nor are this gate's self-test fixtures, now that `.py` is scanned.
EXCLUDED_FILES = ("scripts/build/cargo.sh", "scripts/check-scoped-target-dirs-ignored.py")
SCANNED_SUFFIXES = (".sh", ".just", ".py")
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
        if rel in EXCLUDED_FILES:
            continue
        name = rel.rsplit("/", 1)[-1]
        if rel.endswith(SCANNED_SUFFIXES) or name in SCANNED_NAMES:
            keep.append(Path(rel))
    return keep


def strip_comments(text, rel):
    """Drop a trailing `#` comment. A `#` inside quotes is not one; be conservative
    and only strip when the hash is preceded by whitespace or starts the line."""
    # phase-472 W3 — the shared stripper (scripts/lib/comments.py).
    return comments.strip_comments(text, comments.lang_for(rel) or "sh")


def call_sites(root: Path) -> list[tuple[str, int, str]]:
    found = []
    for rel in tracked_files(root):
        try:
            text = (root / rel).read_text(encoding="utf-8", errors="replace")
        except OSError:
            continue
        if "nros_scoped_target_dir" not in text:
            continue
        for n, code in enumerate(strip_comments(text, rel).split("\n"), 1):
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


def as_glob(suffix: str) -> str:
    """The suffix with every interpolation turned into `*`."""
    return INTERPOLATION.sub("*", suffix)


def covered(wanted: str, entries: set[str]) -> bool:
    """Is `wanted` (`/target-<suffix>/`, maybe holding `*`) ignored by an entry?

    An exact line covers it; so does a glob entry that matches it with every
    `*` read as a literal stand-in, i.e. a glob at least as wide as the family.
    """
    if wanted in entries:
        return True
    probe = wanted.replace("*", "x")
    return any("*" in e and fnmatch.fnmatchcase(probe, e) for e in entries)


def check(root: Path) -> tuple[list[str], int, int]:
    sites = call_sites(root)
    entries = ignored_entries(root)
    findings = []
    for rel, line, suffix in sorted(set(sites)):
        suffix = as_glob(suffix)
        wanted = f"/target-{suffix}/"
        if not covered(wanted, entries):
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
        "a Python f-string call site with no glob entry is caught",
        "/target/\n",
        {"scripts/x.py": 'cmd = f"nros_scoped_target_dir fam-{name}"\n'},
        1,
    )
    case(
        "a Python f-string call site covered by a glob entry is clean",
        "/target/\n/target-fam-*/\n",
        {"scripts/x.py": 'cmd = f"nros_scoped_target_dir fam-{name}"\n'},
        0,
    )
    case(
        "a glob narrower than the family does not cover it",
        "/target/\n/target-fam-a*/\n",
        {"scripts/x.py": 'cmd = f"nros_scoped_target_dir fam-{name}"\n'},
        1,
    )
    case(
        "a shell interpolation is a family too",
        "/target/\n/target-fam-*/\n",
        {"just/x.just": '  D="$(nros_scoped_target_dir fam-${board})"\n'},
        0,
    )
    case(
        "a literal suffix is covered by a glob entry",
        "/target/\n/target-fam-*/\n",
        {"just/x.just": '  D="$(nros_scoped_target_dir fam-one)"\n'},
        0,
    )
    case(
        "a Python comment naming the helper is not a call site",
        "/target/\n",
        {"scripts/x.py": '# nros_scoped_target_dir alpha\n'},
        0,
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

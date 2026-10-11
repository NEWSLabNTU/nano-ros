#!/usr/bin/env python3
"""RFC-0103 D6 (amended 2026-10-12) — no Rust or CMake code reads `$NROS_REPO_DIR`.

The `nros` binary knows its own SDK root (`nros_launcher::checkout::own_sdk_root`:
a released binary's `share/nano-ros`, an in-tree binary's checkout), every other
reader walks up from its own location, and an explicit root is a FLAG
(`--nano-ros-path`, `-DNROS_REPO_DIR`). So the variable has no Rust reader left,
and this gate keeps it that way.

History: issue 1641 made this gate require a `repo-dir-env-ok:` reason beside
every read, because a linked git worktree inherits `$NROS_REPO_DIR` from the
shell that spawned it and there it names the PARENT checkout. Read raw, the
variable made a worktree act on the parent: issue 1280 (build scripts compiled
the parent's sources), 1510 (the CLI's SDK root), 1538 (a gate certified the
parent's headers), and 1641 itself — seven more CLI-side readers, one of which
WROTE a file into the parent checkout. Every reason those sites gave was "the
caller's own checkout comes first"; the amendment finished the thought by
removing the rung below it, so a read is now refused outright rather than
excused.

`just` recipes still export the variable to the SCRIPTS they run
(`just/sdk-env.just`); that is shell plumbing inside one checkout, not an input
any Rust code may take.

CMake is held to the same rule: no tracked CMakeLists / `*.cmake` reads
`$ENV{NROS_REPO_DIR}` or `$ENV{nano_ros_ROOT}`. A module locates the root from
its own path; a leaf that must know the root before it can include anything
carries the ONE leaf block (explicit `-DNROS_REPO_DIR`/`-Dnano_ros_ROOT`, else
`nros sdk-root`), and every copy of that block must be byte-identical — the
copies are compared with each other, so the spelling lives in the leaves, not
restated here.

Scope (Rust): tracked `*.rs` outside test code (a `tests/` directory, or a `*_tests.rs`
file). A test that reads the variable to decide whether to skip is not acting on
a checkout. `#[cfg(test)]` modules inside a source file are NOT exempt — a
reader cannot tell a test module from a cfg'd production path by grep.
"""

from __future__ import annotations

import re
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent

READ_RE = re.compile(r'\benv::var(?:_os)?\s*\(\s*"NROS_REPO_DIR"\s*\)|\benv!\s*\(\s*"NROS_REPO_DIR"')
CMAKE_READ_RE = re.compile(r"ENV\{(?:NROS_REPO_DIR|nano_ros_ROOT)\}")
CMAKE_GLOBS = ["*CMakeLists.txt", "*.cmake", "*.cmake.in"]
# The first line of the shared leaf block; the block ends at its first
# column-0 `endif()`.
BLOCK_HEAD = "# Resolve the nano-ros SDK root — RFC-0103 D6 (amended 2026-10-12). This"


def is_test_path(rel: str) -> bool:
    return "/tests/" in f"/{rel}" or rel.endswith("_tests.rs")


def findings(rel: str, text: str) -> list[str]:
    """One message per read in one file."""
    out = []
    for i, line in enumerate(text.splitlines()):
        if not READ_RE.search(line):
            continue
        stripped = line.lstrip()
        if stripped.startswith("//"):
            continue  # prose naming the variable is not a read
        out.append(
            f"{rel}:{i + 1}: reads $NROS_REPO_DIR.\n"
            f"      {stripped.strip()}\n"
            f"      RFC-0103 D6 (amended 2026-10-12): nothing exports it to a user's "
            f"shell, and in a linked worktree an inherited value names the PARENT "
            f"checkout (issues 1280/1510/1538/1641). Ask the ladder "
            f"(`nano_ros_root::resolve`), the binary's own root "
            f"(`nros_launcher::checkout::own_sdk_root`), or walk up from your own "
            f"location (`nros_build_paths::checkout_root_of`)."
        )
    return out


def cmake_findings(rel: str, text: str) -> list[str]:
    """One message per non-comment `$ENV{NROS_REPO_DIR}` / `$ENV{nano_ros_ROOT}` read."""
    out = []
    for i, line in enumerate(text.splitlines()):
        code = line.split("#", 1)[0]
        if CMAKE_READ_RE.search(code):
            out.append(
                f"{rel}:{i + 1}: reads the root from the environment.\n"
                f"      {line.strip()}\n"
                f"      RFC-0103 D6 (amended 2026-10-12): a module locates the root from "
                f"its own path (CMAKE_CURRENT_LIST_DIR); a leaf carries the shared "
                f"block (explicit -DNROS_REPO_DIR / -Dnano_ros_ROOT, else `nros sdk-root`)."
            )
    return out


def leaf_block(text: str) -> str | None:
    """The shared leaf block in `text`, or None when the file carries none."""
    lines = text.splitlines()
    for i, line in enumerate(lines):
        if line == BLOCK_HEAD:
            for j in range(i, len(lines)):
                if lines[j] == "endif()":
                    return "\n".join(lines[i : j + 1])
            return "\n".join(lines[i:])  # unterminated: reported as a distinct copy
    return None


def self_test() -> None:
    """Negative controls on the normal path — a rule that never fires proves nothing."""
    bad = 'fn f() {\n    let r = std::env::var_os("NROS_REPO_DIR");\n}\n'
    assert len(findings("a.rs", bad)) == 1, "a read must fire"
    assert len(findings("a.rs", 'let r = std::env::var("NROS_REPO_DIR").ok();')) == 1
    assert len(findings("a.rs", 'const R: &str = env!("NROS_REPO_DIR");')) == 1
    # The old per-site excuse no longer excuses anything.
    marked = (
        "    // repo-dir-env-ok: handed to nano_ros_root::resolve below.\n"
        '    let r = std::env::var_os("NROS_REPO_DIR");\n'
    )
    assert len(findings("a.rs", marked)) == 1, "a reason must not excuse a read"
    # Prose that names the variable is not a read.
    assert findings("a.rs", '/// `std::env::var_os("NROS_REPO_DIR")` used to win.') == []
    assert is_test_path("packages/x/tests/foo.rs") and not is_test_path("packages/x/src/foo.rs")
    # CMake: a read fires, a comment naming it does not.
    assert len(cmake_findings("CMakeLists.txt", 'include("$ENV{NROS_REPO_DIR}/x.cmake")')) == 1
    assert len(cmake_findings("a.cmake", "if(DEFINED ENV{NROS_REPO_DIR})")) == 1
    assert len(cmake_findings("a.cmake", 'set(R "$ENV{nano_ros_ROOT}")')) == 1
    assert cmake_findings("a.cmake", "# `$ENV{NROS_REPO_DIR}` used to win.") == []
    block = BLOCK_HEAD + "\nif(NOT NROS_REPO_DIR)\nendif()"
    assert leaf_block("x\n" + block + "\nafter") == block
    assert leaf_block("no block here") is None


def main() -> int:
    self_test()
    if "--self-test" in sys.argv:
        print("check-repo-dir-readers self-test: OK")
        return 0
    r = subprocess.run(
        ["git", "-C", str(ROOT), "ls-files", "--", "*.rs"],
        capture_output=True, text=True, check=False,
    )
    if r.returncode != 0:
        sys.exit(f"check-repo-dir-readers: `git ls-files` failed:\n  {r.stderr.strip()}")
    errors, scanned = [], 0
    for rel in (x.strip() for x in r.stdout.splitlines()):
        if not rel or is_test_path(rel):
            continue
        path = ROOT / rel
        if not path.is_file():
            continue
        scanned += 1
        text = path.read_text(errors="replace")
        if "NROS_REPO_DIR" not in text:
            continue
        errors += findings(rel, text)
    if scanned == 0:
        sys.exit("check-repo-dir-readers: scanned NO Rust files — an empty scan passes vacuously.")
    r = subprocess.run(
        ["git", "-C", str(ROOT), "ls-files", "--", *CMAKE_GLOBS],
        capture_output=True, text=True, check=False,
    )
    if r.returncode != 0:
        sys.exit(f"check-repo-dir-readers: `git ls-files` failed:\n  {r.stderr.strip()}")
    cmake_scanned, blocks = 0, {}
    for rel in (x.strip() for x in r.stdout.splitlines()):
        path = ROOT / rel
        if not rel or not path.is_file():
            continue
        cmake_scanned += 1
        text = path.read_text(errors="replace")
        errors += cmake_findings(rel, text)
        b = leaf_block(text)
        if b is not None:
            blocks.setdefault(b, []).append(rel)
    if cmake_scanned == 0:
        sys.exit("check-repo-dir-readers: scanned NO CMake files — an empty scan passes vacuously.")
    if len(blocks) > 1:
        variants = sorted(blocks.values(), key=len, reverse=True)
        errors.append(
            "the shared leaf SDK-root block has diverged into "
            f"{len(blocks)} spellings; every copy must be byte-identical:\n"
            + "\n".join(f"      {len(v)} file(s): {', '.join(v)}" for v in variants)
        )
    if errors:
        print("check-repo-dir-readers: FAIL", file=sys.stderr)
        for e in errors:
            print(f"  - {e}", file=sys.stderr)
        return 1
    copies = sum(len(v) for v in blocks.values())
    print(
        f"check-repo-dir-readers: OK — {scanned} Rust and {cmake_scanned} CMake file(s), "
        f"none reads the root from the environment; {copies} identical leaf block(s)."
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())

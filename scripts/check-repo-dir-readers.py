#!/usr/bin/env python3
"""issue 1641 — every Rust read of `$NROS_REPO_DIR` must say where the 1280 rule is applied.

A linked git worktree inherits `$NROS_REPO_DIR` from the shell that spawned it,
and there it names the PARENT checkout. Read raw and used as given, the variable
makes a worktree act on the parent: issue 1280 (build scripts compiled the
parent's sources), issue 1510 (the CLI's SDK root), issue 1538 (a gate certified
the parent's headers), and issue 1641 — seven more CLI-side readers, one of which
WROTE a file into the parent checkout and one of which emitted path deps that
compiled the parent's core crates.

That is the same rule fixed three times where the symptom was seen, which is
issue 0196's shape. This gate is the class fix: a read of the variable is only
allowed where it hands the value to something that applies the rule —
`nros_build_paths::reroot_foreign`, `nano_ros_root::resolve`, or a site-specific
ladder that puts the caller's own checkout first — and the site has to SAY which,
in a `repo-dir-env-ok: <reason>` comment within three lines above the read.

A reason at the site rather than an allowlist here, because the reason belongs
next to the code it excuses: an allowlist is only as complete as whoever last
edited it, and it does not move when the code does.

Scope: tracked `*.rs` outside test code (a `tests/` directory, or a `*_tests.rs`
file). A test that reads the variable to decide whether to skip is not acting on
a checkout. `#[cfg(test)]` modules inside a source file are NOT exempt — a marker
there costs one line, and a reader cannot tell a test module from a cfg'd
production path by grep.
"""

from __future__ import annotations

import re
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent

READ_RE = re.compile(r'\benv::var(?:_os)?\s*\(\s*"NROS_REPO_DIR"\s*\)|\benv!\s*\(\s*"NROS_REPO_DIR"')
MARKER_RE = re.compile(r"repo-dir-env-ok:\s*\S")
WINDOW = 3


def is_test_path(rel: str) -> bool:
    return "/tests/" in f"/{rel}" or rel.endswith("_tests.rs")


def findings(rel: str, text: str) -> list[str]:
    """One message per unmarked read in one file."""
    out = []
    lines = text.splitlines()
    for i, line in enumerate(lines):
        if not READ_RE.search(line):
            continue
        stripped = line.lstrip()
        if stripped.startswith("//"):
            continue  # prose naming the variable is not a read
        window = lines[max(0, i - WINDOW) : i + 1]
        if any(MARKER_RE.search(w) for w in window):
            continue
        out.append(
            f"{rel}:{i + 1}: reads $NROS_REPO_DIR with no `repo-dir-env-ok:` reason "
            f"in the {WINDOW} lines above.\n"
            f"      {stripped.strip()}\n"
            f"      In a linked worktree this variable names the PARENT checkout "
            f"(issues 1280/1510/1538/1641). Hand it to "
            f"`nros_build_paths::reroot_foreign` or `nano_ros_root::resolve`, or "
            f"to a ladder that puts the caller's own checkout first, and say "
            f"which in a `// repo-dir-env-ok: <reason>` comment."
        )
    return out


def self_test() -> None:
    """Negative controls on the normal path — a rule that never fires proves nothing."""
    bad = 'fn f() {\n    let r = std::env::var_os("NROS_REPO_DIR");\n}\n'
    assert len(findings("a.rs", bad)) == 1, "an unmarked read must fire"
    assert len(findings("a.rs", 'let r = std::env::var("NROS_REPO_DIR").ok();')) == 1
    good = (
        "    // repo-dir-env-ok: handed to nano_ros_root::resolve below.\n"
        '    let r = std::env::var_os("NROS_REPO_DIR");\n'
    )
    assert findings("a.rs", good) == []
    # A marker too far above does not count — it must be next to the read.
    far = "// repo-dir-env-ok: x\n" + "\n" * WINDOW + 'let r = std::env::var_os("NROS_REPO_DIR");\n'
    assert len(findings("a.rs", far)) == 1, "a distant marker must not excuse a read"
    # An empty reason is not a reason.
    assert len(findings("a.rs", '// repo-dir-env-ok:\nlet r = std::env::var_os("NROS_REPO_DIR");')) == 1
    # Prose that names the variable is not a read.
    assert findings("a.rs", '/// `std::env::var_os("NROS_REPO_DIR")` used to win.') == []
    assert is_test_path("packages/x/tests/foo.rs") and not is_test_path("packages/x/src/foo.rs")


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
    errors, reads = [], 0
    for rel in (x.strip() for x in r.stdout.splitlines()):
        if not rel or is_test_path(rel):
            continue
        path = ROOT / rel
        if not path.is_file():
            continue
        text = path.read_text(errors="replace")
        if "NROS_REPO_DIR" not in text:
            continue
        reads += sum(
            1 for ln in text.splitlines() if READ_RE.search(ln) and not ln.lstrip().startswith("//")
        )
        errors += findings(rel, text)
    if errors:
        print("check-repo-dir-readers: FAIL", file=sys.stderr)
        for e in errors:
            print(f"  - {e}", file=sys.stderr)
        return 1
    if reads == 0:
        sys.exit(
            "check-repo-dir-readers: found NO reads at all — the reader has drifted "
            "from the spelling the code uses, and an empty scan passes vacuously."
        )
    print(f"check-repo-dir-readers: OK — {reads} read(s) of $NROS_REPO_DIR, each naming where the 1280 rule is applied.")
    return 0


if __name__ == "__main__":
    sys.exit(main())

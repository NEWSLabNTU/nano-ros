#!/usr/bin/env python3
"""phase-360 W4 — the in-repo source closure a build ACTUALLY read.

Prints NUL-separated repo-relative paths, deduplicated and sorted, gathered from
every cargo dep-info (`*.d`) under a row's build directory.

This is the measured half of a fixture signature. The other half — the row's own
source dir — is a complete answer for what it covers and blind to everything a
compile-check row exists to compile AGAINST. Issue 0466 records the cost: an
edit to `packages/boards/nros-board-common/src/platform_config.rs` left the gate
silent while the tests caught it on mtime, i.e. the build-side probe watching
LESS than the test-side gate (issue 0196's rule, violated toward museum
binaries).

Reading the compiler's own dependency output rather than guessing is what ninja
does with `.ninja_deps` and what ccache stores in its manifest.

Usage: dep-closure.py <repo_root> <build_dir>
"""

from __future__ import annotations

import sys
from pathlib import Path


def dep_targets(text: str) -> list[str]:
    """Yield the dependency tokens of a Make-style dep-info body.

    Cargo writes `<target>: <dep> <dep> …`, one logical rule per line, and
    escapes a space inside a path as `\\ `. Splitting on unescaped whitespace is
    therefore required — a naive `.split()` truncates any path with a space in
    it, which silently shrinks the closure (the failure mode this whole file
    exists to remove).
    """
    deps: list[str] = []
    for line in text.splitlines():
        line = line.rstrip("\\").strip()
        if ":" not in line:
            continue
        _, _, rhs = line.partition(":")
        cur = ""
        i = 0
        while i < len(rhs):
            ch = rhs[i]
            if ch == "\\" and i + 1 < len(rhs) and rhs[i + 1] == " ":
                cur += " "
                i += 2
                continue
            if ch.isspace():
                if cur:
                    deps.append(cur)
                    cur = ""
                i += 1
                continue
            cur += ch
            i += 1
        if cur:
            deps.append(cur)
    return deps


def main() -> int:
    if len(sys.argv) != 3:
        sys.stderr.write("usage: dep-closure.py <repo_root> <build_dir>\n")
        return 2
    repo_root = Path(sys.argv[1]).resolve()
    build_dir = Path(sys.argv[2]).resolve()
    if not build_dir.is_dir():
        return 0

    # The build root holds this build's OWN output. Hashing it would arm a
    # rebuild on what the build just produced — the same self-triggering loop
    # `compile-check-signature.sh` avoids by enumerating through the git index.
    try:
        build_rel = build_dir.relative_to(repo_root)
    except ValueError:
        build_rel = None

    found: set[str] = set()
    for dep_file in build_dir.rglob("*.d"):
        try:
            text = dep_file.read_text(errors="replace")
        except OSError as exc:
            # A dep-info we cannot read means an INCOMPLETE closure, and a
            # short closure hashes to a perfectly valid-looking signature.
            # Refuse rather than under-report.
            sys.stderr.write(f"dep-closure: cannot read {dep_file}: {exc}\n")
            return 1
        for tok in dep_targets(text):
            path = Path(tok)
            if not path.is_absolute():
                continue
            try:
                rel = path.resolve().relative_to(repo_root)
            except (ValueError, OSError):
                continue  # registry/sysroot/out-of-tree — not ours to watch
            if build_rel is not None and rel.is_relative_to(build_rel):
                continue
            if not path.is_file():
                continue  # listed but gone: a rebuild will settle it
            found.add(str(rel))

    out = sys.stdout.buffer
    for rel in sorted(found):
        out.write(rel.encode() + b"\0")
    return 0


if __name__ == "__main__":
    sys.exit(main())

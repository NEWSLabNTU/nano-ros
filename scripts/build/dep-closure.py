#!/usr/bin/env python3
"""phase-363 W4 — the in-repo source closure a build ACTUALLY read.

Prints NUL-separated repo-relative paths, deduplicated and sorted, gathered from
whichever of these a row's build directory happens to hold:

  * cargo / compiler dep-info    `*.d`, Make syntax          (cargo + cxx rows)
  * CMake's configure inputs     `CMakeFiles/Makefile.cmake` (Makefile generator)
  * Ninja's re-configure edge    `build.ninja` RERUN_CMAKE   (Ninja generator)

Each is the same idea from a different tool: the thing that read the files wrote
down what it read. A row with none of them yields an empty closure and keeps
whatever its `sig_paths` already cover.

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

import re
import subprocess
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
    # Unfold backslash continuations FIRST. Cargo writes one rule per physical
    # line, but a compiler's `-MD` output wraps, and a per-line parser then sees
    # continuation lines with no colon and skips them — silently returning only
    # the first dependency. That produced an EMPTY closure for the
    # `-fsyntax-only` rows on the first attempt here.
    text = text.replace("\\\n", " ")

    deps: list[str] = []
    for line in text.splitlines():
        line = line.strip()
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

    def keep(tok: str, base: Path | None = None) -> None:
        path = Path(tok)
        if not path.is_absolute():
            # A depfile may record paths relative to the COMPILER's cwd, which
            # for these builders is the repo root, or relative to the file that
            # records them. Try both and accept only a base that actually
            # resolves — guessing one would silently drop the whole list, which
            # is how the `-fsyntax-only` rows produced an empty closure on the
            # first attempt at this.
            for cand in (repo_root / tok, (base / tok) if base else None):
                if cand is not None and cand.is_file():
                    path = cand
                    break
            else:
                return
        try:
            rel = path.resolve().relative_to(repo_root)
        except (ValueError, OSError):
            return  # registry / sysroot / another checkout — not ours to watch
        if build_rel is not None and rel.is_relative_to(build_rel):
            return
        if not path.is_file():
            return  # listed but gone: a rebuild will settle it
        found.add(str(rel))

    # 1) CMake's own record of every file the CONFIGURE read — the cmake-configure
    #    rows compile nothing, so this IS their dependency set. Without it an edit
    #    to `cmake/NanoRosCodegenCore.cmake` moved no signature at all, though 18
    #    in-repo modules are listed here for a single row.
    for mk in build_dir.rglob("CMakeFiles/Makefile.cmake"):
        try:
            text = mk.read_text(errors="replace")
        except OSError as exc:
            sys.stderr.write(f"dep-closure: cannot read {mk}: {exc}\n")
            return 1
        m = re.search(r"set\(CMAKE_MAKEFILE_DEPENDS\s*(.*?)\)", text, re.S)
        if m:
            for tok in re.findall(r'"([^"]+)"', m.group(1)):
                keep(tok)

    # 2) The Ninja generator records the same thing on its re-configure edge.
    #    `$` continues a line. A stale `build.ninja` copied from another checkout
    #    lists that checkout's paths; `keep()` drops anything outside this root.
    for nj in build_dir.rglob("build.ninja"):
        try:
            text = nj.read_text(errors="replace")
        except OSError as exc:
            sys.stderr.write(f"dep-closure: cannot read {nj}: {exc}\n")
            return 1
        text = text.replace("$\n", " ")
        for line in text.splitlines():
            if not line.startswith("build build.ninja:"):
                continue
            _, _, rhs = line.partition("|")
            for tok in rhs.split():
                keep(tok)

    # 3) Compiler / cargo dep-info.
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
            keep(tok, dep_file.parent)

    # Drop anything git ignores. A depfile lists BUILD OUTPUT as readily as
    # source — `target/nros-c-generated/nros/nros_config_generated.h` is emitted
    # by another crate's build.rs and rewritten on every run — and hashing that
    # arms a rebuild on what the build just produced. Excluding only the row's
    # own build dir was not enough: the tree has other output roots, and the
    # result was three cxx-syntax rows reporting STALE immediately after a
    # successful build, forever.
    #
    # "Is this build output?" already has an answer in this repo, and it is the
    # same one `nros_source_manifest` uses: git's ignore rules. One policy for
    # both halves of a signature rather than two.
    ordered = sorted(found)

    # `git check-ignore` REFUSES a path inside a submodule ("Pathspec … is in
    # submodule", exit 128) rather than answering for it. Submodule content is
    # tracked source in a nested repo — zenoh-pico's headers are a genuine input
    # to these rows — so those paths are kept and simply not asked about.
    submodules: list[str] = []
    gm = repo_root / ".gitmodules"
    if gm.is_file():
        submodules = re.findall(r"^\s*path\s*=\s*(.+?)\s*$", gm.read_text(errors="replace"), re.M)
    in_submodule = [p for p in ordered if any(p == m or p.startswith(m + "/") for m in submodules)]
    ordered = [p for p in ordered if p not in set(in_submodule)]

    if ordered:
        proc = subprocess.run(
            ["git", "-C", str(repo_root), "check-ignore", "--stdin", "-z"],
            input="\0".join(ordered) + "\0",
            capture_output=True,
            text=True,
        )
        # rc 0 = some ignored, 1 = none ignored, 128 = real error.
        if proc.returncode not in (0, 1):
            sys.stderr.write(f"dep-closure: git check-ignore failed: {proc.stderr}\n")
            return 1
        ignored = {p for p in proc.stdout.split("\0") if p}
        ordered = [p for p in ordered if p not in ignored]
    ordered = sorted(ordered + in_submodule)

    out = sys.stdout.buffer
    for rel in ordered:
        out.write(rel.encode() + b"\0")
    return 0


if __name__ == "__main__":
    sys.exit(main())

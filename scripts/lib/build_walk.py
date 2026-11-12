"""Walk a BUILD directory's output, through symlinked directories.

A build dir is not a plain tree. When cargo directories are shared
(`NROS_SHARED_CARGO_ROOT`, which every fixture build sets), `<build>/cargo` is a
SYMLINK into `build/corrosion-cargo/<platform>/<key>`. Everything cargo writes
for the image lives behind it: the `.su` frames a build script's C writes, and
the dep-info `.d` files. `Path.rglob` and a bare `os.walk` do not enter a
symlinked directory, so a reader that walks the build dir with either one
silently reads none of that output. The stack-floor walk refused over
frameless zpico functions that way (issue 2466).

Read build output with `build_files()`. It visits each REAL directory once, so a
link that points back up the tree cannot loop. This helper is for READING. A
tool that DELETES build output must never follow a link out of the tree it
owns, so it keeps its own walk (trim-caches, prune-superseded-artifacts,
gc-shared-cargo).

Standard library only: the stack-floor CHECK imports this on the user's own
Python (`check build-python-stdlib`).
"""

from __future__ import annotations

import os
from pathlib import Path


def build_files(root: Path, suffixes: tuple[str, ...] = (), names: tuple[str, ...] = ()) -> list[Path]:
    """Every file under `root` whose name ends with one of `suffixes` or equals
    one of `names`, reached through symlinked directories, each real
    directory walked once. Sorted, so the order is stable."""
    out: list[Path] = []
    seen: set[str] = set()
    for d, dirs, files in os.walk(root, followlinks=True):  # walk-ok: untracked build output in a build dir the caller names
        real = os.path.realpath(d)
        if real in seen:
            dirs[:] = []
            continue
        seen.add(real)
        out += [Path(d, f) for f in files
                if (suffixes and f.endswith(suffixes)) or f in names]
    return sorted(out)


def self_test() -> list[str]:
    """Problems, empty when the walk follows links and does not loop."""
    import tempfile

    bad = []
    with tempfile.TemporaryDirectory() as td:
        shared = Path(td, "shared", "deps")
        shared.mkdir(parents=True)
        (shared / "x.d").write_text("")
        build = Path(td, "build")
        (build / "sub").mkdir(parents=True)
        (build / "sub" / "build.ninja").write_text("")
        (build / "cargo").symlink_to(Path(td, "shared"), target_is_directory=True)
        (build / "loop").symlink_to(build, target_is_directory=True)
        got = [p.relative_to(build).as_posix() for p in build_files(build, (".d",), ("build.ninja",))]
        if got != ["cargo/deps/x.d", "sub/build.ninja"]:
            bad.append(f"build_files: {got}")
    return bad


if __name__ == "__main__":
    import sys

    problems = self_test()
    for p in problems:
        print(f"build_walk self-test: FAIL {p}", file=sys.stderr)
    print(f"build_walk self-test: {'FAIL' if problems else 'OK'}")
    sys.exit(1 if problems else 0)

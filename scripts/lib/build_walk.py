"""Walk a BUILD directory's output, through symlinked directories.

A build dir is not a plain tree. When cargo directories are shared
(`NROS_SHARED_CARGO_ROOT`, which every fixture build sets), `<build>/cargo` is a
SYMLINK into `build/corrosion-cargo/<platform>/<key>`. Everything cargo writes
for the image lives behind it: the `.su` frames a build script's C writes, and
the dep-info `.d` files. `Path.rglob` and a bare `os.walk` do not enter a
symlinked directory, so a reader that walks the build dir with either one
silently reads none of that output. The stack-floor walk refused over
frameless zpico functions that way (issue 2466).

Read build output with `build_files()`, and glob it with `glob_paths()`. Both
follow a symlinked directory, visit each REAL directory once, and never follow
a link whose target CONTAINS the walk's root: a link back up the tree (to the
checkout the build dir sits in, say) would otherwise walk the whole checkout,
or, for `glob.glob(recursive=True)`, which has no guard at all, loop until the
path-length limit (a stack-floor check hung for 7 h 42 min that way). This
helper is for READING. A
tool that DELETES build output must never follow a link out of the tree it
owns, so it keeps its own walk (trim-caches, prune-superseded-artifacts,
gc-shared-cargo).

Standard library only: the stack-floor CHECK imports this on the user's own
Python (`check build-python-stdlib`).
"""

from __future__ import annotations

import fnmatch
import os
from pathlib import Path


def _contains(outer: str, inner: str) -> bool:
    """Whether real path `outer` is `inner` or one of its ancestors."""
    return inner == outer or inner.startswith(outer.rstrip(os.sep) + os.sep)


def _walk(root: str):
    """`os.walk(root)` through symlinked directories, each real directory
    once, never into a link whose target contains `root` (a link back up the
    tree). Yields what `os.walk` yields."""
    root_real = os.path.realpath(root)
    seen: set[str] = set()
    for d, dirs, files in os.walk(root, followlinks=True):  # walk-ok: untracked build output in a build dir the caller names; links pruned below
        real = os.path.realpath(d)
        if real in seen:
            dirs[:] = []
            continue
        seen.add(real)
        dirs[:] = [n for n in dirs
                   if not (os.path.islink(os.path.join(d, n))
                           and _contains(os.path.realpath(os.path.join(d, n)), root_real))]
        yield d, dirs, files


def build_files(root: Path, suffixes: tuple[str, ...] = (), names: tuple[str, ...] = ()) -> list[Path]:
    """Every file under `root` whose name ends with one of `suffixes` or equals
    one of `names`, reached through symlinked directories, each real
    directory walked once, never back up the tree. Sorted, so the order is
    stable."""
    out: list[Path] = []
    for d, _dirs, files in _walk(str(root)):
        out += [Path(d, f) for f in files
                if (suffixes and f.endswith(suffixes)) or f in names]
    return sorted(out)


_MAGIC = "*?["


def _seg_match(segs: list[str], pats: list[str]) -> bool:
    """`glob` semantics for one relative path: `**` is zero or more segments,
    any other segment is an `fnmatch` pattern, and a wildcard never matches a
    segment starting with `.` unless the pattern does."""
    if not pats:
        return not segs
    p = pats[0]
    if p == "**":
        if _seg_match(segs, pats[1:]):
            return True
        return bool(segs) and not segs[0].startswith(".") and _seg_match(segs[1:], pats)
    if not segs:
        return False
    s0 = segs[0]
    if s0.startswith(".") and not p.startswith("."):
        return False
    return fnmatch.fnmatchcase(s0, p) and _seg_match(segs[1:], pats[1:])


def glob_paths(pattern: str) -> list[str]:
    """`glob.glob(pattern, recursive=True)`, with `_walk`'s rule for links: a
    symlinked directory is followed, each real directory once, never back up
    the tree. Sorted. A pattern with no `**` is `glob.glob`'s, which does not
    recurse and so cannot loop."""
    import glob

    parts = Path(pattern).parts
    if "**" not in parts:
        return sorted(glob.glob(pattern))  # walk-ok: no `**`, so no recursion
    i = next(k for k, seg in enumerate(parts) if any(c in seg for c in _MAGIC))
    base = os.path.join(*parts[:i]) if i else "."
    pats = list(parts[i:])
    if not os.path.isdir(base):
        return []
    out: list[str] = []
    for d, _dirs, files in _walk(base):
        rel = os.path.relpath(d, base)
        segs = [] if rel == "." else rel.split(os.sep)
        if segs and _seg_match(segs, pats):
            out.append(d)
        for f in files:
            if _seg_match(segs + [f], pats):
                out.append(os.path.join(d, f))
    return sorted(set(out))


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

    # A link to an ANCESTOR of the root (the checkout a build dir sits in):
    # never followed, by either walk. `glob.glob(recursive=True)` followed it
    # round the cycle until the path-length limit.
    with tempfile.TemporaryDirectory() as td:
        checkout = Path(td, "checkout")
        build = checkout / "tmp" / "proj" / "build"
        crate = build / "nano_ros" / "image"
        (crate / "nano_ros_x_ffi_1" / "target" / "deps").mkdir(parents=True)
        (crate / "nano_ros_x_ffi_1" / "target" / "deps" / "a.bc").write_text("")
        (crate / "_nros").symlink_to(checkout, target_is_directory=True)
        shared = Path(td, "shared")
        (shared / "nano_ros_y_ffi_2" / "target" / "deps").mkdir(parents=True)
        (build / "cargo").symlink_to(shared, target_is_directory=True)
        got = [os.path.relpath(p, build)
               for p in glob_paths(str(build / "**" / "nano_ros_*_ffi_*" / "target"))]
        if got != ["cargo/nano_ros_y_ffi_2/target", "nano_ros/image/nano_ros_x_ffi_1/target"]:
            bad.append(f"glob_paths through an ancestor link: {got}")
        # Each trip round the loop is another copy of every file below it:
        # `glob.glob` returned this one file once per trip (about 40, the
        # kernel's symlink-hop limit), each trip re-walking everything the
        # link reaches, which for a link to the checkout is the checkout.
        bcs = glob_paths(str(build / "**" / "*.bc"))
        if [os.path.relpath(p, build) for p in bcs] != [
                "nano_ros/image/nano_ros_x_ffi_1/target/deps/a.bc"]:
            bad.append(f"glob_paths went round the loop: {len(bcs)} copies")
        files = [p.relative_to(build).as_posix() for p in build_files(build, (".bc",))]
        if files != ["nano_ros/image/nano_ros_x_ffi_1/target/deps/a.bc"]:
            bad.append(f"build_files through an ancestor link: {files}")

    # With no link at all, `glob_paths` answers what `glob.glob` answers.
    import glob
    with tempfile.TemporaryDirectory() as td:
        for rel in ("a/b/x.txt", "a/x.txt", "x.txt", "a/.hid/x.txt", "c/d/e/y.txt"):
            Path(td, rel).parent.mkdir(parents=True, exist_ok=True)
            Path(td, rel).write_text("")
        for pat in ("**/x.txt", "a/**/x.txt", "**/d/**", "*/x.txt", "**/*.txt", "c/**/e"):
            want = sorted(glob.glob(os.path.join(td, pat), recursive=True))  # walk-ok: self-test temp tree
            want = sorted(w.rstrip(os.sep) for w in want if w.rstrip(os.sep) != td)
            have = glob_paths(os.path.join(td, pat))
            if have != want:
                bad.append(f"glob_paths({pat!r}) = {have} != glob {want}")
    return bad


if __name__ == "__main__":
    import sys

    problems = self_test()
    for p in problems:
        print(f"build_walk self-test: FAIL {p}", file=sys.stderr)
    print(f"build_walk self-test: {'FAIL' if problems else 'OK'}")
    sys.exit(1 if problems else 0)

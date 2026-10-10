#!/usr/bin/env python3
"""Issue 0268 — drift gate for the per-build sizes headers (the "stale mirror"
class), re-founded on the BUILD's own mirror commands by issue 1792.

`nros-c` / `nros-cpp` build.rs emit `nros_config_generated.h` and
`nros_cpp_config_generated.h` (executor/entity storage sizes); CMake mirrors
them onto every consumer's include path (`<crate>/include/nros/`). The mirror
MUST equal its source: the C `_opaque` buffers are sized from the mirror while
the Rust objects placement-constructed into them are sized by the crate that
wrote the source. A stale mirror is silent memory corruption — issue 0268
(freertos C, 336 bytes short), 0245 (zephyr C++, 32 bytes), and the
0088/0114/0122/0123 lineage before them.

## Why this gate was blind (issue 1792)

The shell version found its pairs by GUESSING both halves:

* WHERE a mirror is — globs under `nano_ros/packages/core/nros-*/include/nros/`.
  The crates moved to `packages/api/`, so the globs matched nothing, every run
  compared 0 pairs, and the run was recorded NOT VERIFIED on every machine,
  including the ones holding hundreds of built mirrors.
* WHAT it mirrors — the leaf copy beside the crate's binary dir. Issue 0978 made
  that copy the FALLBACK: the mirror script prefers the leaf-independent
  `cargo/*/<gen>/nros/<name>` (newest), because a leaf copy can outlive the run
  that wrote it. So even with the right globs the gate checked a rule the build
  no longer follows.

Both halves are now DERIVED from the subject. A CMake build dir states every
mirror it maintains as a `mirror-generated-header.sh <leaf> <build> <gen>
<name> <dest>` command in its `build.ninja`; this gate reads those, and asks
the same script `--resolve` which source that command copies. No path is
spelled here, and the source rule exists once.

Not covered, and said so: a Makefile-generator build dir (its commands live in
`CMakeFiles/*/build.make`) is counted and reported, not compared.

A pair whose SHARED source is gone (a deleted cargo store) resolves to the
leaf fallback. When that leaf copy is OLDER than the mirror, it cannot be what
the mirror was copied from, and a difference proves nothing about the build —
measured: 159 "drifted" pairs in a checkout whose `build/` caches had been
cleaned, every one a mirror newer than its leaf. Those are counted as "source
gone" and reported, never as drift. A leaf copy NEWER than its mirror is still
the 0268 shape and still fails.

`--fix` re-runs the mirror command for each drifted pair — exactly what the
build does, out of band.
"""
from __future__ import annotations

import os
import re
import subprocess
import sys
import tempfile
from pathlib import Path

REPO = Path(__file__).resolve().parent.parent
sys.path.insert(0, str(REPO / "scripts" / "lib"))
import check_skip  # noqa: E402
from repo_walk import prune  # noqa: E402

MIRROR = REPO / "scripts" / "build" / "mirror-generated-header.sh"
# Where build dirs live. A ROOT list, not a depth: the walk finds every
# `build.ninja` beneath, so a new layout cannot fall out of scope by depth.
ROOTS = ("examples", "packages", "build")
# Never holds a CMake build dir of its own, and is where the walk's cost is.
SKIP_DIRS = {"CMakeFiles", "cargo", ".git", "generated", "node_modules", ".cargo"}
CMD = re.compile(r"mirror-generated-header\.sh\s+(\S+)\s+(\S+)\s+(\S+)\s+(\S+)\s+(\S+)")


def build_dirs(root: Path):
    """(dir, generator) for every CMake build dir under `root`'s ROOTS."""
    for top in ROOTS:
        base = root / top
        if not base.is_dir():
            continue
        # walk-ok: finds UNTRACKED build.ninja files (build output), pruned at
        # nested repos and cargo/target/CMakeFiles; 0.6 s over 88 build dirs.
        for dirpath, dirnames, filenames in os.walk(base):
            prune(dirpath, dirnames, str(root))
            dirnames[:] = [d for d in dirnames
                           if d not in SKIP_DIRS and not d.startswith("target")]
            if "build.ninja" in filenames:
                yield Path(dirpath), "ninja"
            elif "CMakeCache.txt" in filenames and "Makefile" in filenames:
                yield Path(dirpath), "make"


def mirror_commands(build_ninja: Path):
    """The (leaf, build, gen, name, dest) of every mirror command in it."""
    try:
        text = build_ninja.read_text(encoding="utf-8", errors="replace")
    except OSError:
        return []
    return sorted(set(CMD.findall(text)))


def resolve(leaf: str, build: str, gen: str, name: str) -> str | None:
    r = subprocess.run(["bash", str(MIRROR), "--resolve", leaf, build, gen, name],
                       capture_output=True, text=True)
    return r.stdout.strip() if r.returncode == 0 and r.stdout.strip() else None


def same(a: str, b: str) -> bool:
    try:
        return Path(a).read_bytes() == Path(b).read_bytes()
    except OSError:
        return False


def scan(root: Path, fix: bool = False):
    """Returns (dirs, make_dirs, compared, drift[(dest, src)], fixed, gone)."""
    dirs = make_dirs = compared = fixed = gone = 0
    drift = []
    seen = set()
    for d, gen in build_dirs(root):
        if gen == "make":
            make_dirs += 1
            continue
        dirs += 1
        for leaf, build, gsub, name, dest in mirror_commands(d / "build.ninja"):
            if dest in seen or not Path(dest).is_file():
                continue  # not built yet: nothing to be stale
            seen.add(dest)
            src = resolve(leaf, build, gsub, name)
            if src is None:
                continue  # no source left — the build itself would fail loudly
            if same(src, dest):
                compared += 1
                continue
            if src == leaf and os.path.getmtime(src) < os.path.getmtime(dest):
                gone += 1  # the shared copy it was mirrored from no longer exists
                continue
            compared += 1
            if fix:
                subprocess.run(["bash", str(MIRROR), leaf, build, gsub, name, dest],
                               check=False)
                fixed += 1
            else:
                drift.append((dest, src))
    return dirs, make_dirs, compared, drift, fixed, gone


def _sizes(path: str) -> list[str]:
    try:
        lines = Path(path).read_text(errors="replace").splitlines()
    except OSError:
        return []
    return [ln for ln in lines if re.match(r"#define \S*(_SIZE|_OPAQUE_U64S) ", ln)]


def self_test() -> None:
    """Runs on the NORMAL path (`check-gate-selftests`): a synthetic checkout
    whose crate sits under `packages/api/` — where the shell gate looked for
    `packages/core/` and saw nothing — with a shared copy NEWER than a stale
    leaf copy (0978's tree)."""
    check_skip.self_test()
    with tempfile.TemporaryDirectory() as t:
        root = Path(t)
        b = root / "examples" / "plat" / "c" / "talker" / "build-zenoh"
        crate = b / "nano_ros" / "packages" / "api" / "nros-c"
        shared = b / "cargo" / "ws_1" / "nros-c-generated" / "nros"
        for p in (crate / "include" / "nros", shared):
            p.mkdir(parents=True)
        name = "nros_config_generated.h"
        (crate / name).write_text("#define X_SIZE 1\n")      # stale leaf copy
        (shared / name).write_text("#define X_SIZE 2\n")     # what the build mirrors
        dest = crate / "include" / "nros" / name
        (b / "build.ninja").write_text(
            f"  COMMAND = cd {b} && bash {MIRROR} {crate / name} {b} "
            f"nros-c-generated {name} {dest}\n")
        # a CMakeFiles subtree is pruned; its own build.ninja must not count twice
        (b / "CMakeFiles").mkdir()
        (b / "CMakeFiles" / "build.ninja").write_text("")

        dest.write_text("#define X_SIZE 2\n")
        dirs, _, compared, drift, _, _ = scan(root)
        assert (dirs, compared, drift) == (1, 1, []), \
            f"a mirror equal to the SHARED copy is current, got {(dirs, compared, drift)}"

        dest.write_text("#define X_SIZE 1\n")
        _, _, compared, drift, _, _ = scan(root)
        assert compared == 1 and len(drift) == 1, \
            "a mirror equal only to the stale LEAF copy is drift (0978's rule)"

        _, _, _, _, fixed, _ = scan(root, fix=True)
        assert fixed == 1 and dest.read_text() == "#define X_SIZE 2\n", "--fix re-mirrors"
        assert scan(root)[3] == [], "after --fix nothing drifts"

        # The shared store deleted: the leaf fallback is OLDER than the mirror,
        # so it is not the mirror's source — "source gone", not drift. A leaf
        # NEWER than its mirror is still drift.
        dest.write_text("#define X_SIZE 2\n")
        for f in shared.iterdir():
            f.unlink()
        os.utime(crate / name, (1, 1))
        r = scan(root)
        assert (r[2], r[3], r[5]) == (0, [], 1), f"source gone is not drift, got {r}"
        os.utime(crate / name, None)
        os.utime(dest, (1, 1))
        r = scan(root)
        assert len(r[3]) == 1 and r[5] == 0, f"a leaf newer than its mirror is drift, got {r}"

        dest.unlink()
        assert scan(root)[2] == 0, "an unbuilt mirror is not compared"


def main(argv: list[str]) -> int:
    fix = "--fix" in argv
    self_test()
    dirs, make_dirs, compared, drift, fixed, gone = scan(REPO, fix)
    if drift:
        for dest, src in drift:
            print(f"STALE MIRROR: {dest}\n         vs: {src}")
            a, b = set(_sizes(src)), set(_sizes(dest))
            for ln in sorted(a - b):
                print(f"             source: {ln}")
            for ln in sorted(b - a):
                print(f"             mirror: {ln}")
        print(f"""
{len(drift)} stale sizes-header mirror(s) — every consumer TU in those trees compiles
against the wrong storage sizes (silent `_opaque` overflow at runtime).

Fix: `python3 scripts/check-sizes-header-mirrors.py --fix` re-runs each drifted
mirror command (what the build does). Then find the missing edge:
  ninja -C <build-dir> -t query <...>/include/nros/nros_config_generated.h
— issues 0268, 1783.""")
        return 1
    if fix and fixed:
        print(f"re-mirrored {fixed} stale pair(s) through mirror-generated-header.sh")
    note = (f"; {make_dirs} Makefile-generator build dir(s) not compared"
            if make_dirs else "")
    if gone:
        note += (f"; {gone} mirror(s) whose shared source is gone (older leaf "
                 "copy only) not compared")
    if compared == 0:
        # issue 1739 — comparing nothing is NOT VERIFIED, through the ledger.
        return check_skip.unverified(
            "sizes-header-mirrors",
            f"0 built mirror(s) across {dirs} build dir(s){note} — no built "
            "nros-c/nros-cpp mirror here")
    print(f"sizes-header mirrors OK — {compared} mirror/source pair(s) across "
          f"{dirs} build dir(s){note}")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))

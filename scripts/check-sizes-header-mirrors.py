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
* WHAT it mirrors — the leaf copy beside the crate's binary dir, after issue
  0978 had made the mirror script prefer the `cargo/*/` side-channel copy. So
  even with the right globs the gate checked a rule the build no longer
  followed.

Both halves are now DERIVED from the subject. A CMake build dir states every
mirror it maintains as a `mirror-generated-header.sh <src> <build> - <name>
<dest>` command in its `build.ninja`; this gate reads those, and asks the same
script `--resolve` which source that command copies. No path is spelled here,
and the source rule exists once. Since issue 1824 that source is always the
PLACED copy (`-` as the gen-subdir) — the one `cargo-out-dir-headers.py` put
from cargo's own `$OUT_DIR` — so the old choice between a leaf copy and a
shared one, and this gate's "shared source gone" exemption for it, are gone.
A command with any other gen-subdir is from a configure older than 1824: the
script refuses it, so it is counted as UNPLACED and reported (re-configure),
never compared.

A Makefile-generator build dir states the SAME command as a recipe line in
`CMakeFiles/<target>.dir/build.make` (one per target, nested subproject dirs
included) — issue 1832: CI's lanes configure with Unix Makefiles, and while
this gate read `build.ninja` only, it compared 0 pairs in every CI run
("15 Makefile-generator build dir(s) not compared"). One derivation per
generator (`mirror_commands`) feeds the one comparison below.

A pair that differs because its source was written AFTER its build dir last
ran the mirror command is not drift (issue 1911): the cargo run places a new
header before the mirror command re-runs (issue 1783), and a checkout that is
not rebuilt between two gates holds exactly that state — the self-hosted lanes
build in a persistent tree that runs `check fast` before `just build`
refreshes anything. The mirror command records each run
(`mirror-generated-header.sh --marker`); a pair counts as drift only when that
run is newer than the source, i.e. the build ran its copy and the copy still
differs, which is the 0268 shape. A pair with no newer run is counted as
"pending" and reported, never compared. (Issue 1901's archive bridge for
marker-less dirs read the `.a` beside the old leaf-copy argument; a placed
source has no archive beside it, and every `-` command's script writes a
marker, so the bridge went with 1824.)

`--fix` re-runs the mirror command for each DRIFTED pair -- exactly what the
build does, out of band. It never touches a pending pair (issue 1901): that
dir has not built since the source moved, so its archive predates the new
header, and copying the header in would make its consumers compile against
sizes the archive was not built with, i.e. CREATE the 0268 state. It names the dir instead; rebuilding it
re-mirrors before any consumer compiles (issue 1783).
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


def _read(path: Path) -> str:
    try:
        return path.read_text(encoding="utf-8", errors="replace")
    except OSError:
        return ""


def _build_make_files(build_dir: Path):
    """Every `CMakeFiles/<target>.dir/build.make` of a Makefile build dir,
    including nested subproject dirs (`nano_ros/packages/api/nros-c/...`).
    A nested dir holding its OWN `CMakeCache.txt` is a separate build dir the
    outer walk yields itself, so it is not entered here."""
    # walk-ok: untracked build output under ONE build dir, pruned like the
    # outer walk (cargo/target/generated never hold a CMake target dir).
    for dirpath, dirnames, filenames in os.walk(build_dir):
        here = Path(dirpath)
        dirnames[:] = [
            d for d in dirnames
            if d not in SKIP_DIRS - {"CMakeFiles"} and not d.startswith("target")
            and not (here / d / "CMakeCache.txt").is_file()]
        if "build.make" in filenames and here.parent.name == "CMakeFiles":
            yield here / "build.make"


def mirror_commands(build_dir: Path, generator: str):
    """The (leaf, build, gen, name, dest) of every mirror command the build
    dir states — from `build.ninja` (ninja) or every target's `build.make`
    (make). Both spell the command line identically, so one regex serves."""
    if generator == "ninja":
        texts = [_read(build_dir / "build.ninja")]
    elif generator == "make":
        texts = [_read(f) for f in _build_make_files(build_dir)]
    else:
        raise ValueError(f"unknown generator {generator!r}")
    return sorted({c for t in texts for c in CMD.findall(t)})


def resolve(leaf: str, build: str, gen: str, name: str) -> str | None:
    r = subprocess.run(["bash", str(MIRROR), "--resolve", leaf, build, gen, name],
                       capture_output=True, text=True)
    return r.stdout.strip() if r.returncode == 0 and r.stdout.strip() else None


def marker(build: str, dest: str) -> Path:
    """Where the mirror command records that it ran for `dest` (issue 1911).
    Asked of the script, so the path is spelled once."""
    r = subprocess.run(["bash", str(MIRROR), "--marker", build, dest],
                       capture_output=True, text=True, check=True)
    return Path(r.stdout.strip())


def ran_after(build: str, dest: str, src: str) -> bool:
    """Did this build dir build `dest`'s mirror after `src` was last written?

    The mirror command's own run marker answers it (issue 1911). Without one,
    the dir has not provably built since the source moved: pending."""
    try:
        ts = os.path.getmtime(src)
    except OSError:
        return False
    m = marker(build, dest)
    return m.is_file() and os.path.getmtime(m) >= ts


def same(a: str, b: str) -> bool:
    try:
        return Path(a).read_bytes() == Path(b).read_bytes()
    except OSError:
        return False


def scan(root: Path, fix: bool = False):
    """Returns (dirs, make_dirs, compared, drift[(dest, src)], fixed, unplaced,
    pending[(dest, src, build)]). `dirs` counts every build dir scanned;
    `make_dirs` the Makefile ones; `unplaced` the built mirrors whose command
    predates issue 1824 (a gen-subdir other than `-`). `--fix` re-mirrors
    drift only."""
    dirs = make_dirs = compared = fixed = unplaced = 0
    drift = []
    pending = []
    seen = set()
    for d, gen in build_dirs(root):
        dirs += 1
        if gen == "make":
            make_dirs += 1
        for leaf, build, gsub, name, dest in mirror_commands(d, gen):
            if dest in seen or not Path(dest).is_file():
                continue  # not built yet: nothing to be stale
            seen.add(dest)
            if gsub != "-":
                # Issue 1824 -- a pre-placement command; the script refuses it
                # and the next configure rewrites it. Nothing to compare against.
                unplaced += 1
                continue
            src = resolve(leaf, build, gsub, name)
            if src is None:
                continue  # no source left — the build itself would fail loudly
            if same(src, dest):
                compared += 1
                continue
            if not ran_after(build, dest, src):
                # issue 1911: source moved since this dir last built. Never
                # re-mirrored by --fix either (issue 1901): the dir's archive
                # predates the source, so the copy would mismatch it.
                pending.append((dest, src, build))
                continue
            compared += 1
            if fix:
                subprocess.run(["bash", str(MIRROR), leaf, build, gsub, name, dest],
                               check=False)
                fixed += 1
            else:
                drift.append((dest, src))
    return dirs, make_dirs, compared, drift, fixed, unplaced, pending


def _sizes(path: str) -> list[str]:
    try:
        lines = Path(path).read_text(errors="replace").splitlines()
    except OSError:
        return []
    return [ln for ln in lines if re.match(r"#define \S*(_SIZE|_OPAQUE_U64S) ", ln)]


def _pending_cases(root: Path, b: Path, src: Path, dest: Path, tag: str) -> None:
    """Issues 1911 + 1901, shared by both generators. `dest` differs from
    `src`; on return the marker records a run newer than `src` (drift)."""
    stale = dest.read_text()
    t = os.path.getmtime(src)
    m = marker(str(b), str(dest))
    assert not m.exists(), f"{tag}: clean start"

    # 1911 -- no recorded run: the dir has not built since the source moved.
    # Pending, not drift.
    r = scan(root)
    assert (r[2], r[3], len(r[6])) == (0, [], 1), \
        f"{tag}: a source newer than the dir's last build is pending, got {r}"
    # 1901 -- --fix must not copy the new header in before the dir rebuilds.
    r = scan(root, fix=True)
    assert r[4] == 0 and dest.read_text() == stale and len(r[6]) == 1, \
        f"{tag}: --fix re-mirrors a PENDING pair (issue 1901), got {r[4]} fixed"

    # A recorded run older than the source is pending (1911's partial build).
    m.parent.mkdir(parents=True, exist_ok=True)
    m.write_text("")
    os.utime(m, (t - 100, t - 100))
    r = scan(root)
    assert (r[3], len(r[6])) == ([], 1), \
        f"{tag}: a recorded run older than the source is pending, got {r}"
    # The mirror command ran after the source was written and the copy
    # still differs: that is the 0268 shape.
    os.utime(m, (t + 100, t + 100))
    r = scan(root)
    assert r[2] == 1 and len(r[3]) == 1 and r[6] == [], \
        f"{tag}: a recorded run newer than a differing source is drift, got {r}"


def self_test() -> None:
    """Runs on the NORMAL path (`check-gate-selftests`): a synthetic checkout
    whose crate sits under `packages/api/` — where the shell gate looked for
    `packages/core/` and saw nothing — mirroring a PLACED source (issue 1824)
    beside a stale `cargo/*/` side-channel copy the gate must never consult."""
    check_skip.self_test()
    with tempfile.TemporaryDirectory() as t:
        root = Path(t)
        b = root / "examples" / "plat" / "c" / "talker" / "build-zenoh"
        crate = b / "nano_ros" / "packages" / "api" / "nros-c"
        placed = crate / "nros-placed" / "nros-c-generated" / "rclc" / "detail"
        stale_shared = b / "cargo" / "ws_1" / "nros-c-generated" / "rclc" / "detail"
        for p in (crate / "include" / "rclc" / "detail", placed, stale_shared):
            p.mkdir(parents=True)
        name = "nros_config_generated.h"
        (placed / name).write_text("#define X_SIZE 2\n")
        (stale_shared / name).write_text("#define X_SIZE 9\n")  # must be ignored
        dest = crate / "include" / "rclc" / "detail" / name
        (b / "build.ninja").write_text(
            f"  COMMAND = cd {b} && bash {MIRROR} {placed / name} {b} - {name} {dest}\n")
        # a CMakeFiles subtree is pruned; its own build.ninja must not count twice
        (b / "CMakeFiles").mkdir()
        (b / "CMakeFiles" / "build.ninja").write_text("")

        dest.write_text("#define X_SIZE 2\n")
        r = scan(root)
        assert (r[0], r[2], r[3]) == (1, 1, []), \
            f"a mirror equal to its placed source is current, got {r}"

        dest.write_text("#define X_SIZE 1\n")
        _pending_cases(root, b, placed / name, dest, "ninja")
        _, _, _, _, fixed, _, _ = scan(root, fix=True)
        assert fixed == 1 and dest.read_text() == "#define X_SIZE 2\n", \
            "--fix re-mirrors from the placed source, never the shared copy"
        assert scan(root)[3] == [], "after --fix nothing drifts"

        # Issue 1824 (D1's mutation-verified case) -- a placed mirror NEWER than
        # its source and different is drift. The old "shared copy gone"
        # exemption fired on `src == leaf`, always true for a placed source, and
        # hid exactly this.
        os.utime(placed / name, (1, 1))
        dest.write_text("#define X_SIZE 1\n")
        os.utime(marker(str(b), str(dest)), None)
        r = scan(root)
        assert [d for d, _ in r[3]] == [str(dest)] and r[5] == 0, \
            f"a placed mirror newer than its source that differs is drift, got {r}"

        # Issue 1824 -- a pre-placement command (a real gen-subdir) is UNPLACED:
        # counted, never compared, never re-run by --fix, even beside a
        # side-channel copy that differs from the mirror.
        (b / "build.ninja").write_text(
            f"  COMMAND = cd {b} && bash {MIRROR} {crate / name} {b} "
            f"nros-c-generated/rclc/detail {name} {dest}\n")
        r = scan(root, fix=True)
        assert (r[2], r[3], r[4], r[5]) == (0, [], 0, 1) \
            and dest.read_text() == "#define X_SIZE 1\n", \
            f"a pre-1824 mirror command is unplaced, not compared or fixed, got {r}"

        dest.unlink()
        assert scan(root)[2] == 0, "an unbuilt mirror is not compared"

    # issue 1832 — a Makefile-generator build dir (what CI's lanes configure):
    # the command sits in a NESTED subproject's `CMakeFiles/<t>.dir/build.make`
    # as a tab-indented recipe line, and must be compared, not just counted.
    with tempfile.TemporaryDirectory() as t:
        root = Path(t)
        b = root / "build" / "cmake-fixtures" / "cpp_leaf"
        crate = b / "nano_ros" / "packages" / "api" / "nros-cpp"
        tdir = crate / "CMakeFiles" / "nros_cpp_config_header.dir"
        placed = crate / "nros-placed" / "nros-cpp-generated" / "rclcpp" / "detail"
        for p in (crate / "include" / "rclcpp" / "detail", tdir, placed):
            p.mkdir(parents=True)
        (b / "CMakeCache.txt").write_text("CMAKE_GENERATOR:INTERNAL=Unix Makefiles\n")
        (b / "Makefile").write_text("all:\n")
        name = "nros_cpp_config_generated.h"
        (placed / name).write_text("#define Y_SIZE 2\n")
        dest = crate / "include" / "rclcpp" / "detail" / name
        (tdir / "build.make").write_text(
            f"{dest}: {crate}/libnros_cpp.a\n"
            f"\tcd {crate} && bash {MIRROR} {placed / name} {b} - {name} {dest}\n")

        empty = scan(root)
        assert empty[:2] == (1, 1) and empty[2] == 0, \
            f"an UNBUILT mirror in a Makefile dir compares nothing, got {empty}"

        dest.write_text("#define Y_SIZE 2\n")
        dirs, make_dirs, compared, drift, _, _, _ = scan(root)
        assert (dirs, make_dirs, compared, drift) == (1, 1, 1, []), \
            f"a Makefile dir's current mirror is COMPARED, got {(dirs, make_dirs, compared, drift)}"

        dest.write_text("#define Y_SIZE 1\n")
        _pending_cases(root, b, placed / name, dest, "make")
        _, _, compared, drift, _, _, _ = scan(root)
        assert compared == 1 and [d for d, _ in drift] == [str(dest)], \
            "a drifted mirror in a Makefile dir fails"

        _, _, _, _, fixed, _, _ = scan(root, fix=True)
        assert fixed == 1 and dest.read_text() == "#define Y_SIZE 2\n", \
            "--fix re-mirrors through the Makefile dir's own command"

        # build.make outside a `CMakeFiles/<t>.dir/` is not a target's rules,
        # and a nested dir with its own CMakeCache.txt is its OWN build dir.
        (crate / "build.make").write_text((tdir / "build.make").read_text())
        nested = b / "sub"
        (nested / "CMakeFiles" / "x.dir").mkdir(parents=True)
        (nested / "CMakeCache.txt").write_text("")
        (nested / "Makefile").write_text("")
        assert len(mirror_commands(b, "make")) == 1, "only target build.make files count"
        assert [d for d, _ in build_dirs(root)] == [b, nested], \
            "a nested CMake build dir is its own build dir"


def main(argv: list[str]) -> int:
    fix = "--fix" in argv
    self_test()
    dirs, make_dirs, compared, drift, fixed, unplaced, pending = scan(REPO, fix)
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
  ninja -C <build-dir> -t query <...>/include/rclc/detail/nros_config_generated.h
(Makefile dir: the rule for that path in CMakeFiles/<target>.dir/build.make)
— issues 0268, 1783.""")
        return 1
    if fix and fixed:
        print(f"re-mirrored {fixed} stale pair(s) through mirror-generated-header.sh")
    if fix and pending:
        # issue 1901 -- refuse, by name, rather than copy beside an old archive.
        for b in sorted({b for _, _, b in pending}):
            n = sum(1 for _, _, pb in pending if pb == b)
            print(f"NOT re-mirrored: {n} pending pair(s) in {b}\n"
                  "  its source moved after this dir last built, so its archive "
                  "predates the new header;\n"
                  "  copying the header in would make its consumers compile "
                  "against sizes the archive was\n"
                  f"  not built with. Rebuild that build dir instead (`cmake --build {b}`): "
                  "its build re-mirrors\n  before any consumer compiles (issues 1783, 1901).")
    note = (f" ({make_dirs} of them Makefile-generator)" if make_dirs else "")
    if unplaced:
        note += (f"; {unplaced} mirror(s) from a configure older than issue 1824 "
                 "(no placed source — re-configure that build dir) not compared")
    if pending:
        note += (f"; {len(pending)} mirror(s) whose source moved after their build dir "
                 "last built, pending that dir's next build (issues 1911, 1901), "
                 "not compared")
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

#!/usr/bin/env python3
"""Remove shared Corrosion cargo directories nothing points at. Issues 0805, 2222.

`nros_share_corrosion_cargo_dir()` names its directory by a HASH of the leaf's
resolved configuration, and replaces `<leaf-build>/cargo` with a symlink to it.
So when any key input changes — a profile, a capability, a normalisation fix —
the old directory stops being referenced and simply stays on disk. One such
change in this repo stranded 8 directories holding 8.6 GB.

That is the issue-0500 SDK-store class one directory over: a store that only
grows. This is its GC.

## Reachability, not age

A key directory is LIVE if some leaf still refers to it. No heuristic about
mtimes, no "probably unused".

There are TWO ways a leaf refers to one, and counting only the first is a bug
this tool shipped with:

* **A `cargo` symlink** — the Corrosion consumers. Corrosion computes its own
  `--target-dir` from `CMAKE_BINARY_DIR`, so the only way to redirect it is to
  replace that path with a symlink.
* **A path baked into the generated build files** — the NuttX FFI driver, which
  sets `CARGO_TARGET_DIR` itself and needs no symlink. Its shared directory
  appears in the leaf's `build.ninja` and nowhere else.

The symlink-only version reported the live NuttX directory as unreachable, so
`--prune` would have deleted 502 MB that twelve leaves were actively building
against — not corruption, but a full rebuild for everyone and a tool that lies
about what it is deleting.

## Folders inside a live store (issue 2222)

A store is shared by every build dir with the same key, but it is not one
cargo target dir. Corrosion's `--target-dir` is `<build>/cargo/<root>_<5 hex>`,
the hash taken over the WORKSPACE MANIFEST path, and since phase-506 W4a
(RFC-0071 D9) each build dir generates its own image crate manifest under its
own binary dir. So every build dir that reaches a store adds a folder of its
own there (about 1 GB for a C++ image), and when that build dir goes away the
folder stays, inside a store that is still live because its siblings link to
it. One store grew to 410 GB that way, and the store-level rule above could
never see it.

The folder rule is the same exact reachability one level down: a folder is
named iff the `build.ninja` of some build dir whose `cargo` link resolves to
that store passes `<build>/cargo/<folder>` (issue 1954 reads the same
`--target-dir` values for the staleness probe). A store is folder-collected
ONLY when every such build dir has a readable `build.ninja`, and never when a
generated build file names the store itself (the baked road above: its
contents are cargo's own layout, not Corrosion folders). Anything this cannot
decide is reported as undetermined and left alone. Only Corrosion-shaped
entries (`<name>_<5 hex>`) are candidates. Deleting a folder costs a rebuild,
never a dangling link: the store directory itself stays, so the issue-1990
failure (`File exists (os error 17)` under a live link) cannot arise here.

Reporting is the default. `--prune` deletes. `--self-test` runs the negative
controls against a synthetic tree.
"""

from __future__ import annotations

import argparse
import os
import re
import shutil
import subprocess
import sys
import tempfile
from dataclasses import dataclass, field
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent.parent
sys.path.insert(0, str(ROOT / "scripts" / "lib"))
from build_root import build_dir  # noqa: E402  issue 1921 — the one derivation

# Where build dirs that reach the store live. `tmp/` and `tests/` hold scratch
# build dirs (gate scratch, copied workspaces); a folder one of them names must
# not be reported as an orphan just because the walk did not look there.
WALK_BASES = ("examples", "packages", "tests", "tmp")

# Never descend: cargo output and VCS metadata hold no build dir of interest,
# and they are most of the file count.
PRUNE_DIRS = {".git", "target", "deps", "incremental", ".fingerprint"}

# Corrosion's folder name: `<workspace-folder>_<first 5 hex of SHA1(manifest)>`
# (Corrosion 0.6 `Corrosion.cmake`, `cargo_path_hash`).
CORROSION_FOLDER = re.compile(r"^.+_[0-9a-f]{5}$")


def shared_root() -> Path:
    # issue 1921 — the corrosion-cargo kind under THE build root, where every
    # producer writes (one dir per platform). This was a `<repo>/build`
    # literal, so under a relocated NROS_BUILD_ROOT it swept a tree no
    # producer used.
    return build_dir(ROOT, "corrosion-cargo")


@dataclass
class Scan:
    # stores referenced by a path baked into a generated build file
    baked: set[Path] = field(default_factory=set)
    # store -> build dirs whose `cargo` symlink resolves to it
    linked: dict[Path, list[Path]] = field(default_factory=dict)
    # store -> folder names some linking build dir's build.ninja passes
    named: dict[Path, set[str]] = field(default_factory=dict)
    # store -> linking build dirs whose build.ninja could not be read
    undetermined: dict[Path, list[Path]] = field(default_factory=dict)

    def live(self) -> set[Path]:
        return self.baked | set(self.linked)


def _read(p: Path) -> str | None:
    try:
        return p.read_text(errors="replace")
    except OSError:
        return None


def generated_build_text(build: Path) -> str | None:
    """The generated build files of a cmake build dir, where Corrosion's
    `--target-dir` values are spelled: `build.ninja` for Ninja, every
    `CMakeFiles/*.dir/build.make` for Unix Makefiles (the metadata probe's
    generator, so the store that leaked is exactly one a ninja-only reader
    could not decide). None when neither exists."""
    ninja = _read(build / "build.ninja")
    if ninja is not None:
        return ninja
    cache = _read(build / "CMakeCache.txt")
    if cache is None or "CMAKE_GENERATOR:INTERNAL=Unix Makefiles" not in cache:
        return None
    parts = []
    # walk-ok: generated build files inside one UNTRACKED build dir.
    for dirpath, dirnames, filenames in os.walk(build):
        dirnames[:] = [d for d in dirnames if d != "cargo" and d not in PRUNE_DIRS]
        if "build.make" in filenames:
            parts.append(_read(Path(dirpath) / "build.make") or "")
    return "\n".join(parts) if parts else None


def scan(store_root: Path, walk_roots: list[Path]) -> Scan:
    """Every reference to a store under `store_root`, by either mechanism, and
    the Corrosion folders each linking build dir names."""
    s = Scan()
    root_s = str(store_root)
    root_real = os.path.realpath(store_root)
    baked_re = re.compile(re.escape(root_s) + r"/([^/\s\"']+)/([0-9a-f]{12})")
    for start in walk_roots:
        if not start.is_dir():
            continue
        # walk-ok: hunts references inside UNTRACKED build dirs — the git index
        # cannot see build output, which is the whole point of this sweep.
        for dirpath, dirnames, filenames in os.walk(start):
            here = Path(dirpath)
            if os.path.realpath(here) == root_real:
                dirnames[:] = []
                continue
            # 1. the Corrosion consumers: a `cargo` symlink.
            if "cargo" in dirnames:
                p = here / "cargo"
                if p.is_symlink():
                    target = os.path.realpath(p)
                    if target.startswith(root_real + os.sep):
                        store = Path(root_s) / Path(target).relative_to(root_real)
                        s.linked.setdefault(store, []).append(here)
                        ninja = generated_build_text(here)
                        if ninja is None:
                            s.undetermined.setdefault(store, []).append(here)
                        else:
                            folders = s.named.setdefault(store, set())
                            for prefix in (str(p), target):
                                pat = re.escape(prefix + "/") + r"([^/\s\"'$:]+)"
                                folders.update(m.group(1) for m in re.finditer(pat, ninja))
                # a real `cargo` dir is cargo output: nothing to find inside
                dirnames.remove("cargo")
            # 2. the NuttX consumer: the path is baked into build.ninja. Read
            #    only the generated build files, never the whole tree.
            for name in ("build.ninja", "CMakeCache.txt"):
                if name not in filenames:
                    continue
                text = _read(here / name)
                if text is None or root_s not in text:
                    continue
                for m in baked_re.finditer(text):
                    s.baked.add(Path(root_s) / m.group(1) / m.group(2))
            dirnames[:] = [d for d in dirnames if d not in PRUNE_DIRS]
    return s


def du(path: Path) -> int:
    out = subprocess.run(["du", "-sb", str(path)], capture_output=True, text=True).stdout
    return int(out.split()[0]) if out.split() else 0


def origin(folder: Path) -> str:
    """The build dir a Corrosion folder was compiled for, read back from the
    image crate's dep-info (`<build>/nano_ros/.../<x>-image/src/lib.rs`), so a
    report names the PRODUCER of an orphan and not just its hash. Issue 2222:
    the 400 folders of the first sighting were deleted before anyone could ask
    which build dirs had made them."""
    for depth in ("*/*/lib*.d", "*/lib*.d"):
        for d in sorted(folder.glob(depth))[:4]:
            text = _read(d) or ""
            m = re.search(r"(/\S+?)/nano_ros/\S*?-image/src/lib\.rs", text)
            if m:
                return m.group(1)
    return "(no dep-info)"


def stores(store_root: Path):
    for platform_dir in sorted(store_root.iterdir()):
        if not platform_dir.is_dir():
            continue
        for entry in sorted(platform_dir.iterdir()):
            if entry.suffix == ".key" or not entry.is_dir() or entry.is_symlink():
                continue
            yield entry


def orphan_folders(s: Scan, store: Path) -> list[Path] | None:
    """Corrosion folders in a LIVE store that no linking build dir names, or
    None when the store's folders cannot be decided (baked, or a linking build
    dir without a readable build.ninja)."""
    if store in s.baked or store in s.undetermined:
        return None
    named = s.named.get(store, set())
    return [
        f
        for f in sorted(store.iterdir())
        if f.is_dir() and not f.is_symlink()
        and CORROSION_FOLDER.match(f.name) and f.name not in named
    ]


def sweep(store_root: Path, walk_roots: list[Path], prune: bool, rel_to: Path, out=print) -> dict:
    s = scan(store_root, walk_roots)
    live = s.live()
    unreachable = []
    folders = []
    undetermined = []
    for entry in stores(store_root):
        if entry not in live:
            key_file = entry.with_suffix(".key")
            key = key_file.read_text().strip() if key_file.exists() else "(no key file)"
            unreachable.append((entry, key_file, key, du(entry)))
            continue
        orphans = orphan_folders(s, entry)
        if orphans is None:
            if entry in s.undetermined:
                undetermined.append(entry)
            continue
        folders.extend((f, du(f)) for f in orphans)

    def rel(p: Path) -> str:
        try:
            return str(p.relative_to(rel_to))
        except ValueError:
            return str(p)

    verb = "removing" if prune else "(re-run with --prune to delete)"
    if unreachable:
        total = sum(x[3] for x in unreachable)
        out(f"gc-shared-cargo: {len(unreachable)} unreachable store(s) {verb}, {total / 1e9:.1f} GB")
        for entry, key_file, key, size in unreachable:
            out(f"  {size / 1e9:6.2f} GB  {rel(entry)}")
            out(f"            key: {key}")
            if prune:
                shutil.rmtree(entry, ignore_errors=True)
                key_file.unlink(missing_ok=True)
    if folders:
        total = sum(x[1] for x in folders)
        out(f"gc-shared-cargo: {len(folders)} orphan folder(s) in live stores {verb}, "
            f"{total / 1e9:.1f} GB (issue 2222: no live build.ninja names them)")
        for f, size in folders:
            out(f"  {size / 1e9:6.2f} GB  {rel(f)}")
            out(f"            built for: {origin(f)}")
            if prune:
                shutil.rmtree(f, ignore_errors=True)
    for entry in undetermined:
        out(f"gc-shared-cargo: NOTE {rel(entry)}: folders not checked, a build dir linking "
            f"to it has no readable build.ninja: "
            + ", ".join(rel(b) for b in s.undetermined[entry][:3]))
    if not unreachable and not folders:
        out(f"gc-shared-cargo: OK, every store under {rel(store_root)} is referenced by a "
            f"leaf and every Corrosion folder in it by a live build.ninja "
            f"({len(live)} live store(s)).")
    return {
        "stores": [x[0] for x in unreachable],
        "folders": [x[0] for x in folders],
        "undetermined": undetermined,
    }


# --------------------------------------------------------------------------
# self-test: negative controls on a synthetic tree (issue 2222)
# --------------------------------------------------------------------------
def _mkfolder(store: Path, name: str) -> Path:
    f = store / name
    (f / "release").mkdir(parents=True)
    (f / "release" / "libx.a").write_bytes(b"x" * 16)
    return f


def _mkbuild(where: Path, store: Path, folders: list[str] | None) -> Path:
    where.mkdir(parents=True)
    (where / "cargo").symlink_to(store)
    if folders is not None:
        lines = [f"  COMMAND = cargo rustc --target-dir {where}/cargo/{f} --release\n" for f in folders]
        (where / "build.ninja").write_text("rule CARGO\n" + "".join(lines))
    return where


def self_test() -> int:
    failures = []
    # repo rule: scratch lives in `$project/tmp/`, never the system temp dir
    (ROOT / "tmp").mkdir(exist_ok=True)
    with tempfile.TemporaryDirectory(prefix="gc-shared-cargo-selftest-", dir=ROOT / "tmp") as td:
        t = Path(td)
        root = t / "store"
        leaves = t / "leaves"
        live = root / "native" / ("a" * 12)
        only_link = root / "native" / ("b" * 12)
        baked = root / "nuttx" / ("c" * 12)
        dead = root / "native" / ("d" * 12)
        only_mk = root / "metadata-probe" / ("e" * 12)
        for d in (live, only_link, baked, dead, only_mk):
            d.mkdir(parents=True)
            d.with_suffix(".key").write_text("k\n")
        named = _mkfolder(live, "nros-cpp-image_aaaaa")
        orphan = _mkfolder(live, "nros-cpp-image_bbbbb")
        other = live / "not-a-corrosion-folder"
        other.mkdir()
        _mkbuild(leaves / "one" / "build-zenoh", live, ["nros-cpp-image_aaaaa"])
        _mkbuild(leaves / "two" / "build-zenoh", live, [])
        # a Unix Makefiles build dir (the metadata probe's generator) naming a
        # folder in its build.make, beside an orphan in the same store
        mk = _mkfolder(only_mk, "nros-cpp-image_fffff")
        mk_orphan = _mkfolder(only_mk, "nros-cpp-image_99999")
        mkb = _mkbuild(leaves / "five" / "build", only_mk, None)
        (mkb / "CMakeCache.txt").write_text("CMAKE_GENERATOR:INTERNAL=Unix Makefiles\n")
        sub = mkb / "nano_ros" / "CMakeFiles" / "_cargo-build_nros_cpp.dir"
        sub.mkdir(parents=True)
        (sub / "build.make").write_text(
            f"\tcargo rustc --target-dir {mkb}/cargo/nros-cpp-image_fffff --release\n")
        # a store reached only through a link whose build dir has no build.ninja
        guarded = _mkfolder(only_link, "nros-c-image_ccccc")
        _mkbuild(leaves / "three" / "build-zenoh", only_link, None)
        # a store whose path is baked into a generated build file
        baked_folder = _mkfolder(baked, "nros-c-image_ddddd")
        (leaves / "four").mkdir()
        (leaves / "four" / "build.ninja").write_text(f"  CARGO_TARGET_DIR={baked}\n")
        _mkfolder(dead, "nros-c-image_eeeee")

        lines: list[str] = []
        r = sweep(root, [leaves], prune=False, rel_to=t, out=lines.append)
        if r["folders"] != [mk_orphan, orphan]:
            failures.append(f"report: orphan folders {r['folders']} != [{mk_orphan}, {orphan}]")
        if r["stores"] != [dead]:
            failures.append(f"report: unreachable stores {r['stores']} != [{dead}]")
        if r["undetermined"] != [only_link]:
            failures.append(f"report: undetermined {r['undetermined']} != [{only_link}]")
        if not orphan.exists():
            failures.append("report mode deleted something")

        sweep(root, [leaves], prune=True, rel_to=t, out=lines.append)
        for p, want in ((orphan, False), (dead, False), (named, True), (guarded, True),
                        (baked_folder, True), (other, True), (only_link, True), (live, True),
                        (mk, True), (mk_orphan, False)):
            if p.exists() != want:
                failures.append(f"prune: {p.relative_to(t)} exists={p.exists()}, want {want}")
        r = sweep(root, [leaves], prune=False, rel_to=t, out=lines.append)
        if r["folders"] or r["stores"]:
            failures.append(f"second pass not clean: {r}")
    if failures:
        print("gc-shared-cargo self-test: FAIL")
        for f in failures:
            print(f"  {f}")
        return 1
    print("gc-shared-cargo self-test: OK (an orphan folder in a live store is reported and "
          "pruned; a folder a live build.ninja or Makefiles build.make names, a store whose "
          "linking build dir has no generated build file, and a baked store are left alone)")
    return 0


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--prune", action="store_true",
                    help="delete unreachable stores and orphan folders")
    ap.add_argument("--self-test", action="store_true", help="run the negative controls")
    args = ap.parse_args()
    if args.self_test:
        return self_test()

    root = shared_root()
    if not root.is_dir():
        print("gc-shared-cargo: no shared store — nothing to do.")
        return 0
    sweep(root, [ROOT / b for b in WALK_BASES], args.prune, ROOT)
    return 0


if __name__ == "__main__":
    sys.exit(main())

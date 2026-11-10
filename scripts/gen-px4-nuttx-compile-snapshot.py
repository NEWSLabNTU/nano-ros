#!/usr/bin/env python3
"""Record what PX4 compiles a NuttX module's C++ with -- issue 2419.

`check-px4-uorb-nuttx-compile` compiles nano-ros's uORB backend and its PX4
push-wake glue the way PX4 compiles a module for a NuttX board. CI has no PX4
tree (the depth-1 `px4-autopilot` source has no NuttX submodule and no build
dir), so the gate reads a COMMITTED snapshot, and this script is what writes
it -- the pattern of `scripts/gen-upstream-edition.py`. Two things are
recorded, both read from PX4 rather than typed:

  * the compile options and include roots, taken from PX4's own
    `compile_commands.json`: the flag set most stock `px4_add_module()` C++
    TUs share (PX4's toolchain file, `px4_add_common_flags()`, the NuttX
    platform flags and the module default `-Wframe-larger-than`), with the
    per-module `MODULE_NAME` / `PX4_MAIN` values renamed and the output paths
    removed;
  * the headers the backend's TUs actually reach under those roots, found by
    `-M` against the real tree, plus BOTH NuttX `include/cxx` directories
    whole -- those directories are the C++ standard library a `-nostdinc++`
    PX4 build has, so their file LIST is the fact the gate checks (issue 2403:
    `<atomic>` is not in it).

Reproduce (any host with the pinned arm-none-eabi-gcc and PX4's python deps):

    git clone https://github.com/PX4/PX4-Autopilot.git px4 && cd px4
    git checkout <[source.px4-autopilot] ref in nros-sdk-index.toml>
    git submodule update --init platforms/nuttx/NuttX/nuttx platforms/nuttx/NuttX/apps
    cmake -S . -B ../px4-build -DCONFIG=px4_fmu-v6x_default -GNinja
    ninja -C ../px4-build nuttx_context uORB/topics/uORBTopics.hpp
    scripts/gen-px4-nuttx-compile-snapshot.py --px4 <px4> --build <px4-build>

The configure writes NuttX's context (`include/nuttx/config.h`, the arch
links) INTO the PX4 tree, which is why the tree must be one of your own.

Usage:
    scripts/gen-px4-nuttx-compile-snapshot.py --px4 DIR --build DIR [--cxx G++]
"""

from __future__ import annotations

import argparse
import collections
import json
import os
import shlex
import subprocess
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent / "lib"))
import px4_nuttx_compile as pnc  # noqa: E402

MODULE_DIRS = ("src/modules/", "src/drivers/", "src/systemcmds/")
# `px4_add_module(MAIN <m>)` defines MODULE_NAME / PX4_MAIN per module (PX4's
# log macros need the first); the snapshot keeps the defines, under this name.
MODULE = "nros_uorb_check"
# Removed from PX4's command line, each for a stated reason.
DROPPED = {
    "-fdiagnostics-color=always": "cosmetic: escape codes in a CI log",
}


def git(tree: Path, *args: str) -> str:
    return subprocess.run(["git", "-C", str(tree), *args], check=True,
                          capture_output=True, text=True).stdout.strip()


def split(entry: dict) -> tuple[tuple[str, ...], list[tuple[str, str]]]:
    """(options, include roots) of one compile_commands entry."""
    argv = shlex.split(entry["command"]) if "command" in entry else list(entry["arguments"])
    opts: list[str] = []
    roots: list[tuple[str, str]] = []
    it = iter(range(1, len(argv)))
    for i in it:
        a = argv[i]
        if a in ("-isystem", "-idirafter", "-I"):
            roots.append((a, argv[i + 1]))
            next(it)
        elif a.startswith("-I"):
            roots.append(("-I", a[2:]))
        elif a in ("-o", "-MF", "-MT", "-MQ"):
            next(it)
        elif a == "-include":
            opts += [a, argv[i + 1]]
            next(it)
        elif a in ("-c", "-MD", "-MMD") or a == entry["file"]:
            continue
        elif a.startswith("-DMODULE_NAME="):
            opts.append(f'-DMODULE_NAME="{MODULE}"')
        elif a.startswith("-DPX4_MAIN="):
            opts.append(f"-DPX4_MAIN={MODULE}_app_main")
        elif a in DROPPED:
            continue
        else:
            opts.append(a)
    return tuple(opts), roots


def to_base(path: str, bases: dict[str, Path]) -> str | None:
    p = os.path.normpath(path)
    for name in pnc.BASES:
        root = str(bases[name])
        if p == root or p.startswith(root + os.sep):
            return f"{name}/{os.path.relpath(p, root)}" if p != root else name
    return None


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    ap.add_argument("--px4", type=Path, required=True, help="PX4-Autopilot tree at the pin")
    ap.add_argument("--build", type=Path, required=True, help="its configured build dir")
    ap.add_argument("--cxx", default="arm-none-eabi-g++")
    args = ap.parse_args()
    bases = {"px4": args.px4.resolve(), "build": args.build.resolve()}

    ref = pnc.pinned_px4_ref()
    head = git(bases["px4"], "rev-parse", "HEAD")
    if head != ref:
        sys.exit(f"{bases['px4']} is at {head}, the pin is {ref} ([source.px4-autopilot])")
    for line in git(bases["px4"], "submodule", "status", "platforms/nuttx/NuttX/nuttx",
                    "platforms/nuttx/NuttX/apps").splitlines():
        if line[:1] in ("-", "+", "U"):
            sys.exit(f"NuttX submodule not at PX4's gitlink: {line}")
    cache = (bases["build"] / "CMakeCache.txt").read_text()
    config = next((ln.split("=", 1)[1] for ln in cache.splitlines()
                   if ln.startswith("CONFIG:")), None)
    for need in (bases["build"] / "px4_boardconfig.h",
                 bases["build"] / "uORB/topics/uORBTopics.hpp",
                 bases["px4"] / "platforms/nuttx/NuttX/nuttx/include/nuttx/config.h"):
        if not need.is_file():
            sys.exit(f"missing {need} -- run `ninja nuttx_context uORB/topics/uORBTopics.hpp`")

    entries = json.loads((bases["build"] / "compile_commands.json").read_text())
    modules = [e for e in entries if e["file"].endswith(".cpp")
               and any(f"{bases['px4']}/{d}" in e["file"] for d in MODULE_DIRS)]
    tally = collections.Counter(split(e)[0] for e in modules)
    opts, share = tally.most_common(1)[0]
    chosen = [e for e in modules if split(e)[0] == opts]
    common = set(split(chosen[0])[1])
    for e in chosen:
        common &= set(split(e)[1])
    example = chosen[0]
    include = []
    for kind, d in split(example)[1]:
        if (kind, d) not in common:
            continue
        rel = to_base(d, bases)
        if rel is None:
            sys.exit(f"include root outside the PX4 tree and build dir: {d}")
        include.append({"kind": kind, "dir": rel})

    manifest = {"flags": {"cxx": list(opts), "include": include}}
    # The closure, against the REAL trees.
    files: set[str] = set()
    for src in pnc.backend_sources():
        # -fno-canonical-system-headers: by default GCC prints a SYSTEM header
        # by its resolved path, which drops the NuttX `include/arch/*` links
        # the include is spelled through, and the copy would then sit where
        # no `#include` looks.
        argv = pnc.compile_argv(args.cxx, manifest, bases, src,
                                ["-M", "-fno-canonical-system-headers"])
        r = subprocess.run(argv, capture_output=True, text=True)
        if r.returncode != 0:
            sys.exit(f"the backend does not compile against the real PX4 tree:\n"
                     f"{shlex.join(argv)}\n{r.stderr}")
        for dep in r.stdout.replace("\\\n", " ").split()[1:]:
            rel = to_base(dep, bases)
            if rel is not None:
                files.add(rel)
    for root in include:
        if root["kind"] == "-isystem" and root["dir"].endswith("include/cxx"):
            base, _, rel = root["dir"].partition("/")
            for f in sorted((bases[base] / rel).iterdir()):
                if f.is_file():
                    files.add(f"{root['dir']}/{f.name}")

    blobs = {}
    for rel in sorted(files):
        base, _, sub = rel.partition("/")
        blobs[rel] = (bases[base] / sub).read_bytes()
    pnc.write_headers_tar(blobs)

    version = subprocess.run([args.cxx, "--version"], capture_output=True,
                             text=True).stdout.splitlines()[0]
    q = json.dumps
    out = [
        "# GENERATED by scripts/gen-px4-nuttx-compile-snapshot.py -- do not edit (issue 2419).",
        "# What PX4 compiles a NuttX module's C++ with, read from a configured PX4 tree",
        "# at the pin, and the PX4 / NuttX headers nano-ros's uORB backend reaches under",
        "# it. The headers in headers.tar.xz are verbatim copies and keep their upstream",
        "# licences (PX4: BSD-3-Clause, NuttX: Apache-2.0), stated in each file.",
        "# check-px4-uorb-nuttx-compile reads this; regenerate when the pin moves.",
        "",
        "[px4]",
        f"version = {q(git(bases['px4'], 'describe', '--tags'))}",
        f"ref = {q(head)}",
        f"nuttx_ref = {q(git(bases['px4'] / 'platforms/nuttx/NuttX/nuttx', 'rev-parse', 'HEAD'))}",
        f"apps_ref = {q(git(bases['px4'] / 'platforms/nuttx/NuttX/apps', 'rev-parse', 'HEAD'))}",
        f"config = {q(config)}",
        f"generated_with = {q(version)}",
        "",
        "[flags]",
        f"derived_from = {q(f'the option set {share} of {len(modules)} px4_add_module C++ TUs share')}",
        f"example_tu = {q(os.path.relpath(example['file'], bases['px4']))}",
        "dropped = [" + ", ".join(q(f"{k}: {v}") for k, v in DROPPED.items()) + "]",
        "cxx = [",
        *(f"    {q(o)}," for o in opts),
        "]",
        "include = [",
        *(f"    {{ kind = {q(r['kind'])}, dir = {q(r['dir'])} }}," for r in include),
        "]",
        "",
        "# The archive is hashed whole (it is byte-reproducible); its member names",
        "# are not listed here, where repo-wide scanners would read them as ours.",
        "[headers]",
        f"sha256 = {q(pnc.sha256(pnc.HEADERS_TAR))}",
        f"members = {len(blobs)}",
        "",
    ]
    pnc.MANIFEST.write_text("\n".join(out))
    print(f"wrote {pnc.MANIFEST.relative_to(pnc.REPO)}: {len(opts)} options, "
          f"{len(include)} roots, {len(blobs)} headers")
    return 0


if __name__ == "__main__":
    sys.exit(main())

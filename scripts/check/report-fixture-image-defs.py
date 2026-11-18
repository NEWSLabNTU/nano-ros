#!/usr/bin/env python3
"""report-fixture-image-defs: what each image fixture row's `-D`s say beside its image.

phase-568 W3b, report-only. `workspace-fixtures-build.sh` hands a migrated
(`image = …`) cmake row's `-D` defs to `nros build` after `--`: the row's
`rmw` coordinate as `-DNROS_RMW`, plus its `cmake_defs`. RFC-0117 D8 narrows
`--` to west's own options (phase-568 W6), so every one of those must either
be the image's own fact already (redundant, safe to drop), contradict it (a
live fixture bug today: the row builds something other than its image says),
or add something the image does not state (it needs a new home first).

Each `-D` is classified against the image as `system.toml` resolves it:
- `NROS_RMW` against `[image] rmw` > `[image_defaults] rmw` > `[system] rmw`;
- `NROS_BOARD` against the image's `board`, or any name its descriptor lists;
- `NROS_PLATFORM` against that descriptor's `platform`;
- `NROS_ENTRY_LOCATOR` against the image's `locator`, when it states one.
Anything else ADDS. Exit 0 always: this reports, it does not gate.
"""
from __future__ import annotations

import collections
import subprocess
import sys
try:
    import tomllib
except ModuleNotFoundError:  # Python < 3.11
    import tomli as tomllib
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / "scripts" / "lib"))
import system_toml  # noqa: E402  issue 2587, the one system.toml reader


def rows():
    out = subprocess.run(
        [sys.executable, str(ROOT / "scripts/build/fixtures-manifest.py"), "list-workspaces"],
        capture_output=True, text=True, check=True,
    ).stdout
    for line in out.splitlines():
        f = line.split("\x1f")
        if len(f) < 14:
            continue
        builder = f[14] if len(f) > 14 else ""
        yield {"id": f[0], "lang": f[1], "dir": f[2], "bringup": f[3], "defs": f[8],
               "image": f[13], "builder": builder}


def descriptors():
    by_name = {}
    listed = subprocess.run(
        ["git", "-C", str(ROOT), "ls-files", "packages/boards/**/nros-board.toml"],
        capture_output=True, text=True, check=True,
    ).stdout.split()
    for rel in sorted(listed):
        p = ROOT / rel
        if "/third-party/" in rel:
            continue
        d = tomllib.loads(p.read_text())
        boards = d.get("board", {})
        # One `[board]` table, or several `[[board]]` entries in one file.
        for board in boards if isinstance(boards, list) else [boards]:
            names = board.get("names") or d.get("names") or []
            plat = board.get("platform") or d.get("platform")
            for n in names:
                by_name[n] = {"names": set(names), "platform": plat}
    return by_name


def classify(name, value, img, sysd, desc):
    if name == "NROS_RMW":
        want = img.get("rmw") or (sysd.get("image_defaults") or {}).get("rmw") \
            or (sysd.get("system") or {}).get("rmw")
    elif name == "NROS_BOARD":
        board = img.get("board")
        d = desc.get(board)
        if board is None:
            return "adds", None
        return ("redundant" if value == board or (d and value in d["names"]) else "contradicts"), board
    elif name == "NROS_PLATFORM":
        d = desc.get(img.get("board"))
        want = d["platform"] if d else None
    elif name == "NROS_ENTRY_LOCATOR":
        want = img.get("locator")
    else:
        return "adds", None
    if want is None:
        return "adds", None
    if value == want:
        return "redundant", want
    # CMake's `NROS_PLATFORM` axis is the FAMILY (`threadx`, tested as
    # `MATCHES "^threadx"`), and a descriptor's `platform` is the coordinate
    # (`threadx-linux`). The same platform, spelled at another level, is not a
    # contradiction; but `nros build` passes no `NROS_PLATFORM` of its own, so
    # whether this one is redundant is what W6's build on each side measures.
    if name == "NROS_PLATFORM" and want.split("-")[0] == value:
        return "family", want
    return "contradicts", want


def main() -> int:
    desc = descriptors()
    counts = collections.Counter()
    by_name = collections.defaultdict(collections.Counter)
    contradictions, additions = [], collections.Counter()
    seen_rows = 0
    for r in rows():
        if not r["image"] or not r["defs"] or r["lang"] not in ("c", "cpp") or r["builder"] == "cargo":
            continue
        seen_rows += 1
        sysd = system_toml.load(ROOT / r["dir"] / r["bringup"] / "system.toml")  # issue 2587
        img = (sysd.get("image") or {}).get(r["image"], {})
        for d in r["defs"].split():
            name, _, value = d.removeprefix("-D").partition("=")
            kind, want = classify(name, value, img, sysd, desc)
            counts[kind] += 1
            by_name[name][kind] += 1
            if kind == "contradicts":
                contradictions.append(f"{r['id']}: -D{name}={value}, image `{r['image']}` says {want!r}")
            elif kind == "adds":
                additions[f"-D{name}"] += 1
    print(f"report-fixture-image-defs: {seen_rows} image cmake row(s) pass -D defs after `--`")
    print(f"  -D's: {counts['redundant']} redundant, {counts['contradicts']} contradicting, "
          f"{counts['family']} the same platform at family level, {counts['adds']} adding something")
    for n, c in sorted(by_name.items()):
        print(f"    {n}: " + ", ".join(f"{k} {v}" for k, v in sorted(c.items())))
    if contradictions:
        print("  CONTRADICTING (the row builds other than its image says):")
        for c in contradictions:
            print(f"    {c}")
    if additions:
        print("  ADDING (needs a home before `--` narrows): " +
              ", ".join(f"{k} x{v}" for k, v in additions.most_common()))
    return 0


if __name__ == "__main__":
    sys.exit(main())

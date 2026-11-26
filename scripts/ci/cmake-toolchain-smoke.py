#!/usr/bin/env python3
"""Issue 2990: one CMake-road image per cross-toolchain file, configured and linked.

`just ci matrix build` was `l3` alone: the cargo-road RTOS link check and ELF
checks on three Rust images. It configured no CMake image, so a change to
`cmake/toolchain/*.cmake` passed it unbuilt (#4928). The user's ruling
(2026-11-27): build depth also configures and links one cheap CMake-road
image per cross-toolchain file, with the list DERIVED from the toolchain
files, so a new toolchain file without a smoke fails.

Every link in the chain is read, never authored:

    cmake/toolchain/<f>.cmake    a toolchain is a file that sets CMAKE_SYSTEM_NAME
      <- nros-board.toml          a board names its `toolchain_file`
      <- system.toml              an image names its `board`
      <- examples/fixtures.toml   a C/C++ row builds that image

The first such row in manifest order is the file's smoke. Two cases have no
row, and each needs a reason:

- A file that only `include()`s another toolchain file adds nothing to build,
  so it is credited through the file it includes. One that also sets anything
  of its own is a toolchain in its own right and needs its own smoke.
- `NO_ROW` below: a toolchain no in-tree board uses, smoked by the check
  recipes that build through it. An entry that names a missing file, or a file
  that has gained a row, is refused, so the table cannot rot.

Usage:
    cmake-toolchain-smoke.py --plan       print the plan; rc 1 if a file has no smoke
    cmake-toolchain-smoke.py --run        build each row and run each recipe
    cmake-toolchain-smoke.py --self-test  the derivation's negative controls
"""

from __future__ import annotations

import argparse
import re
import shutil
import subprocess
import sys
import tempfile
import time
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / "scripts" / "lib"))
import system_toml  # noqa: E402  issue 2587, the one system.toml reader

try:
    import tomllib
except ModuleNotFoundError:  # Python < 3.11
    import tomli as tomllib  # type: ignore[no-redef]

TOOLCHAIN_DIR = ROOT / "cmake" / "toolchain"

# Toolchain files no in-tree board names, and the check recipes that configure
# and link through them instead. Each states why it has no row.
NO_ROW: dict[str, tuple[list[str], str]] = {
    "arm-freertos-armcr5f.cmake": (
        ["r5f-hard-float-abi", "out-of-tree-board-images"],
        "no in-tree board uses the Cortex-R5F profile yet (phase-575); the first "
        "recipe builds a C and C++ STATIC library through it and links it with the "
        "Rust half, the second builds the out-of-tree R5F board image",
    ),
    "arm-freertos-tiarmclang-cr5f.cmake": (
        ["tiarmclang-image"],
        "TI Arm Clang is licence-gated and never fetched (user ruling); the "
        "recipe needs `tiarmclang`, which no lane claims today, so the smoke "
        "reports it out of scope rather than running it, and runs it once a lane "
        "claims the capability",
    ),
}

SETS_SYSTEM_NAME = re.compile(r"^\s*set\s*\(\s*CMAKE_SYSTEM_NAME\b", re.MULTILINE)
INCLUDE = re.compile(r'^\s*include\s*\(\s*"\$\{CMAKE_CURRENT_LIST_DIR\}/([^"/]+\.cmake)"\s*\)\s*$')


def _statements(path: Path) -> list[str]:
    out = []
    for line in path.read_text().splitlines():
        s = line.split("#", 1)[0].strip()
        if s:
            out.append(s)
    return out


def classify(tdir: Path) -> tuple[list[str], dict[str, str]]:
    """(toolchain files that need a smoke, {include-only file: the file it includes})."""
    names = {p.name for p in tdir.glob("*.cmake")}
    toolchains: list[str] = []
    credited: dict[str, str] = {}
    for p in sorted(tdir.glob("*.cmake")):
        text = p.read_text()
        stmts = _statements(p)
        includes = [m.group(1) for s in stmts if (m := INCLUDE.match(s))]
        includes_toolchain = [i for i in includes if i in names and SETS_SYSTEM_NAME.search((tdir / i).read_text())]
        if SETS_SYSTEM_NAME.search(text):
            toolchains.append(p.name)
        elif includes_toolchain:
            if len(stmts) == 1:
                credited[p.name] = includes_toolchain[0]
            else:
                toolchains.append(p.name)  # it sets something of its own
    return toolchains, credited


def _find_toolchain_files(node, out: set[str]) -> None:
    if isinstance(node, dict):
        for k, v in node.items():
            if k == "toolchain_file" and isinstance(v, str):
                out.add(Path(v).name)
            else:
                _find_toolchain_files(v, out)
    elif isinstance(node, list):
        for v in node:
            _find_toolchain_files(v, out)


def board_toolchains() -> dict[str, set[str]]:
    """{board name: the toolchain files its descriptor names}."""
    out: dict[str, set[str]] = {}
    for f in sorted((ROOT / "packages" / "boards").glob("*/nros-board.toml")):
        doc = tomllib.loads(f.read_text())
        for entry in doc.get("board", []):
            files: set[str] = set()
            _find_toolchain_files(entry, files)
            for n in entry.get("names", []):
                out.setdefault(n, set()).update(files)
    return out


def rows() -> list[dict]:
    """The C/C++ workspace rows, in manifest order, with the board their image names."""
    out = subprocess.run(
        [sys.executable, str(ROOT / "scripts/build/fixtures-manifest.py"), "list-workspaces"],
        capture_output=True, text=True, check=True, cwd=ROOT,
    ).stdout
    found = []
    for line in out.splitlines():
        f = line.split("\x1f")
        if len(f) < 14 or f[1] not in ("c", "cpp", "mixed"):
            continue
        board = None
        m = re.search(r"-DNROS_BOARD=(\S+)", f[8])
        sysf = ROOT / f[2] / f[3] / "system.toml"
        if sysf.is_file():
            doc = system_toml.load(sysf)
            img = (doc.get("image") or {}).get(f[13]) or {}
            board = img.get("board") or (doc.get("image_defaults") or {}).get("board")
        if not board and m:
            board = m.group(1)
        if board:
            found.append({"id": f[0], "lang": f[1], "board": board})
    return found


def plan(tdir: Path = TOOLCHAIN_DIR, no_row: dict | None = None) -> tuple[list[tuple[str, str, object]], list[str]]:
    """([(toolchain file, kind, what)], [errors]). kind: row | recipes | credited."""
    no_row = NO_ROW if no_row is None else no_row
    toolchains, credited = classify(tdir)
    boards = board_toolchains()
    rs = rows()
    entries: list[tuple[str, str, object]] = []
    errors: list[str] = []
    for t in toolchains:
        users = {b for b, fs in boards.items() if t in fs}
        row = next((r for r in rs if r["board"] in users), None)
        if row and t in no_row:
            errors.append(f"{t}: NO_ROW names it, but row {row['id']} now builds it -- drop the entry")
        if row:
            entries.append((t, "row", row))
        elif t in no_row:
            entries.append((t, "recipes", no_row[t][0]))
        else:
            errors.append(
                f"{t}: no smoke. No C/C++ fixture row builds an image whose board names it "
                f"(boards naming it: {sorted(users) or 'none'}). Add a row, or a NO_ROW entry "
                f"in {Path(__file__).relative_to(ROOT)} naming the check recipes that build "
                "through it, with the reason")
    for t, inc in credited.items():
        entries.append((t, "credited", inc))
    known = recipe_needs()
    for t, (recipes, _why) in no_row.items():
        for r in recipes:
            if r not in known:
                errors.append(f"NO_ROW: {t} names `just check {r}`, which is not a check recipe")
    for t in no_row:
        if not (tdir / t).is_file():
            errors.append(f"NO_ROW names {t}, which is not in {tdir.relative_to(ROOT) if tdir.is_relative_to(ROOT) else tdir}")
    return entries, errors


def recipe_needs() -> dict[str, set[str]]:
    """{check recipe: the capabilities its `[group('needs:<cap>')]` declare}."""
    import json
    dump = json.loads(subprocess.run(["just", "--dump", "--dump-format", "json"], cwd=ROOT,
                                     capture_output=True, text=True, check=True).stdout)
    out = {}
    for name, r in dump["modules"]["check"]["recipes"].items():
        out[name] = {a["group"][len("needs:"):] for a in r.get("attributes", [])
                     if isinstance(a, dict) and str(a.get("group", "")).startswith("needs:")}
    return out


def claimed_anywhere() -> set[str]:
    """Every capability some lane in .config/gate-lanes.toml claims."""
    doc = tomllib.loads((ROOT / ".config" / "gate-lanes.toml").read_text())
    return {c for lane in (doc.get("lane") or {}).values() for c in lane.get("claims", [])}


def out_of_scope(recipe: str, needs: dict[str, set[str]], claimed: set[str]) -> set[str]:
    """The needs of `recipe` that no lane claims. Such a recipe is out of every lane's
    scope, as the gate harness leaves it out, so the smoke reports it and does not run it:
    run anyway, it fails on a missing capability, which is not a toolchain verdict."""
    return needs.get(recipe, set()) - claimed


def describe(entries) -> str:
    lines = []
    for t, kind, what in entries:
        if kind == "row":
            lines.append(f"  {t:42} row {what['id']} ({what['lang']}, board {what['board']})")
        elif kind == "recipes":
            needs, claimed = recipe_needs(), claimed_anywhere()
            shown = [r + (f" (out of scope: needs {', '.join(sorted(m))}, claimed by no lane)"
                          if (m := out_of_scope(r, needs, claimed)) else "") for r in what]
            lines.append(f"  {t:42} just check {'; just check '.join(shown)}")
        else:
            lines.append(f"  {t:42} credited through {what} (include-only)")
    return "\n".join(lines)


def platform_of(row_id: str) -> str:
    out = subprocess.run(
        [sys.executable, str(ROOT / "scripts/build/fixtures-manifest.py"), "describe-id", "--id", row_id],
        capture_output=True, text=True, check=True, cwd=ROOT,
    ).stdout.strip()
    return out.split("\x1f")[1]


def run(entries) -> int:
    rc = 0
    total = time.monotonic()
    for t, kind, what in entries:
        if kind == "credited":
            continue
        if kind == "row":
            cmds = [["bash", "scripts/build/workspace-fixtures-build.sh", platform_of(what["id"]),
                     what["lang"], "--id", what["id"]]]
        else:
            needs, claimed = recipe_needs(), claimed_anywhere()
            cmds = []
            for r in what:
                missing = out_of_scope(r, needs, claimed)
                if missing:
                    print(f"cmake-toolchain-smoke: {t}: just check {r} OUT OF SCOPE -- needs "
                          f"{', '.join(sorted(missing))}, which no lane in .config/gate-lanes.toml "
                          "claims; not run, and not a pass", flush=True)
                else:
                    cmds.append(["just", "check", r])
        for cmd in cmds:
            t0 = time.monotonic()
            r = subprocess.run(cmd, cwd=ROOT).returncode
            print(f"cmake-toolchain-smoke: {t}: {' '.join(cmd[-3:])} rc={r} "
                  f"({time.monotonic() - t0:.0f} s)", flush=True)
            rc = rc or r
    print(f"cmake-toolchain-smoke: {'FAILED' if rc else 'OK'} in {time.monotonic() - total:.0f} s")
    return rc


def self_test() -> int:
    """Negative controls on a copy of the toolchain dir: each mutation must be refused."""
    fails = []
    with tempfile.TemporaryDirectory() as d:
        base = Path(d) / "base"
        shutil.copytree(TOOLCHAIN_DIR, base)
        _, errs = plan(base)
        if errs:
            fails.append(f"the real tree must plan clean: {errs}")

        new = Path(d) / "new"
        shutil.copytree(TOOLCHAIN_DIR, new)
        (new / "acme-freertos-m7.cmake").write_text("set(CMAKE_SYSTEM_NAME Generic)\n")
        _, errs = plan(new)
        if not any(e.startswith("acme-freertos-m7.cmake: no smoke") for e in errs):
            fails.append("a new toolchain file with no row and no NO_ROW entry was not refused")

        alias = Path(d) / "alias"
        shutil.copytree(TOOLCHAIN_DIR, alias)
        (alias / "acme-alias.cmake").write_text(
            '# an alias\ninclude("${CMAKE_CURRENT_LIST_DIR}/riscv64-threadx.cmake")\n')
        ents, errs = plan(alias)
        if ("acme-alias.cmake", "credited", "riscv64-threadx.cmake") not in ents or errs:
            fails.append(f"an include-only file was not credited through its include: {errs}")
        (alias / "acme-alias.cmake").write_text(
            'include("${CMAKE_CURRENT_LIST_DIR}/riscv64-threadx.cmake")\n'
            'set(CMAKE_C_FLAGS_INIT "-O3")\n')
        _, errs = plan(alias)
        if not any(e.startswith("acme-alias.cmake: no smoke") for e in errs):
            fails.append("an include file that sets its own flag was credited instead of needing a smoke")

        stale = dict(NO_ROW)
        stale["gone.cmake"] = (["x"], "y")
        _, errs = plan(base, stale)
        if not any("NO_ROW names gone.cmake" in e for e in errs):
            fails.append("a NO_ROW entry for a missing file was not refused")
    needs = {"a": {"cmake"}, "b": {"cmake", "licensed-thing"}}
    if out_of_scope("a", needs, {"cmake"}) or out_of_scope("b", needs, {"cmake"}) != {"licensed-thing"}:
        fails.append("a recipe needing a capability no lane claims was not reported out of scope")
    for f in fails:
        print(f"cmake-toolchain-smoke SELFTEST FAILED: {f}", file=sys.stderr)
    return 1 if fails else 0


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__.split("\n", 1)[0])
    g = ap.add_mutually_exclusive_group(required=True)
    g.add_argument("--plan", action="store_true")
    g.add_argument("--run", action="store_true")
    g.add_argument("--self-test", action="store_true")
    a = ap.parse_args()
    if a.self_test:
        return self_test()
    entries, errors = plan()
    print("cmake-toolchain-smoke: one CMake-road image per cross-toolchain file (issue 2990)")
    print(describe(entries))
    if errors:
        for e in errors:
            print(f"cmake-toolchain-smoke: {e}", file=sys.stderr)
        return 1
    return run(entries) if a.run else 0


if __name__ == "__main__":
    sys.exit(main())

#!/usr/bin/env python3
"""The codegen UNITS of the tree, and which of them a fixture lane reaches.

phase-535 W10 (issue 2475 defect 4). `scripts/regenerate-bindings.sh` used to
discover its units itself and generate every one of them, whatever the lane:
about 30 colcon roots plus every standalone example and test-bin package,
measured at 38-40 min inside `just build tier1` on a hosted runner before a
single fixture was built. A one-board shard needs a handful of them.

A unit is one thing `regenerate-bindings.sh` generates into:

  * `workspace`  — a colcon root (`<root>/src/<member>/package.xml`), one
    shared `generated/` written by `nros dev bindings <root>`;
  * `standalone` — an example `package.xml` dir outside every colcon root,
    with its own per-package `generated/`;
  * `testbin`    — a `package.xml` dir under the nros-bench / nros-tests bins /
    nros-smoke test-bin trees.

ONE discovery, two readers: the generator (through the `units` verb) and the
`_require-leaf-includes` guard (through `scoped_units()`), so the guard checks
exactly the leaves the generator produced, never a wider set it then refuses.

Scope: with `NROS_CODEGEN_COORDS=<lane coords file>` (or `--coords-from`), a
unit is kept when it and an in-lane row's dir are NESTED either way (the row is
inside the unit, or the unit inside the row). The row dirs come from
`fixtures-manifest.py lane-dirs`, the build's own predicate. Nesting in both
directions only ever keeps MORE: a row naming a workspace keeps every unit
under it, and a row inside a colcon root keeps that root. Without a coords
file nothing is narrowed (`lane=all`, `lane=linux`, a bare
`just generate-bindings`).

Usage:
    codegen_scope.py units [--coords-from FILE]   # `<kind>\\t<dir>` lines
"""
from __future__ import annotations

import os
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
COORDS_ENV = "NROS_CODEGEN_COORDS"

TESTBIN_GLOBS = (
    "packages/testing/nros-bench/**/package.xml",
    "packages/testing/nros-tests/bins/**/package.xml",
    "packages/testing/nros-smoke/**/package.xml",
)


def _ls(*globs: str) -> list[str]:
    out = subprocess.run(
        ["git", "ls-files", *globs], cwd=ROOT, capture_output=True, text=True,
        check=True,
    ).stdout
    return sorted(set(out.split()))


def _parent(rel: str) -> str:
    return rel.rsplit("/", 1)[0]


def _nested(a: str, b: str) -> bool:
    """True when `a` and `b` are the same dir or one contains the other."""
    return a == b or a.startswith(b + "/") or b.startswith(a + "/")


def all_units() -> list[tuple[str, str]]:
    """Every codegen unit, `(kind, repo-relative dir)`, in generation order."""
    ws_roots = set()
    # Two depths: `examples/<ws>/src/<member>` and
    # `examples/templates/<t>/src/<member>` (the old single-depth glob skipped
    # every template workspace).
    for pxml in _ls("examples/*/src/*/package.xml", "examples/*/*/src/*/package.xml"):
        member = _parent(pxml)
        src = _parent(member)
        if src.rsplit("/", 1)[-1] != "src":
            continue
        ws_roots.add(_parent(src))
    units = [("workspace", r) for r in sorted(ws_roots)]
    for pxml in _ls("examples/**/package.xml"):
        d = _parent(pxml)
        if any(d == r or d.startswith(r + "/") for r in ws_roots):
            continue
        units.append(("standalone", d))
    for pxml in _ls(*TESTBIN_GLOBS):
        units.append(("testbin", _parent(pxml)))
    return units


def lane_dirs(coords: str) -> list[str]:
    """Source dirs of every row the lane's fixture build can reach."""
    out = subprocess.run(
        [sys.executable, str(ROOT / "scripts/build/fixtures-manifest.py"),
         "lane-dirs", "--manifest", str(ROOT / "examples/fixtures.toml"),
         "--coords-from", coords],
        cwd=ROOT, capture_output=True, text=True,
    )
    if out.returncode != 0:
        raise SystemExit(f"codegen-scope: lane-dirs failed:\n{out.stderr}")
    return [d for d in out.stdout.split("\n") if d]


def select(units, dirs):
    """The units nested (either way) with at least one row dir."""
    return [(k, u) for k, u in units if any(_nested(u, d) for d in dirs)]


def scoped_units(coords: str | None = None):
    """`(units, narrowed)`: the units this run generates, and whether a lane
    narrowed them."""
    coords = coords or os.environ.get(COORDS_ENV) or None
    units = all_units()
    if not coords:
        return units, False
    return select(units, lane_dirs(coords)), True


def self_test() -> int:
    units = [
        ("workspace", "examples/workspaces/rust"),
        ("workspace", "examples/workspaces/c"),
        ("standalone", "examples/zephyr/rust/talker"),
        ("testbin", "packages/testing/nros-tests/bins/a"),
    ]
    cases = [
        # a row inside a colcon root keeps that root, and only that root
        (["examples/workspaces/rust/src/talker"], ["examples/workspaces/rust"]),
        # a row naming the workspace itself
        (["examples/workspaces/c"], ["examples/workspaces/c"]),
        # a row naming a parent keeps every unit under it
        (["examples/zephyr"], ["examples/zephyr/rust/talker"]),
        # a sibling with a shared PREFIX is not nested
        (["examples/workspaces/rust-extra"], []),
        (["packages/testing/nros-tests/bins/a"], ["packages/testing/nros-tests/bins/a"]),
    ]
    for dirs, want in cases:
        got = [u for _, u in select(units, dirs)]
        assert got == want, (dirs, got, want)
    print(f"codegen-scope self-test: {len(cases)} case(s) OK")
    return 0


def main(argv: list[str]) -> int:
    if argv[1:] == ["--self-test"]:
        return self_test()
    if len(argv) < 2 or argv[1] != "units":
        print("usage: codegen_scope.py units [--coords-from FILE] | --self-test", file=sys.stderr)
        return 2
    coords = None
    if argv[2:4] and argv[2] == "--coords-from":
        coords = argv[3]
    units, narrowed = scoped_units(coords)
    all_n = len(all_units()) if narrowed else len(units)
    for kind, d in units:
        print(f"{kind}\t{d}")
    print(f"codegen scope: {len(units)} of {all_n} unit(s)"
          + (" (narrowed to the lane's fixture rows)" if narrowed else ""),
          file=sys.stderr)
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))

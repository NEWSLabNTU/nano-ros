#!/usr/bin/env python3
"""check-workspace-fixture-road — issue 2463.

A `[[workspace_fixture]]` row is built down the cargo road or the cmake road:
its authored `builder` (the record's 15th column) when it states one, else its
language. That decision is spelled in bash (`nros_ws_row_road`,
`scripts/lib/fixture-road.sh`, used by `workspace-fixtures-build.sh`) and in
Python (`row_road`, `scripts/build/workspace-fixture-units.py`, which groups the
rows into build units). The two drifted once: the bash copy tested the variable
it was assigning, so under `set -u` every row with no `builder` stopped at
`road: unbound variable` and built nothing, while the Python copy grouped those
rows correctly. No gate noticed, because `ci gate` builds no fixture.

This runs EVERY manifest row through both and fails on any disagreement. It
also runs a fixed table of the cases that matter (each language with no
builder, and an authored builder on each), so a manifest that happens to lack
a shape cannot hide a broken arm. The Rust twin
(`nros_tests::fixtures::migrated_leaf::road`) has a unit test over that same
table.

Buildless, about 0.3 s. It self-tests its comparison on every run: a twin that
disagrees on one row must be reported.
"""
from __future__ import annotations

import importlib.util
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
SEP = "\x1f"

# (lang, builder, road): the shapes every twin must agree on.
TABLE = [
    ("rust", "", "cargo"),
    ("c", "", "cmake"),
    ("cpp", "", "cmake"),
    ("mixed", "", "cmake"),
    ("c", "cargo", "cargo"),
    ("cpp", "cargo", "cargo"),
    ("rust", "cmake", "cmake"),
    ("mixed", "cmake", "cmake"),
]


def python_road():
    spec = importlib.util.spec_from_file_location(
        "workspace_fixture_units", ROOT / "scripts/build/workspace-fixture-units.py"
    )
    mod = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(mod)
    return mod.row_road


def bash_roads(pairs: list[tuple[str, str]]) -> list[str]:
    """`nros_ws_row_road` over every pair, in ONE bash process."""
    script = (
        f'source "{ROOT}/scripts/lib/fixture-road.sh"\n'
        "while IFS=$'\\x1f' read -r lang builder; do nros_ws_row_road \"$lang\" \"$builder\"; done\n"
    )
    stdin = "".join(f"{lang}{SEP}{builder}\n" for lang, builder in pairs)
    out = subprocess.run(
        ["bash", "-c", script], input=stdin, capture_output=True, text=True, check=True
    ).stdout.splitlines()
    if len(out) != len(pairs):
        raise RuntimeError(f"bash answered {len(out)} of {len(pairs)} rows")
    return out


def disagreements(rows, bash, python) -> list[str]:
    """rows: (label, lang, builder, want-or-None). One line per disagreement."""
    got_bash = bash([(lang, b) for _, lang, b, _ in rows])
    found = []
    for (label, lang, builder, want), b in zip(rows, got_bash):
        p = python(lang, builder)
        if b != p or (want is not None and b != want):
            found.append(
                f"{label}: lang={lang!r} builder={builder!r} -> bash {b!r}, python {p!r}"
                + (f", want {want!r}" if want is not None else "")
            )
    return found


def manifest_rows():
    out = subprocess.run(
        [sys.executable, str(ROOT / "scripts/build/fixtures-manifest.py"), "list-workspaces"],
        capture_output=True,
        text=True,
        check=True,
    ).stdout
    rows = []
    for rec in out.splitlines():
        f = rec.split(SEP) + [""] * 15
        rows.append((f"row {f[0]}", f[1], f[14], None))
    return rows


def self_test(python) -> int:
    table = [(f"table {lang}/{b or '-'}", lang, b, want) for lang, b, want in TABLE]
    found = disagreements(table, bash_roads, python)
    if found:
        print("check-workspace-fixture-road --self-test: the twins disagree on the fixed table:")
        for line in found:
            print(f"  {line}")
        return 1
    # Negative control: a bash twin that sends a builder-less Rust row to cmake
    # (the shape of the original defect's `set +u` reading) must be caught.
    def broken(pairs):
        return ["cmake" if (lang == "rust" and not b) else r for (lang, b), r in zip(pairs, bash_roads(pairs))]
    if not disagreements(table, broken, python):
        print("check-workspace-fixture-road --self-test: FAILED — a twin that disagrees was not reported")
        return 1
    return 0


def main() -> int:
    python = python_road()
    if self_test(python):
        return 1
    table = [(f"table {lang}/{b or '-'}", lang, b, want) for lang, b, want in TABLE]
    rows = manifest_rows()
    if not rows:
        print("check-workspace-fixture-road: the manifest listed no workspace rows (wrong root, or the query failed)")
        return 1
    found = disagreements(table + rows, bash_roads, python)
    if found:
        print("check-workspace-fixture-road: the bash and Python roads disagree:\n")
        for line in found:
            print(f"  {line}")
        print(
            "\nOne decision, two spellings (issue 2463): `nros_ws_row_road` in "
            "scripts/lib/fixture-road.sh and `row_road` in "
            "scripts/build/workspace-fixture-units.py must answer alike."
        )
        return 1
    print(
        f"check-workspace-fixture-road: OK ({len(rows)} manifest row(s) and "
        f"{len(TABLE)} table case(s), bash and Python agree)"
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())

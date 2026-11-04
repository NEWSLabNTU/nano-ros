#!/usr/bin/env python3
"""Phase-535 W1 (RFC-0122 D5/D10) — the example coverage ratchet.

Phase-535 moves the example pairs out of standalone leaves and into the
language workspaces, and RFC-0122 D10 removes hand-written `main`s from every
example that is not a manual-entry demo. Both delete directories, and a
deleted directory can take with it the last fixture exercising some way of
building or entering an image on some platform. Nothing else would notice:
the pair cells keep their names (RFC-0122 D4), so every test stays green.

This gate derives, from `examples/fixtures.toml`, one tuple per fixture row:

    (platform, lang, driver, shape, launch, entry)

    platform  the row's fixture platform token (`row_coord`)
    lang      rust | c | cpp | mixed
    driver    cargo | cmake | west        — the build tool that drives it
    shape     single | multi              — the image's project holds one image
                                            (a leaf, or a one-image bringup) or
                                            several
    launch    generated | authored        — who wrote the launch file
    entry     generated | manual          — `nros::main!` / a generated entry,
                                            or a hand-written `main`

and fails when a tuple in the committed FLOOR
(`.config/example-coverage-floor.txt`) is no longer produced by any row. The
floor may only grow: `--write-floor` adds what the tree now produces and never
removes a line. Removing a line is a deliberate edit, made in a commit that
says why — RFC-0122 D10 is one such reason (a hand-written `main` that is not a
manual-entry demo).

Derived, never listed by hand: an authored list of "what we cover" drifts
toward OK (`check-default-gates-run-somewhere`, issue 1040).

    python3 scripts/check-example-coverage-ratchet.py              # check
    python3 scripts/check-example-coverage-ratchet.py --list       # print tuples
    python3 scripts/check-example-coverage-ratchet.py --write-floor
    python3 scripts/check-example-coverage-ratchet.py --self-test
"""
from __future__ import annotations

import argparse
import re
import sys
import tempfile
from pathlib import Path

try:
    import tomllib  # Python 3.11+
except ModuleNotFoundError:  # pragma: no cover
    import tomli as tomllib

REPO = Path(__file__).resolve().parents[1]
MANIFEST = Path("examples/fixtures.toml")
FLOOR = Path(".config/example-coverage-floor.txt")
DEFAULT_RMW = "zenoh"

FIELDS = ("platform", "lang", "driver", "shape", "launch", "entry")


def _toml(path: Path) -> dict:
    try:
        with open(path, "rb") as f:
            return tomllib.load(f)
    except (OSError, tomllib.TOMLDecodeError):
        return {}


def _is_cargo_row(row: dict) -> bool:
    # The same rule as fixtures-manifest.py `is_cargo_row` (builder first,
    # then the language default). Kept in step by the self-test below.
    b = row.get("builder")
    if b == "cmake" or b == "west":
        return False
    if b == "cargo":
        return True
    return row.get("lang") not in ("c", "cpp")


def _leaf_entry(leaf: Path, lang: str) -> str:
    """`manual` when the leaf's own sources carry a hand-written `main`."""
    src = leaf / "src"
    if lang == "rust":
        main = src / "main.rs"
        if not main.is_file():
            return "generated"
        return "generated" if "nros::main!" in main.read_text(errors="replace") else "manual"
    for name in ("main.c", "main.cpp", "main.cc"):
        f = src / name
        if f.is_file() and re.search(r"\bint\s+main\s*\(", f.read_text(errors="replace")):
            return "manual"
    return "generated"


def _leaf_launch(leaf: Path) -> str:
    if any(leaf.glob("launch/*.launch.xml")) or any(leaf.glob("launch/*.launch.py")):
        return "authored"
    system = _toml(leaf / "system.toml")
    if any("launch" in img for img in system.get("image", {}).values() if isinstance(img, dict)):
        return "authored"
    return "generated"


def fixture_tuple(root: Path, row: dict) -> tuple:
    lang = row.get("lang", "")
    if row.get("builder") == "west":
        driver = "west"
    elif _is_cargo_row(row):
        driver = "cargo"
    else:
        driver = "cmake"
    leaf = root / row.get("dir", "")
    return (row.get("platform", ""), lang, driver, "single", _leaf_launch(leaf),
            _leaf_entry(leaf, lang))


def workspace_tuple(root: Path, row: dict) -> tuple:
    lang = row.get("lang", "")
    if row.get("builder") == "cmake" or (lang in ("c", "cpp") and not row.get("west_build_name")):
        driver = "cmake"
    elif row.get("west_build_name") or row.get("builder") == "west":
        driver = "west"
    else:
        driver = "cargo" if lang == "rust" else "cmake"
    system = _toml(root / row.get("dir", "") / row.get("bringup", "") / "system.toml")
    images = system.get("image", {}) if isinstance(system.get("image"), dict) else {}
    image = images.get(row.get("image", ""), {}) if isinstance(images, dict) else {}
    shape = "single" if len(images) <= 1 else "multi"
    authored = "launch" in image or "default_launch" in system.get("system", {})
    entry = "manual" if "entry" in image else "generated"
    return (row.get("platform", ""), lang, driver, shape,
            "authored" if authored else "generated", entry)


def tuples(root: Path) -> set:
    doc = _toml(root / MANIFEST)
    out = set()
    for row in doc.get("fixture", []):
        out.add(fixture_tuple(root, row))
    for row in doc.get("workspace_fixture", []):
        out.add(workspace_tuple(root, row))
    return out


def fmt(t: tuple) -> str:
    return " ".join(t)


def read_floor(path: Path) -> set:
    if not path.is_file():
        return set()
    out = set()
    for line in path.read_text().splitlines():
        line = line.split("#", 1)[0].strip()
        if line:
            out.add(tuple(line.split()))
    return out


HEADER = """\
# Phase-535 W1 (RFC-0122 D5) — the example coverage FLOOR.
#
# One line per (platform lang driver shape launch entry) that some fixture row
# exercised when the line was added. `check-example-coverage-ratchet` fails when
# a line here is produced by no row. ADD with
#     python3 scripts/check-example-coverage-ratchet.py --write-floor
# (it never removes). REMOVE a line only by hand, in a commit that says why
# (RFC-0122 D10: a hand-written `main` that is not a manual-entry demo).
#
# platform lang driver shape launch entry
"""


def write_floor(path: Path, current: set) -> int:
    floor = read_floor(path) | current
    path.write_text(HEADER + "".join(fmt(t) + "\n" for t in sorted(floor)))
    return len(floor)


def check(root: Path, floor_path: Path) -> list:
    floor = read_floor(floor_path)
    current = tuples(root)
    return sorted(floor - current)


def self_test() -> int:
    """Negative control: a gate that cannot fail proves nothing."""
    with tempfile.TemporaryDirectory() as td:
        root = Path(td)
        (root / "examples/a/c/talker/src").mkdir(parents=True)
        (root / "examples/a/c/talker/src/main.c").write_text("int main(void) { return 0; }\n")
        (root / "examples/a/c/talker/system.toml").write_text("[image.x]\nboard = \"b\"\n")
        bring = root / "ws/src/pubsub_bringup"
        bring.mkdir(parents=True)
        (bring / "system.toml").write_text(
            "[image.p_talker]\nboard = \"b\"\nlaunch = \"talker.launch.xml\"\n"
            "[image.p_listener]\nboard = \"b\"\nlaunch = \"listener.launch.xml\"\n")
        (root / ".config").mkdir()
        manifest = (
            '[[fixture]]\nplatform = "p"\nlang = "c"\ndir = "examples/a/c/talker"\n\n'
            '[[workspace_fixture]]\nplatform = "p"\nlang = "rust"\ndir = "ws"\n'
            'bringup = "src/pubsub_bringup"\nimage = "p_talker"\n'
        )
        (root / MANIFEST).parent.mkdir(parents=True, exist_ok=True)
        (root / MANIFEST).write_text(manifest)
        want = {
            ("p", "c", "cmake", "single", "generated", "manual"),
            ("p", "rust", "cargo", "multi", "authored", "generated"),
        }
        got = tuples(root)
        if got != want:
            print(f"self-test: derivation wrong:\n  got  {sorted(got)}\n  want {sorted(want)}")
            return 1
        write_floor(root / FLOOR, got)
        if check(root, root / FLOOR):
            print("self-test: a tree equal to its floor was reported short")
            return 1
        # Delete the leaf's row: its tuple is the last of its kind.
        (root / MANIFEST).write_text(manifest[manifest.index("[[workspace_fixture]]"):])
        lost = check(root, root / FLOOR)
        if lost != [("p", "c", "cmake", "single", "generated", "manual")]:
            print(f"self-test: deleting the last manual C leaf was not caught: {lost}")
            return 1
        # --write-floor must never drop the lost line.
        write_floor(root / FLOOR, tuples(root))
        if ("p", "c", "cmake", "single", "generated", "manual") not in read_floor(root / FLOOR):
            print("self-test: --write-floor removed a floor line")
            return 1
    print("check-example-coverage-ratchet self-test: OK (4 cases)")
    return 0


def main() -> int:
    ap = argparse.ArgumentParser(description=(__doc__ or "").splitlines()[0])
    ap.add_argument("--list", action="store_true", help="print the tuples the tree produces")
    ap.add_argument("--write-floor", action="store_true", help="add the tree's tuples to the floor")
    ap.add_argument("--self-test", action="store_true")
    args = ap.parse_args()
    if args.self_test:
        return self_test()
    if args.list:
        for t in sorted(tuples(REPO)):
            print(fmt(t))
        return 0
    rc = self_test()
    if rc:
        return rc
    if args.write_floor:
        n = write_floor(REPO / FLOOR, tuples(REPO))
        print(f"check-example-coverage-ratchet: floor now {n} tuple(s) ({FLOOR})")
        return 0
    if not (REPO / FLOOR).is_file():
        print(f"check-example-coverage-ratchet: {FLOOR} is missing; create it with --write-floor")
        return 1
    lost = check(REPO, REPO / FLOOR)
    if lost:
        print("check-example-coverage-ratchet: FAIL — the floor names coverage no fixture row "
              "produces any more:")
        for t in lost:
            print("    " + fmt(t))
        print("  (platform lang driver shape launch entry). A move must keep at least one row\n"
              "  producing each tuple. If the loss is deliberate (RFC-0122 D10: a hand-written\n"
              f"  `main` that is not a manual-entry demo), delete the line from {FLOOR} in the\n"
              "  same commit and say why.")
        return 1
    print(f"check-example-coverage-ratchet: OK ({len(read_floor(REPO / FLOOR))} floor tuple(s) "
          "all produced)")
    return 0


if __name__ == "__main__":
    sys.exit(main())

#!/usr/bin/env python3
"""Every `[[workspace_fixture]]` row is reached by a platform build recipe.

Issue 2481: the qemu `build-fixtures` recipe called
`workspace-fixtures-build.sh baremetal rust`, so the bare-metal C and C++
workspace rows (phase-535 W6) were built by no lane and every test reading
them reported an unmet precondition. A language filter on a recipe call is a
DECISION that the platform has no rows in the other languages, and nothing
re-asked it when rows were added.

The rule: for each (platform, lang) that has rows, some `just` recipe runs
`workspace-fixtures-build.sh <platform>` with no language (all of them) or
with that language. A call narrowed to one row (`--id`, `NROS_FIXTURE_ID=`)
reaches only that row and does not count. Rows the Zephyr west lane builds
are out of scope: that lane reads the manifest itself, with no language
filter.

Usage:
    check-workspace-fixture-lang-reach.py              # check the tree
    check-workspace-fixture-lang-reach.py --self-test  # negative controls
"""
from __future__ import annotations

import pathlib
import re
import subprocess
import sys

try:
    import tomllib
except ModuleNotFoundError:  # python < 3.11
    import tomli as tomllib

ROOT = pathlib.Path(__file__).resolve().parent.parent
# Mirrors fixtures-manifest.py's ZEPHYR_WEST_PLATFORMS: those rows are built
# by the west lane, not by workspace-fixtures-build.sh.
WEST_PLATFORMS = {"zephyr", "zephyr-cortex-m", "zephyr-nucleo-f767zi"}
CALL = re.compile(r"workspace-fixtures-build\.sh\s+(\S+)(?:\s+(rust|c|cpp|mixed)\b)?")


def row_coords(rows):
    """{(platform, lang)} the build script must reach."""
    out = set()
    for r in rows:
        plat = r.get("platform", "")
        if plat in WEST_PLATFORMS or r.get("builder") == "west":
            continue
        out.add((plat, r.get("lang", "")))
    return out


def recipe_calls(texts):
    """[(file, line, platform, lang-or-None)] of whole-platform calls.

    Comment lines and calls narrowed to one row are skipped. A platform that
    is not a literal word is returned as-is so the caller can refuse it.
    """
    calls = []
    for name, text in texts:
        for n, line in enumerate(text.splitlines(), 1):
            if line.lstrip().startswith("#"):
                continue
            if "--id" in line or "NROS_FIXTURE_ID=" in line:
                continue
            for m in CALL.finditer(line):
                calls.append((name, n, m.group(1), m.group(2)))
    return calls


def unreached(coords, calls):
    """Return (missing coords, refusal lines)."""
    refusals = []
    whole, by_lang = set(), set()
    for name, n, plat, lang in calls:
        if not re.fullmatch(r"[a-z0-9][a-z0-9-]*", plat):
            refusals.append(f"{name}:{n}: platform `{plat}` is not a literal name, "
                            "so which rows it builds cannot be read")
            continue
        if lang is None:
            whole.add(plat)
        else:
            by_lang.add((plat, lang))
    missing = sorted(c for c in coords if c[0] not in whole and c not in by_lang)
    return missing, refusals


def check(root: pathlib.Path) -> int:
    with open(root / "examples/fixtures.toml", "rb") as fh:
        rows = tomllib.load(fh).get("workspace_fixture", [])
    listed = subprocess.run(
        ["git", "-C", str(root), "ls-files", "--", "justfile", "just/*.just"],
        check=True, capture_output=True, text=True).stdout.split()
    texts = [(name, (root / name).read_text()) for name in sorted(listed)]
    coords = row_coords(rows)
    missing, refusals = unreached(coords, recipe_calls(texts))
    for r in refusals:
        print(f"FAIL {r}")
    for plat, lang in missing:
        n = sum(1 for r in rows if r.get("platform") == plat and r.get("lang") == lang)
        print(f"FAIL {n} workspace fixture row(s) for platform `{plat}`, lang `{lang}` "
              "are built by no recipe: no `workspace-fixtures-build.sh "
              f"{plat}` call without a language, and none with `{lang}`.")
    if missing or refusals:
        print("  Fix: drop the language filter from that platform's recipe call, "
              "or add a call for the missing language.")
        return 1
    print(f"workspace-fixture-lang-reach: {len(coords)} platform/lang pair(s) "
          "reached by a recipe")
    return 0


def self_test() -> int:
    rows = [{"platform": "baremetal", "lang": "rust"},
            {"platform": "baremetal", "lang": "c"},
            {"platform": "zephyr", "lang": "c"},
            {"platform": "nuttx", "lang": "cpp", "builder": "west"}]
    coords = row_coords(rows)
    assert coords == {("baremetal", "rust"), ("baremetal", "c")}, coords

    def run(text):
        return unreached(coords, recipe_calls([("t.just", text)]))

    cases = [
        # The shape that shipped: rust only, so the C rows go unbuilt.
        ("rust-only call", "    bash scripts/build/workspace-fixtures-build.sh baremetal rust\n",
         [("baremetal", "c")], 0),
        ("no language reaches all", "    bash scripts/build/workspace-fixtures-build.sh baremetal\n",
         [], 0),
        ("per-language calls", "x workspace-fixtures-build.sh baremetal rust\n"
                               "x workspace-fixtures-build.sh baremetal c\n", [], 0),
        ("--id reaches one row only",
         "x workspace-fixtures-build.sh baremetal rust\n"
         "x workspace-fixtures-build.sh baremetal c --id one\n", [("baremetal", "c")], 0),
        ("NROS_FIXTURE_ID narrows too",
         "NROS_FIXTURE_ID=x workspace-fixtures-build.sh baremetal\n",
         [("baremetal", "c"), ("baremetal", "rust")], 0),
        ("a comment is not a call", "# workspace-fixtures-build.sh baremetal\n",
         [("baremetal", "c"), ("baremetal", "rust")], 0),
        ("variable platform refused", "x workspace-fixtures-build.sh \"$plat\"\n",
         [("baremetal", "c"), ("baremetal", "rust")], 1),
    ]
    for name, text, want_missing, want_refusals in cases:
        missing, refusals = run(text)
        assert missing == want_missing, (name, missing)
        assert len(refusals) == want_refusals, (name, refusals)
    print(f"workspace-fixture-lang-reach self-test: {len(cases)} case(s) OK")
    return 0


def main(argv) -> int:
    if argv[1:] == ["--self-test"]:
        return self_test()
    if argv[1:]:
        print("usage: check-workspace-fixture-lang-reach.py [--self-test]", file=sys.stderr)
        return 2
    self_test()
    return check(ROOT)


if __name__ == "__main__":
    sys.exit(main(sys.argv))

#!/usr/bin/env python3
"""Print the build wiring inventory: roads, carriers, producers, readers.

Why this is a SCRIPT and not a table in the reference doc: every count here
moves. `docs/reference/canonical-build-path.md` explains the four roads and why
their carriers differ — prose that stays true — and defers every NUMBER to this
command. A hand-authored census is the failure mode this repository has
recorded more than any other (CLAUDE.md's "Gated" note on the package list, the
28-vs-88 RMW parity map, the sizes-header family): it reads as current and
answers about the tree someone measured once.

Everything below is derived from the tree at the moment you run it. Nothing is
an authored list except `ROADS`, which is checked against the `Driver` enum by
`check-build-wiring-roads`.

    python3 scripts/nros-build-wiring.py            # the whole inventory
    python3 scripts/nros-build-wiring.py --roads    # just the road table
    python3 scripts/nros-build-wiring.py --scripts  # build.rs by capability
    python3 scripts/nros-build-wiring.py --json
"""

from __future__ import annotations

import argparse
import json
import re
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent

# The one authored structure, and the reason it is authored: a road's CARRIER
# is a design fact, not something a grep can read off the tree. The road NAMES
# are checked against `Driver` (plan.rs) by `check-build-wiring-roads`, so this
# cannot silently miss a road someone adds.
ROADS = {
    "Cargo": {
        "exec": "cargo build",
        "carrier": "`[env]` rows in the generated `build/<coord>/nros-cargo.toml`, read via `--config`",
        "emits_root": True,
        "hazard": "issue 0491 — a PATH-valued row has three spellings, so watch the CONTENT, never `rerun-if-env-changed`",
    },
    "CMake": {
        "exec": "cmake --build",
        "carrier": "`corrosion_set_env_vars()` on the target's own cargo command",
        "emits_root": True,
        "hazard": "issue 0460 — `set(ENV{})` touches only the configure process, so it carries NOTHING to a cargo lane",
    },
    "West": {
        "exec": "west build -b <board>",
        "carrier": "`$DOTCONFIG`, read by each build script through `nros_zephyr_build`",
        "emits_root": False,
        "hazard": "issue 0460 — zephyr-lang-rust builds its own cargo command and inherits no environment at all",
    },
    "IdfPy": {
        "exec": "idf.py build",
        "carrier": "(the ESP-IDF port was retired in phase-468 W2; the driver survives for a user-owned IDF project)",
        "emits_root": False,
        "hazard": "not exercised in-tree",
    },
}

CAPABILITIES = [
    ("C", "compiles C", re.compile(r"cc::Build")),
    ("K", "reads Kconfig", re.compile(r"nros_zephyr_build::|dotconfig")),
    ("E", "watches env knobs", re.compile(r"rerun-if-env-changed")),
    ("B", "routes via nros-board-common", re.compile(r"nros_board_common")),
    ("P", "re-roots inherited paths", re.compile(r"nros_build_paths")),
    ("G", "emits cfg", re.compile(r"cargo:{1,2}rustc-cfg")),
    ("A", "generates ABI bindings", re.compile(r"bindgen")),
    ("D", "reads the sizing descriptor", re.compile(r"nros_sizing_descriptor")),
]


def tracked(*patterns: str) -> list[str]:
    out = subprocess.run(
        ["git", "ls-files", *patterns],
        cwd=ROOT, capture_output=True, text=True, check=True,
    ).stdout.split()
    return sorted(out)


def read(rel: str) -> str:
    try:
        return (ROOT / rel).read_text(errors="replace")
    except OSError:
        return ""


def build_scripts() -> list[tuple[str, str, str]]:
    """(category, path, capability letters) for every tracked build.rs."""
    rows = []
    for p in tracked("*/build.rs"):
        text = read(p)
        caps = "".join(letter for letter, _, rx in CAPABILITIES if rx.search(text))
        parts = p.split("/")
        if parts[0] == "packages":
            category = parts[1]
        elif parts[0] == "examples":
            category = "examples"
        else:
            category = parts[0]
        rows.append((category, p, caps))
    return sorted(rows)


def knob_sources() -> dict[str, int]:
    kconfig = 0
    for f in ("zephyr/Kconfig", "packages/rmw/zenoh/zpico-zephyr/Kconfig"):
        kconfig += len(re.findall(r"^config [A-Z0-9_]+", read(f), re.M))
    cargo_build = read("zephyr/cmake/nros_cargo_build.cmake")
    return {
        "nros-platform.toml (RFC-0049 platform rung)": len(tracked("*nros-platform.toml")),
        "nros-board.toml (RFC-0064 board rung)": len(tracked("*nros-board.toml")),
        "nros-rmw.toml (RFC-0071 backend descriptor)": len(tracked("*nros-rmw.toml")),
        "Kconfig symbols (the Zephyr rung)": kconfig,
        "cmake _nros_resolve_knob() forwards": len(re.findall(r"_nros_resolve_knob\(", cargo_build)),
        "system.toml (the leaf's declaration)": len(tracked("*system.toml")),
        "system.contract.yaml (declared QoS)": len(tracked("*system.contract.yaml")),
    }


def readers() -> dict[str, int]:
    """Crates reaching each shared resolution surface."""
    surfaces = [
        "nros_platform_config",
        "nros_board_common",
        "nros_zephyr_build",
        "nros_sizing_descriptor",
        "nros_build_paths",
    ]
    out = {}
    for s in surfaces:
        r = subprocess.run(
            ["git", "grep", "-l", s, "--", "*.rs"],
            cwd=ROOT, capture_output=True, text=True,
        )
        out[s] = len(r.stdout.split()) if r.returncode == 0 else 0
    return out


def inventory() -> dict:
    scripts = build_scripts()
    by_category: dict[str, int] = {}
    by_capability: dict[str, int] = {}
    for category, _path, caps in scripts:
        by_category[category] = by_category.get(category, 0) + 1
        for letter in caps:
            by_capability[letter] = by_capability.get(letter, 0) + 1
    return {
        "roads": ROADS,
        "build_scripts": {
            "total": len(scripts),
            "by_category": dict(sorted(by_category.items(), key=lambda kv: -kv[1])),
            "by_capability": {
                letter: {"label": label, "count": by_capability.get(letter, 0)}
                for letter, label, _ in CAPABILITIES
            },
            "rows": [{"category": c, "path": p, "capabilities": caps} for c, p, caps in scripts],
        },
        "knob_sources": knob_sources(),
        "readers": readers(),
    }


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__,
                                 formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--roads", action="store_true", help="only the road table")
    ap.add_argument("--scripts", action="store_true", help="only the build.rs census")
    ap.add_argument("--json", action="store_true", help="machine-readable")
    args = ap.parse_args()

    inv = inventory()
    if args.json:
        print(json.dumps(inv, indent=2, sort_keys=True))
        return 0

    want_all = not (args.roads or args.scripts)

    if args.roads or want_all:
        print("BUILD ROADS — how a knob reaches the compiler on each")
        print("  Four roads, four DIFFERENT carriers. That is the finding, not an")
        print("  accident: each carrier has produced its own delivery defect.")
        for name, r in ROADS.items():
            print(f"\n  {name}  ({r['exec']})")
            print(f"    stage 4 emits a root: {'yes' if r['emits_root'] else 'no'}")
            print(f"    carrier: {r['carrier']}")
            print(f"    hazard:  {r['hazard']}")
        print()

    if args.scripts or want_all:
        bs = inv["build_scripts"]
        print(f"BUILD SCRIPTS — {bs['total']} tracked `build.rs`")
        print("  by category:")
        for k, v in bs["by_category"].items():
            print(f"    {v:>4}  {k}")
        print("  by capability (a script may have several):")
        for letter, meta in bs["by_capability"].items():
            print(f"    {meta['count']:>4}  {letter}  {meta['label']}")
        print()

    if want_all:
        print("KNOB SOURCES — what can state a value")
        for k, v in inv["knob_sources"].items():
            print(f"    {v:>4}  {k}")
        print()
        print("SHARED RESOLUTION SURFACES — files reaching each")
        for k, v in inv["readers"].items():
            print(f"    {v:>4}  {k}")
        print()
        print("  Ladder order is RFC-0049's: builtin < platform < board < env.")
        print("  The map and the rationale: docs/reference/canonical-build-path.md")

    return 0


if __name__ == "__main__":
    sys.exit(main())

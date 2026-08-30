#!/usr/bin/env python3
"""phase-400 W4 — answer "what did THIS build actually compile, and who required it?"
from the build's own artifacts.

WHY THIS EXISTS

Three phase-400 estimates were computed from the WORKSPACE graph (`cargo tree`,
subtree differences) and then assumed to describe a leaf's build. All three were
wrong, and all three were optimistic:

    orchestration share    31.9 %     ->  12.6 %
    W2 orchestration gate  43 crates  ->  6 crates
    W3 bindgen             20.6 s     ->  0 s from our crates

A subtree difference is an upper bound on what COULD leave a graph. It is not a
measurement of what DOES. The leaves that matter here often cannot even be
resolved standalone (`zephyr-build` comes from the west environment), so
`cargo tree` in the workspace is answering a different question than the one
being asked.

WHAT THIS READS

`<target-dir>/**/.fingerprint/<crate>-<hash>/<kind>-<name>.json`, which cargo
writes for every unit it actually built. Each carries a `deps` array whose
entries name the dependency units. That is ground truth: not a re-resolution,
not a guess about features or platforms, but the edges of the build that ran.

HOST vs TARGET matters and is kept separate. A cross build has two graphs in one
target dir — build-dependencies compiled for the host under `<profile>/`, and the
real thing under `<triple>/<profile>/`. Conflating them is how a host-only tool
gets counted against firmware.

USAGE

    scripts/nros-leaf-graph.py <target-dir>                 # crate -> requirers
    scripts/nros-leaf-graph.py <target-dir> --exclusive-to X # what leaves with X
    scripts/nros-leaf-graph.py <target-dir> --json
"""

import argparse
import collections
import json
import pathlib
import sys


def load_units(target_dir: pathlib.Path):
    """Every unit cargo built, as (side, crate, requires) triples.

    `side` is "host" or "target": the fingerprint tree directly under the target
    dir is the host side, anything under `<triple>/<profile>/` is the target.
    """
    units = []
    # Bounded globs, NOT `**`: a populated target dir holds hundreds of thousands
    # of files and `**` walks all of them. Cargo puts `.fingerprint` at exactly
    # these two depths — `<profile>/` and `<triple>/<profile>/` — so ask for those.
    fp_dirs = list(target_dir.glob("*/.fingerprint")) + list(target_dir.glob("*/*/.fingerprint"))
    if (target_dir / ".fingerprint").is_dir():   # pointed straight at a profile dir
        fp_dirs.append(target_dir / ".fingerprint")
    for fp_dir in fp_dirs:
        rel = fp_dir.relative_to(target_dir).parts[:-1]  # drop ".fingerprint"
        # `<profile>/` is the host side; `<triple>/<profile>/` is the target side.
        # Keyed on "a component looks like a target triple" rather than on depth,
        # so the tool works whether it is pointed at `target/` or `target/release`
        # — being wrong about which side a unit is on is the exact confusion this
        # script exists to prevent.
        side = "target" if any("-" in c for c in rel) else "host"
        for unit_dir in fp_dir.iterdir():
            if not unit_dir.is_dir():
                continue
            crate = unit_dir.name.rsplit("-", 1)[0]
            for j in unit_dir.glob("*.json"):
                # `lib-foo.json` / `bin-foo.json` / `build-script-build.json` ...
                try:
                    data = json.loads(j.read_text())
                except (OSError, ValueError):
                    continue
                if not isinstance(data, dict):
                    continue
                requires = {d[1] for d in data.get("deps", []) if len(d) > 1}
                # A crate's own build script is an internal edge, not a dependency.
                requires.discard("build_script_build")
                units.append((side, crate, requires))
    return units


def requirer_map(units):
    """crate -> set of crates that required it, per side.

    Names are normalised to cargo's underscore spelling on BOTH ends, because a
    `deps` entry is the lib name (`nros_rmw`) while the unit dir is the package
    name (`nros-rmw`). Comparing the two spellings without normalising is its own
    small version of the mistake this script exists to prevent.
    """
    out = {"host": collections.defaultdict(set), "target": collections.defaultdict(set)}
    for side, crate, requires in units:
        norm = crate.replace("-", "_")
        out[side].setdefault(norm, set())
        for dep in requires:
            out[side][dep.replace("-", "_")].add(norm)
    return out


def exclusive_to(reqmap, roots):
    """Crates reachable ONLY through `roots` — i.e. what actually leaves if roots go.

    Computed as a fixpoint over the requirer map: drop the roots, then repeatedly
    drop anything whose every requirer has already been dropped. This is the
    calculation the failed estimates approximated by eye.
    """
    roots = {r.replace("-", "_") for r in roots}
    dropped = set(roots)
    changed = True
    while changed:
        changed = False
        for crate, requirers in reqmap.items():
            if crate in dropped or not requirers:
                continue  # no requirers = a root of the build; never dropped
            if requirers <= dropped:
                dropped.add(crate)
                changed = True
    return dropped - roots


def self_test() -> int:
    """Prove the fixpoint on a graph whose right answer is known by hand.

    The repo's gate-selftest convention (see `gen-issue-index.py --self-test`):
    a tool whose whole job is to correct a bad estimate is worth checking against
    a case where the bad estimate is the tempting one.

        app -> macros -> ir -> model -> yaml -> zerocopy
        app -> cbindgen -> serde
        ir  -> serde                       (serde is CONTESTED)

    Dropping `macros` must NOT drop `serde` — that is exactly the shape of the
    W2 error, where a subtree difference claimed a contested crate would leave.
    """
    units = [
        ("host", "app", {"macros", "cbindgen"}),
        ("host", "macros", {"ir"}),
        ("host", "ir", {"model", "serde"}),
        ("host", "model", {"yaml"}),
        ("host", "yaml", {"zerocopy"}),
        ("host", "zerocopy", set()),
        ("host", "cbindgen", {"serde"}),
        ("host", "serde", set()),
    ]
    m = requirer_map(units)["host"]
    failures = []

    got = exclusive_to(m, ["macros"])
    want = {"ir", "model", "yaml", "zerocopy"}
    if got != want:
        failures.append(f"drop macros: got {sorted(got)}, want {sorted(want)}")
    if "serde" in got:
        failures.append("serde left with macros, but cbindgen still requires it (the W2 error)")

    # Dropping BOTH requirers must free the contested crate — the pairing effect.
    got_pair = exclusive_to(m, ["macros", "cbindgen"])
    if "serde" not in got_pair:
        failures.append("serde survived after BOTH its requirers went")

    # A crate with no requirers is a build root and is never dropped.
    if "app" in exclusive_to(m, ["macros"]):
        failures.append("build root was dropped")

    # Name normalisation: `deps` gives lib names, unit dirs give package names.
    m2 = requirer_map([("host", "nros-macros", {"nros_pkg_index"}),
                       ("host", "nros-pkg-index", set())])["host"]
    if m2.get("nros_pkg_index") != {"nros_macros"}:
        failures.append(f"name normalisation: {dict(m2)}")

    for f in failures:
        print(f"[FAIL] {f}", file=sys.stderr)
    if failures:
        return 1
    print("nros-leaf-graph --self-test: OK (4 checks, incl. the contested-crate case)")
    return 0


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("target_dir", type=pathlib.Path, nargs="?")
    ap.add_argument("--self-test", action="store_true",
                    help="check the fixpoint against a hand-computed graph")
    ap.add_argument("--side", choices=["host", "target", "both"], default="both")
    ap.add_argument("--exclusive-to", metavar="CRATE", action="append", default=[],
                    help="report what leaves the build if these crates go (repeatable)")
    ap.add_argument("--json", action="store_true")
    args = ap.parse_args()

    if args.self_test:
        return self_test()
    if args.target_dir is None:
        ap.error("target_dir is required (or pass --self-test)")
    if not args.target_dir.is_dir():
        print(f"[FAIL] no such target dir: {args.target_dir}", file=sys.stderr)
        return 2

    units = load_units(args.target_dir)
    if not units:
        print(f"[FAIL] no .fingerprint units under {args.target_dir} — was anything built there?",
              file=sys.stderr)
        return 2
    reqmap = requirer_map(units)

    sides = ["host", "target"] if args.side == "both" else [args.side]
    result = {}
    for side in sides:
        m = reqmap[side]
        if not m:
            continue
        entry = {"units": len(m)}
        if args.exclusive_to:
            leaving = exclusive_to(m, args.exclusive_to)
            entry["exclusive_to"] = sorted(args.exclusive_to)
            entry["would_leave"] = sorted(leaving)
        else:
            entry["requirers"] = {k: sorted(v) for k, v in sorted(m.items())}
        result[side] = entry

    if args.json:
        print(json.dumps(result, indent=2))
        return 0

    for side, entry in result.items():
        print(f"=== {side} side — {entry['units']} crate(s) actually built")
        if "would_leave" in entry:
            leaving = entry["would_leave"]
            print(f"    removing {', '.join(entry['exclusive_to'])} would drop "
                  f"{len(leaving)} further crate(s):")
            for c in leaving:
                print(f"      {c}")
            if not leaving:
                print("      (none — everything it reaches has another requirer)")
        else:
            for crate, reqs in entry["requirers"].items():
                who = ", ".join(reqs) if reqs else "(build root)"
                print(f"  {crate:40s} <- {who}")
        print()
    return 0


if __name__ == "__main__":
    sys.exit(main())

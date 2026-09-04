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


# A `.fingerprint/` directory ACCUMULATES: units from configurations the tree no
# longer builds are never removed. So "every unit in the tree" is a HISTORICAL
# record, not a description of the current build, and reading it as the latter
# is how this tool reported a dependency that no longer exists.
#
# Measured 2026-08-30 on a freshly built Zephyr Rust leaf: TWO `nros-zpico-build`
# lib units side by side —
#
#   nros-zpico-build-aeea…  mtime 08-29 23:02  feats=[]          deps: cbindgen, cc, …
#   nros-zpico-build-f75f…  mtime 08-30 22:21  feats=["default"] deps: cc, …
#
# — and the tool reported `cbindgen <- nros_zpico_build` from the day-old one,
# for a dependency phase-400 W2a had already removed. The `--exclusive-to` answer
# computed over that graph was wrong in the direction that matters: it understated
# what would leave.
#
# The window has to exceed a single build's wall time, not a session's. There is
# NO exact signal here — a unit that is live but FRESH is not rewritten, so on an
# INCREMENTALLY built tree this drops units that are still real. That is why the
# exclusion is REPORTED rather than silent, and why the honest procedure is to
# build the leaf, then measure it. (`invoked.timestamp` does not help: it marks
# build scripts that RE-RAN, measured at 9 of 64 in a fresh tree with zero
# overlap against the units holding records.)
RECENT_WINDOW_SECS = 6 * 60 * 60


def load_units(target_dir: pathlib.Path, all_units: bool = False, schema: dict = None):
    """Every unit cargo built, as (side, crate, requires) triples.

    `side` is "host" or "target", read from cargo's own `compile_kind`.

    By default only units from the tree's most recent build are returned; pass
    `all_units` to include stale ones, which is a historical view and must not be
    quoted as a property of the current build.

    `schema`, when given, is filled with what the PARSE actually found:
    `records` (json objects read), `with_deps` and `with_compile_kind` (how many
    carried each key this tool reads). Issue 0945 item 3 — cargo's `.fingerprint`
    format carries no stability guarantee, and every key here is read with
    `.get(..., default)`, so a rename does not raise: `deps` gone means every
    crate looks like a build root and `--exclusive-to` answers "nothing leaves",
    which is plausible, optimistic and wrong in exactly the direction the failed
    estimates in this file's header were wrong. The caller refuses rather than
    printing it.
    """
    if schema is not None:
        schema.setdefault("records", 0)
        schema.setdefault("with_deps", 0)
        schema.setdefault("with_compile_kind", 0)
    units = []
    stale = 0
    # Bounded globs, NOT `**`: a populated target dir holds hundreds of thousands
    # of files and `**` walks all of them. Cargo puts `.fingerprint` at exactly
    # these two depths — `<profile>/` and `<triple>/<profile>/` — so ask for those.
    fp_dirs = list(target_dir.glob("*/.fingerprint")) + list(target_dir.glob("*/*/.fingerprint"))
    if (target_dir / ".fingerprint").is_dir():   # pointed straight at a profile dir
        fp_dirs.append(target_dir / ".fingerprint")
    for fp_dir in fp_dirs:
        # Side comes from cargo's own `compile_kind` (0 = host, else a target
        # hash), read per unit below — NOT from the path.
        #
        # An earlier version guessed "a path component containing a hyphen is a
        # target triple". Profile names contain hyphens too (`nros-relwithdebinfo`,
        # `dev-release`), so every unit of a Zephyr build was labelled "target" and
        # the host overlap it was asked to measure came back as zero. Caught by
        # using the tool, which is the only reason it was caught at all.
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
                if schema is not None:
                    schema["records"] += 1
                    schema["with_deps"] += "deps" in data
                    schema["with_compile_kind"] += "compile_kind" in data
                side = "host" if data.get("compile_kind") in (0, None) else "target"
                requires = {d[1] for d in data.get("deps", []) if len(d) > 1}
                # A crate's own build script is an internal edge, not a dependency.
                requires.discard("build_script_build")
                try:
                    mtime = j.stat().st_mtime
                except OSError:
                    mtime = 0.0
                units.append((side, crate, requires, mtime))
    if not units:
        return []
    newest = max(u[3] for u in units)
    if not all_units:
        keep = [u for u in units if newest - u[3] <= RECENT_WINDOW_SECS]
        stale = len(units) - len(keep)
        if stale:
            print(
                f"note: ignored {stale} unit record(s) older than "
                f"{RECENT_WINDOW_SECS // 3600} h — a .fingerprint dir accumulates, and a\n"
                f"      stale unit reports dependencies this build does not have. "
                f"Pass --all-units for\n      the historical view; build the leaf "
                f"first if you are about to quote a number.",
                file=sys.stderr,
            )
        units = keep
    return [(s, c, r) for s, c, r, _ in units]


def schema_complaint(schema):
    """The reason this parse cannot be trusted, or None.

    Issue 0945 item 3. Both keys are present in EVERY record cargo writes today —
    measured across `lib-*`, `build-script-build*` and `run-build-script-*` in a
    populated tree, all carrying the identical key set — so "records were read
    and none carried this key" is a rename, not a legitimate shape.

    This is the honest version of the mitigation issue 0945 claimed for these
    tools. `--self-test` did NOT cover it: it feeds `requirer_map` a hand-written
    `units` list and never touches the parser, so a schema change left the
    self-test green and the answer wrong.
    """
    if not schema or not schema.get("records"):
        return None
    if not schema["with_deps"]:
        return (f"{schema['records']} fingerprint record(s) read and NOT ONE carried a "
                "`deps` key.\n"
                "      cargo's .fingerprint format is private and undocumented (issue 0945 "
                "item 3);\n"
                "      a rename here does not raise — every crate would read as a build root "
                "and\n"
                "      `--exclusive-to` would answer \"nothing leaves\". That answer is "
                "plausible,\n"
                "      optimistic, and wrong, which is the failure this tool exists to "
                "prevent.")
    if not schema["with_compile_kind"]:
        return (f"{schema['records']} fingerprint record(s) read and NOT ONE carried a "
                "`compile_kind` key.\n"
                "      Host and target units would all be labelled \"host\" and the two "
                "graphs conflated —\n"
                "      the same class of error as the path-based guess this tool replaced.")
    return None


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


def self_test(quiet: bool = False) -> int:
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

    # Issue 0945 item 3 — the PARSER, not just the fixpoint.
    #
    # Everything above feeds `requirer_map` a hand-written `units` list, so it
    # cannot see a `.fingerprint` schema change at all. That is why 0945's claim
    # that "--self-test surfaces a schema change" was false for this tool. These
    # two cases read a real on-disk tree and then MUTATE the key cargo owns: the
    # healthy shape must be reported, and the renamed shape must be REFUSED
    # rather than answered.
    import contextlib
    import io
    import tempfile

    def _tree(root, keyname="deps", kindkey="compile_kind"):
        d = pathlib.Path(root) / "trip" / "prof" / ".fingerprint" / "app-0a605463647b4af3"
        d.mkdir(parents=True, exist_ok=True)
        (d / "lib-app.json").write_text(json.dumps({
            kindkey: 0, keyname: [[1, "serde", False, 2]],
        }))
        e = pathlib.Path(root) / "trip" / "prof" / ".fingerprint" / "serde-0b605463647b4af3"
        e.mkdir(parents=True, exist_ok=True)
        (e / "lib-serde.json").write_text(json.dumps({kindkey: 0, keyname: []}))
        return pathlib.Path(root)

    with tempfile.TemporaryDirectory() as root, \
            contextlib.redirect_stdout(io.StringIO()):
        rc = main([str(_tree(root))], run_selftest=False)
        if rc != 0:
            failures.append(f"healthy fingerprint tree was refused (rc={rc})")
    with tempfile.TemporaryDirectory() as root, \
            contextlib.redirect_stderr(io.StringIO()) as refusal:
        rc = main([str(_tree(root, keyname="dependencies"))], run_selftest=False)
        if "deps" not in refusal.getvalue():
            failures.append("the refusal does not name the key that went missing")
        if rc != 2:
            failures.append(
                f"`deps` renamed and the tool still answered (rc={rc}) — a schema "
                "change must be INCONCLUSIVE, not a plausible empty graph")
    with tempfile.TemporaryDirectory() as root, \
            contextlib.redirect_stderr(io.StringIO()):
        rc = main([str(_tree(root, kindkey="kind"))], run_selftest=False)
        if rc != 2:
            failures.append(
                f"`compile_kind` renamed and the tool still answered (rc={rc}) — host "
                "and target would be conflated")

    for f in failures:
        print(f"[FAIL] {f}", file=sys.stderr)
    if failures:
        return 1
    if not quiet:
        print("nros-leaf-graph --self-test: OK (7 checks — 4 fixpoint, 3 fingerprint-schema)")
    return 0


def main(argv=None, run_selftest=True) -> int:
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("target_dir", type=pathlib.Path, nargs="?")
    ap.add_argument("--self-test", action="store_true",
                    help="check the fixpoint against a hand-computed graph")
    ap.add_argument("--side", choices=["host", "target", "both"], default="both")
    ap.add_argument("--exclusive-to", metavar="CRATE", action="append", default=[],
                    help="report what leaves the build if these crates go (repeatable)")
    ap.add_argument("--json", action="store_true")
    ap.add_argument("--all-units", action="store_true",
                    help="include stale fingerprint records (historical view; "
                         "they report dependencies the current build does not have)")
    args = ap.parse_args(argv)

    if args.self_test:
        return self_test()
    if args.target_dir is None:
        ap.error("target_dir is required (or pass --self-test)")
    if not args.target_dir.is_dir():
        print(f"[FAIL] no such target dir: {args.target_dir}", file=sys.stderr)
        return 2

    # The negative control runs on the NORMAL path, not only behind a flag
    # (`check-gate-selftests`' rule, and the right one here): what it proves is
    # that the schema refusal below is still wired, and the moment that matters
    # is the moment someone is about to quote a number out of this tool. Issue
    # 0945 item 3 — the previous shape ran only under `--self-test`, which
    # nothing invoked. ~10 ms, in-process.
    if run_selftest and self_test(quiet=True) != 0:
        print("[FAIL] nros-leaf-graph: own selftest failed — not reporting a graph.",
              file=sys.stderr)
        return 1

    schema = {}
    units = load_units(args.target_dir, all_units=args.all_units, schema=schema)
    if not units:
        print(f"[FAIL] no .fingerprint units under {args.target_dir} — was anything built there?",
              file=sys.stderr)
        return 2
    complaint = schema_complaint(schema)
    if complaint:
        print(f"[INCONCLUSIVE] {complaint}\n"
              "      Nothing is reported, because a wrong number here is worse than none.",
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

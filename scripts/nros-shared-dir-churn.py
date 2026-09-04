#!/usr/bin/env python3
"""phase-400 W5 — is a set of cargo target dirs SAFE to collapse into one?

WHY THIS EXISTS

W5 wants ~89 Zephyr west build trees to stop each recompiling the same ~86 host
crates. The rule that makes that safe is not "they look similar":

    SHARE ONLY WHAT IS PROVABLY IDENTICAL ACROSS THE BUILDS SHARING IT;
    SEPARATE EVERYTHING ELSE.

A shared `--target-dir` is one fingerprint namespace. Cargo hashes what it owns
(features, profile, target, deps) into unit filenames, so those are
identical-or-distinct by construction. What it does NOT hash is the build-script
ENVIRONMENT and the set of files a build script asked to watch. Those are
recorded per unit in

    <target-dir>/**/.fingerprint/<pkg>-<hash>/run-build-script-build-script-build.json

as `RerunIfEnvChanged {var, val}` and `RerunIfChanged {paths}`. This tool reads
those records from builds that ALREADY RAN and asks the only question that
matters: does the SAME unit record a DIFFERENT value, or a different watched-path
set, in two trees that would share a directory?

Two failure modes, and they are not the same severity:

  * ENV DIVERGENCE is churn. Cargo compares the recorded value as text, sees a
    difference, and re-runs the script — correct, but the unit and everything
    downstream rebuilds on every alternation. This is what issue 0805 measured
    when `CORROSION_BUILD_DIR` stayed exempt after leaves began sharing: 459 s of
    cargo per warm rebuild against 6.7 s once fixed.
  * PATH-SET DIVERGENCE is a CORRECTNESS hazard, and it is quieter. Cargo decides
    freshness from the RECORDED path list, because it cannot know the new list
    without running the script. If tree A recorded 21 paths and tree B would have
    watched 132, a shared dir holding A's record leaves B's other 111 inputs
    unwatched — edit one and nothing re-runs. Sharing does not create an
    under-watching build script; it promotes one tree's under-watch to the whole
    cluster.

WHY THIS IS MEASURED AND NOT REASONED

The blocker this tool was written to investigate did not exist. `DOTCONFIG` was
recorded as a W5 blocker on the reading that Zephyr build scripts fingerprint it
and its value is a per-tree path. Measured: 654 records across 41 C/C++ trees,
every one UNSET — the C lane bakes the knobs into `cmake -E env` and never passes
the file, so the fallback that reads it is never reached. It is set on all 526
records of the 18 Rust trees, which are separate workspace roots and share
nothing (issue 0616). The blocker was an artifact of reading `knob_usize` instead
of reading a build.

Run:
    python3 scripts/nros-shared-dir-churn.py <dir>...      # trees to compare
    python3 scripts/nros-shared-dir-churn.py --self-test
"""

import collections
import glob
import json
import os
import sys

RUN = "run-build-script-build-script-build.json"


# A `.fingerprint/` directory ACCUMULATES. Units from configurations the tree no
# longer builds are never removed, so a tree carries records from every shape it
# has ever had — measured spanning 08-15 to 08-30 in one tree.
#
# Comparing those across trees compares museum records: two trees can "diverge"
# on a unit neither builds any more. The first version of this tool did exactly
# that and reported 15 path divergences, one of which survived TWO rebuilds of
# the tree that was supposed to fix it — because that unit is not in the current
# configuration and nothing rebuilt it.
#
# There is no reliable filesystem signal for "live". `invoked.timestamp` marks
# build scripts that RE-RAN, not units that participated: measured at 9 of 64 in
# a freshly built tree, with zero overlap against the units holding records. A
# unit that is live but FRESH is indistinguishable from an orphan by mtime, so
# any timestamp rule either drops live units or keeps orphans.
#
# So the tool does not guess. It reports the AGE SPREAD of the evidence and
# refuses to certify a comparison whose records come from different build eras —
# the answer to "are these trees consistent?" is only meaningful when they were
# built from the same source state. Build the cluster, then measure.
CONTEMPORARY_SECS = 6 * 60 * 60


def newest_record(tree):
    """mtime of the most recent build-script record in `tree`, or None."""
    best = None
    for pat in (os.path.join(tree, "*", "*", ".fingerprint", "*", RUN),
                os.path.join(tree, "*", "*", "*", ".fingerprint", "*", RUN)):
        for f in glob.glob(pat):
            try:
                m = os.path.getmtime(f)
            except OSError:
                continue
            best = m if best is None else max(best, m)
    return best


def records(tree, schema=None):
    """{unit: (env {var: val}, paths frozenset)} for one build tree.

    Only units live in the tree's LAST build (see `_live_units`); a stale orphan
    is not evidence about a directory anyone would share today.

    Bounded globs, not `**`: cargo puts `.fingerprint` at a known depth and a
    recursive walk over a west build tree reads hundreds of thousands of files
    (the mistake `nros-leaf-graph.py` made and fixed by measurement).
    """
    out = {}
    pats = [
        os.path.join(tree, "*", "*", ".fingerprint", "*", RUN),
        os.path.join(tree, "*", "*", "*", ".fingerprint", "*", RUN),
    ]
    for pat in pats:
        for f in glob.glob(pat):
            unit = os.path.basename(os.path.dirname(f))
            try:
                with open(f, encoding="utf-8") as fh:
                    local = json.load(fh).get("local", [])
            except (OSError, ValueError):
                continue
            env, paths = {}, set()
            for e in local:
                if d := e.get("RerunIfEnvChanged"):
                    env[d["var"]] = d.get("val")
                if p := e.get("RerunIfChanged"):
                    paths |= set(p.get("paths", []))
            if schema is not None:
                schema["records"] = schema.get("records", 0) + 1
                if env or paths:
                    schema["with_entries"] = schema.get("with_entries", 0) + 1
            out[unit] = (env, frozenset(paths))
    return out


def compare(trees):
    """[(kind, unit, detail)] — every divergence between units common to 2+ trees."""
    seen = {t: records(t) for t in trees}
    units = collections.Counter(u for r in seen.values() for u in r)
    findings = []
    for unit, n in units.items():
        if n < 2:
            continue  # not shared, so it cannot diverge
        holders = {t: r[unit] for t, r in seen.items() if unit in r}
        for var in {v for env, _ in holders.values() for v in env}:
            vals = {t: env.get(var) for t, (env, _) in holders.items() if var in env}
            if len(set(vals.values())) > 1:
                findings.append(("env", unit, f"{var}: {sorted(set(map(repr, vals.values())))}"))
        sets = {t: p for t, (_, p) in holders.items()}
        if len(set(sets.values())) > 1:
            sizes = sorted({len(p) for p in sets.values()})
            smallest = min(sets.values(), key=len)
            largest = max(sets.values(), key=len)
            shape = "SUBSET" if smallest <= largest else "DISJOINT"
            findings.append(("paths", unit, f"{sizes} paths, smallest is a {shape} of largest"))
    return findings


def self_test(quiet=False):
    """The two divergences, and the case that must NOT be reported.

    Encoded as a test because the interesting predicate is "same unit, different
    record" and it is easy to write one that flags "different units in one tree"
    instead — which is normal (feature variants) and would make the tool cry wolf
    on every build.
    """
    import contextlib
    import io
    import tempfile

    def write(root, tree, unit, env, paths, envkey="RerunIfEnvChanged",
             pathkey="RerunIfChanged"):
        d = os.path.join(root, tree, "trip", "prof", ".fingerprint", unit)
        os.makedirs(d, exist_ok=True)
        local = [{envkey: {"var": k, "val": v}} for k, v in env.items()]
        local.append({pathkey: {"output": "x", "paths": list(paths)}})
        with open(os.path.join(d, RUN), "w", encoding="utf-8") as fh:
            json.dump({"local": local}, fh)

    with tempfile.TemporaryDirectory() as root:
        # same unit, same everything -> silent
        write(root, "a", "pkg-1", {"K": "4"}, ["/s/x.rs"])
        write(root, "b", "pkg-1", {"K": "4"}, ["/s/x.rs"])
        # same unit, env differs -> churn
        write(root, "a", "pkg-2", {"K": "4"}, ["/s/x.rs"])
        write(root, "b", "pkg-2", {"K": None}, ["/s/x.rs"])
        # same unit, watched set differs -> correctness hazard
        write(root, "a", "pkg-3", {"K": "4"}, ["/s/x.rs", "/s/y.rs"])
        write(root, "b", "pkg-3", {"K": "4"}, ["/s/x.rs"])
        # DIFFERENT units in one tree disagreeing is NORMAL (feature variants)
        write(root, "a", "pkg-4", {"K": "4"}, ["/s/x.rs"])
        write(root, "a", "pkg-5", {"K": None}, ["/s/z.rs"])
        got = compare([os.path.join(root, "a"), os.path.join(root, "b")])

    kinds = collections.Counter(k for k, _, _ in got)
    units = {u for _, u, _ in got}
    assert kinds["env"] == 1, f"expected 1 env divergence, got {kinds['env']}: {got}"
    assert kinds["paths"] == 1, f"expected 1 path divergence, got {kinds['paths']}: {got}"
    assert units == {"pkg-2", "pkg-3"}, f"wrong units flagged: {units}"
    assert "pkg-1" not in units, "an identical unit was flagged"
    assert "pkg-4" not in units and "pkg-5" not in units, (
        "two DIFFERENT units in one tree were compared against each other — "
        "feature variants are normal and must never be reported"
    )

    # The vacuity guard: two trees sharing NO unit must not read as a pass.
    with tempfile.TemporaryDirectory() as root:
        write(root, "a", "only-in-a", {"K": "4"}, ["/s/x.rs"])
        write(root, "b", "only-in-b", {"K": "4"}, ["/s/x.rs"])
        with contextlib.redirect_stderr(io.StringIO()):
            rc = main([os.path.join(root, "a"), os.path.join(root, "b")], run_selftest=False)
    assert rc == 2, f"comparing zero shared units must be INCONCLUSIVE, got rc={rc}"

    # Issue 0945 item 3 — the SCHEMA guard. Two trees whose records are identical
    # and readable must certify; rename the keys cargo owns and the same two trees
    # must become INCONCLUSIVE rather than "safe to collapse". Without this the
    # self-test writes the same names it reads and is blind to a cargo rename,
    # which is the mitigation 0945 credited these tools with and they did not have.
    with tempfile.TemporaryDirectory() as root:
        write(root, "a", "pkg-1", {"K": "4"}, ["/s/x.rs"])
        write(root, "b", "pkg-1", {"K": "4"}, ["/s/x.rs"])
        with contextlib.redirect_stdout(io.StringIO()):
            rc = main([os.path.join(root, "a"), os.path.join(root, "b")], run_selftest=False)
    assert rc == 0, f"identical readable trees must certify, got rc={rc}"

    with tempfile.TemporaryDirectory() as root:
        write(root, "a", "pkg-1", {"K": "4"}, ["/s/x.rs"],
              envkey="RerunIfEnvironmentChanged", pathkey="RerunIfPathChanged")
        write(root, "b", "pkg-1", {"K": "4"}, ["/s/x.rs"],
              envkey="RerunIfEnvironmentChanged", pathkey="RerunIfPathChanged")
        with contextlib.redirect_stderr(io.StringIO()) as refusal:
            rc = main([os.path.join(root, "a"), os.path.join(root, "b")], run_selftest=False)
    assert "RerunIfEnvChanged" in refusal.getvalue(), \
        "the refusal must name the key it could not find"
    assert rc == 2, (
        f"a renamed .fingerprint schema still certified sharing (rc={rc}) — an "
        "unparsed record set reads as 'every unit agrees'")

    if not quiet:
        print("nros-shared-dir-churn: self-test OK (7 cases: identical, env, paths, "
              "feature-variants, vacuity, readable-certifies, renamed-schema-refuses)")


def main(argv, run_selftest=True):
    if "--self-test" in argv:
        self_test()
        return 0
    # The negative control runs on the NORMAL path, not only behind a flag
    # (`check-gate-selftests`' rule). This tool's output sentence is "safe to
    # collapse onto one --target-dir"; the moment to prove its parser and its
    # schema refusal are still wired is the moment before it says that, not a
    # `--self-test` nobody invokes. Issue 0945 item 3. ~50 ms, in-process.
    if run_selftest:
        self_test(quiet=True)
    trees = [a for a in argv if not a.startswith("-")]
    if len(trees) < 2:
        print(__doc__.strip().splitlines()[0], file=sys.stderr)
        print("\nneed at least two build trees to compare", file=sys.stderr)
        return 2
    findings = compare(trees)
    schema = {}
    seen = {t: records(t, schema) for t in trees}
    counts = collections.Counter(u for r in seen.values() for u in r)
    shared = sum(1 for u, n in counts.items() if n > 1)

    # Issue 0945 item 3 — never certify on an unparsed schema either.
    #
    # `local` and its `RerunIfEnvChanged` / `RerunIfChanged` entries are cargo's
    # private on-disk format, read here with `.get(..., default)`. A rename does
    # not raise: every unit parses to an EMPTY env dict and an EMPTY path set,
    # every unit therefore agrees with every other, `findings` is empty, and the
    # tool prints "safe to collapse onto one --target-dir" — the most dangerous
    # sentence it can say, on evidence it did not read.
    #
    # `Precalculated` entries are normal and carry neither key (measured: 14 of
    # 125 records in the freertos tree, 16 of 72 in qemu-arm-baremetal), so the
    # predicate is "not ONE record in ANY tree yielded an entry", not "some
    # record yielded none".
    #
    # This is what 0945 assumed `--self-test` already provided. It did not: the
    # self-test writes the same key names it reads, so it pins the parser against
    # itself and stays green through a rename.
    if schema.get("records") and not schema.get("with_entries"):
        print(
            f"nros-shared-dir-churn: INCONCLUSIVE — {schema['records']} build-script "
            "record(s) read and NOT ONE\n"
            "yielded a RerunIfEnvChanged or RerunIfChanged entry. Cargo's .fingerprint\n"
            "format is private; a renamed key reads as \"every unit agrees\", which is\n"
            "indistinguishable from a clean result and would certify sharing on nothing.",
            file=sys.stderr,
        )
        return 2

    # Never certify on nothing. A tool that prints OK when it compared zero units
    # is the vacuous-pass shape this repo gates against elsewhere
    # (`check-no-vacuous-tests`): it reads as evidence and is the absence of it.
    if shared == 0:
        print(
            f"nros-shared-dir-churn: INCONCLUSIVE — {len(trees)} trees, "
            "no unit is common to two or more of them.",
            file=sys.stderr,
        )
        print("Nothing was compared, so this is not a pass.", file=sys.stderr)
        return 2

    # Records from different build eras are not comparable: the repo moved
    # between them, so a difference may be history rather than divergence.
    ages = {t: newest_record(t) for t in trees}
    known = [a for a in ages.values() if a is not None]
    spread = (max(known) - min(known)) if known else 0
    if spread > CONTEMPORARY_SECS:
        import time
        print(f"WARNING: these trees were last built {spread / 3600:.1f} h apart.")
        for t_, a in sorted(ages.items(), key=lambda kv: (kv[1] or 0)):
            when = time.strftime("%Y-%m-%d %H:%M", time.localtime(a)) if a else "unknown"
            print(f"  {when}  {t_}")
        print(
            "A divergence between trees built from different source states may be\n"
            "HISTORY, not a property of sharing. Rebuild the cluster, then measure.\n"
        )

    if not findings:
        print(
            f"nros-shared-dir-churn: OK — {len(trees)} trees, {shared} units, "
            "no unit records a different env value or watched-path set."
        )
        print("These trees are safe to collapse onto one --target-dir on this evidence.")
        return 0
    env = [f for f in findings if f[0] == "env"]
    paths = [f for f in findings if f[0] == "paths"]
    print(f"nros-shared-dir-churn: {len(trees)} trees, {shared} units")
    print(f"  ENV divergence  (churn):       {len(env)}")
    print(f"  PATH divergence (correctness): {len(paths)}")
    for kind, unit, detail in findings:
        print(f"    [{kind}] {unit}  {detail}")
    print(
        "\nA PATH divergence is the serious one: cargo decides freshness from the "
        "RECORDED list, so the smallest set governs the whole shared dir and the "
        "inputs outside it stop triggering a re-run."
    )
    return 1


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))

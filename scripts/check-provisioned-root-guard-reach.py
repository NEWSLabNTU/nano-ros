#!/usr/bin/env python3
"""A fixture build must ask whose checkout its provisioned roots are — issue 1395.

`scripts/check-zephyr-workspace-checkout.sh` refuses a provisioned root (a Zephyr
west workspace, `esp-idf-workspace`, `external/`) that belongs to a DIFFERENT
nano-ros checkout. Issue 1253 wrote it; phase-440 W5 generalised it from one root
to every root.

The guard was never wrong. Its REACH was: three callers, all of them downstream
of the build they are about. `check-tier-preconditions` (tier 1), and the two
tier-2 recipes phase-449 W2 wired -- while CI runs the build as its own earlier
step (`just build tier2` then `just ci matrix`). So the refusal that names the
workspace, the foreign checkout and the fix arrived after the fifteen-minute
cmake configure error about a binary that it exists to replace.

That is the phase-450 class: a gate green while the defect it exists for is
present, because nothing asked it in time. A reach fix is only durable if
something re-derives the reach, which is this file.

THE RULE, STATED ONCE

  Every front door to a fixture build reaches the ownership guard BEFORE the
  build, through ONE shared spelling.

and it is checked as four facts, three of them derived:

  R1 ONE SPELLING     `justfile` defines `_require-owned-provisioned-roots` and
                      it invokes the guard script. No other recipe in any
                      justfile invokes that script or re-implements its
                      question -- issue 0196's rule is widen the reach of the
                      one check, never add a second.

  R2 LANE FRONT DOOR  `build-test-fixtures` and `build-test-fixtures-leaves`
                      each DEPEND on it. Two edges, not one: `build-all` calls
                      the `-leaves` fan-out directly and would otherwise be
                      uncovered, and `just` runs a dependency at most once per
                      invocation, so the pair costs one run.

  R3 ROOT LANES       Every PROVISIONED ROOT has a platform lane that can be
                      reached without passing R2 (`just build <plat>` dispatches
                      to `just <plat> build-fixtures`, and `live-peer.yml` does
                      exactly that). Each such lane calls the shared recipe
                      itself. The ROOT NAMES are READ from the guard script's
                      own `PROVISIONED_ROOTS` block, so a root added there
                      cannot quietly acquire no lane coverage: it fails here
                      until someone classifies it.

  R4 NO BYPASS        A root lane must call the shared recipe, not the script.
                      Covered by R1's uniqueness scan; stated separately because
                      it is the tempting shortcut.

WHAT THIS DOES NOT CLAIM

It does not prove the guard runs before the first COMPILER invocation inside a
lane -- that is a property of statement order in a shell body, and the gate
checks placement in the recipe, not dominance over every build command. What it
does prove is that no fixture-build front door reaches a provisioned root
without the question being wired in at all, which is the defect 1395 measured.

Usage::

    check-provisioned-root-guard-reach.py
    check-provisioned-root-guard-reach.py --selftest
"""

import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
JUSTFILE = ROOT / "justfile"
JUST_DIR = ROOT / "just"
GUARD_SCRIPT = "scripts/check-zephyr-workspace-checkout.sh"
SHARED_RECIPE = "_require-owned-provisioned-roots"

# R2's two front doors. Not derived: these are the tree's PUBLIC fixture-build
# entry points, named in CLAUDE.md and in every CI workflow, and a derivation
# over "recipes mentioning build-test-fixtures" answers 25 recipes of which 23
# are echo lines and per-platform aliases. Two authored names with the reason
# each is here beats a broad set nobody can read.
LANE_FRONT_DOORS = {
    "build-test-fixtures": "the public lane build — `just build <lane>` and CI's `just build tier2`",
    "build-test-fixtures-leaves": "the fan-out `build-all` calls directly, bypassing the above",
}

# R3: provisioned root -> the just recipe that compiles against it without
# passing R2. The KEYS are checked against the guard script's own list, so this
# map cannot silently fall behind it; the VALUES are the authored half.
#
# A root with no platform lane of its own records that, with the reason. That is
# a classification, not an exemption: it asserts R2 is the only way in.
ROOT_LANES = {
    "zephyr": [("just/zephyr-ci.just", "build-fixtures")],
    # `external/` is a provisioning directory consumed by the generic build, not
    # a platform with a lane of its own. R2 is its only front door.
    "external": [],
}


def uncommented(text: str) -> str:
    return "\n".join(re.sub(r"#.*$", "", ln) for ln in text.splitlines())


def provisioned_roots(script_text: str) -> "list[str]":
    """The root names the guard itself sweeps, read from its PROVISIONED_ROOTS."""
    m = re.search(r'PROVISIONED_ROOTS="\n(.*?)\n"', script_text, re.S)
    if not m:
        return []
    names = []
    for line in m.group(1).splitlines():
        line = line.strip()
        if line:
            names.append(line.split(":", 1)[0])
    return names


def check(tree_justfile: str, tree_modules: "dict[str, str]", script_text: str) -> "list[str]":
    fails: "list[str]" = []

    root = recipes_from_text(tree_justfile)
    mods = {p: recipes_from_text(t) for p, t in tree_modules.items()}

    # --- R1: one spelling ---------------------------------------------------
    if SHARED_RECIPE not in root:
        fails.append(
            f"R1: `justfile` defines no `{SHARED_RECIPE}` recipe. It is the one "
            f"spelling every fixture-build front door reaches the ownership "
            f"guard through (issue 1395)."
        )
    elif GUARD_SCRIPT not in uncommented(root[SHARED_RECIPE][1]):
        fails.append(
            f"R1: `{SHARED_RECIPE}` does not invoke `{GUARD_SCRIPT}`. The recipe "
            f"is a reach widening, not a check of its own — it must run that "
            f"script and nothing else."
        )

    for label, table in [("justfile", root)] + [(p, m) for p, m in mods.items()]:
        for name, (deps, body) in table.items():
            if name == SHARED_RECIPE:
                continue
            if GUARD_SCRIPT in uncommented(deps + "\n" + body):
                fails.append(
                    f"R1/R4: `{label}::{name}` invokes `{GUARD_SCRIPT}` directly. "
                    f"Call `just {SHARED_RECIPE}` instead — a second call site "
                    f"for the script is the second spelling issue 0196 forbids, "
                    f"and it is what makes the reach un-auditable."
                )

    # --- R2: the lane front doors ------------------------------------------
    for name, why in LANE_FRONT_DOORS.items():
        if name not in root:
            fails.append(f"R2: `justfile` has no `{name}` recipe ({why}).")
            continue
        if SHARED_RECIPE not in uncommented(root[name][0]):
            fails.append(
                f"R2: `{name}` does not DEPEND on `{SHARED_RECIPE}` — {why}. "
                f"Without that edge the build compiles against a provisioned "
                f"root before anything asks whose checkout it is (issue 1395)."
            )

    # --- R3: one lane per provisioned root ----------------------------------
    roots = provisioned_roots(script_text)
    if not roots:
        fails.append(
            f"R3: could not read `PROVISIONED_ROOTS` out of {GUARD_SCRIPT}. The "
            f"root set is DERIVED from the guard on purpose; if that block moved, "
            f"fix this reader rather than authoring the list a second time."
        )
    for name in roots:
        if name not in ROOT_LANES:
            fails.append(
                f"R3: the guard sweeps a provisioned root `{name}` that this gate "
                f"has no classification for. Add it to ROOT_LANES: either the "
                f"`<file>, <recipe>` of the lane that builds against it without "
                f"passing `build-test-fixtures`, or an empty list saying it has "
                f"none and why."
            )
    for name, lanes in ROOT_LANES.items():
        if name not in roots:
            fails.append(
                f"R3: ROOT_LANES classifies `{name}`, which {GUARD_SCRIPT} no "
                f"longer sweeps. Drop the entry — a stale row here reports "
                f"coverage of a root nothing checks."
            )
            continue
        for path, recipe in lanes:
            table = mods.get(path)
            if table is None:
                fails.append(f"R3: {path} (root `{name}`) is not a justfile this gate read.")
                continue
            if recipe not in table:
                fails.append(f"R3: {path} has no `{recipe}` recipe (root `{name}`).")
                continue
            deps, body = table[recipe]
            if SHARED_RECIPE not in uncommented(deps + "\n" + body):
                fails.append(
                    f"R3: `{path}::{recipe}` builds against the provisioned root "
                    f"`{name}` and never reaches `{SHARED_RECIPE}`. `just build "
                    f"{path.split('/')[1].split('.')[0].replace('-ci', '')}` and "
                    f"`live-peer.yml` reach this lane WITHOUT passing "
                    f"`build-test-fixtures`, so R2's edges do not cover it — add "
                    f"`just {SHARED_RECIPE}` at the head of the body."
                )
    return fails


def recipes_from_text(text: str) -> "dict[str, tuple[str, str]]":
    header = re.compile(r"^([a-z_][a-z0-9_-]*)(?:\s+[^:\n]*?)?:(?!=)(.*)$")
    out: "dict[str, tuple[str, str]]" = {}
    name, deps, body = None, "", []
    for line in text.splitlines():
        m = header.match(line)
        if m and not line[:1].isspace():
            if name:
                out[name] = (deps, "\n".join(body))
            name, deps, body = m.group(1), m.group(2), []
        elif name is not None and (line[:1].isspace() or not line.strip()):
            body.append(line)
        elif name is not None:
            out[name] = (deps, "\n".join(body))
            name, deps, body = None, "", []
    if name:
        out[name] = (deps, "\n".join(body))
    return out


def load_tree() -> "tuple[str, dict[str, str], str]":
    modules = {
        f"just/{p.name}": p.read_text()
        for p in sorted(JUST_DIR.glob("*.just"))
    }
    return JUSTFILE.read_text(), modules, (ROOT / GUARD_SCRIPT).read_text()


def selftest(quiet: bool = False) -> int:
    """Plant each defect this gate exists for; a green here would be the bug."""
    jf, mods, script = load_tree()
    cases = [
        (
            "R2 edge removed from build-test-fixtures",
            jf.replace(
                f"build-test-fixtures lane=\"all\": {SHARED_RECIPE} ",
                'build-test-fixtures lane="all": ',
            ),
            mods,
            script,
            "R2",
        ),
        (
            "R3 call removed from the zephyr lane",
            jf,
            {
                **mods,
                "just/zephyr-ci.just": mods["just/zephyr-ci.just"].replace(
                    f"    just {SHARED_RECIPE}\n", ""
                ),
            },
            script,
            "R3",
        ),
        (
            "a second call site for the guard script",
            jf.replace(
                "build-all:",
                f"_planted-second-spelling:\n    @bash {GUARD_SCRIPT}\n\nbuild-all:",
                1,
            ),
            mods,
            script,
            "R1/R4",
        ),
        (
            "the guard grows a root this gate does not classify",
            jf,
            mods,
            script.replace(
                'external:${NROS_EXTERNAL_DIR_UNSET:-}:external\n"',
                'external:${NROS_EXTERNAL_DIR_UNSET:-}:external\nplanted::planted-workspace\n"',
            ),
            "R3",
        ),
    ]
    ok = True
    for label, a, b, c, expect in cases:
        got = check(a, b, c)
        hit = [f for f in got if f.startswith(expect)]
        if hit:
            if not quiet:
                print(f"  selftest OK   planted: {label} -> {hit[0].splitlines()[0][:96]}")
        else:
            ok = False
            print(f"  selftest FAIL planted: {label} -> no {expect} failure reported", file=sys.stderr)
    clean = check(jf, mods, script)
    if clean:
        ok = False
        print("  selftest FAIL: the unmodified tree is not clean:", file=sys.stderr)
        for f in clean:
            print(f"    {f}", file=sys.stderr)
    elif not quiet:
        print("  selftest OK   the unmodified tree is clean (negative control)")
    return 0 if ok else 1


def main() -> int:
    if "--selftest" in sys.argv:
        return selftest()
    # ALWAYS, not only behind the flag (`check-gate-selftests`): this gate is
    # green whenever the reach is intact, which is also what a gate that stopped
    # looking prints. The negative control is the only thing that tells them
    # apart, so it runs every time — ~10 ms of string work over four planted
    # trees, no I/O beyond the three files already read.
    if selftest(quiet=True) != 0:
        print(
            "check-provisioned-root-guard-reach: its own selftest failed — the "
            "gate cannot be trusted on this tree; run --selftest for detail.",
            file=sys.stderr,
        )
        return 1
    jf, mods, script = load_tree()
    fails = check(jf, mods, script)
    if fails:
        print(
            "check-provisioned-root-guard-reach: a fixture build can reach a "
            "provisioned root without asking whose checkout it is (issue 1395)\n",
            file=sys.stderr,
        )
        for f in fails:
            print(f"  {f}\n", file=sys.stderr)
        return 1
    roots = provisioned_roots(script)
    lanes = sum(len(v) for v in ROOT_LANES.values())
    print(
        f"check-provisioned-root-guard-reach: OK "
        f"({len(LANE_FRONT_DOORS)} lane front door(s) + {lanes} root lane(s) "
        f"over {len(roots)} provisioned root(s), one spelling)"
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())

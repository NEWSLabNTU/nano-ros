#!/usr/bin/env python3
"""One namespace for `just <verb> <scope>` — phase-411 W3.

Scope is the specification: `just test zephyr`, `just build tier2`,
`just doctor native`. Platform names and preset names share ONE argument
position, which buys a surface with one shape to learn — and costs exactly one
invariant, the one this gate holds:

    A name may not mean two things.

`native` is deliberately both a platform module and a fixture lane. That is
legal only because they denote the SAME scope: the `native` lane is every row
of the `native` module. Nothing enforced it. A preset added tomorrow that
happens to share a platform's name — or a lane whose module set quietly grows
past the platform it is named after — would silently re-scope somebody's run,
and the failure mode is the one phase-411 exists to remove: a run that reports
success for coverage it did not have.

Three things are checked, all buildlessly (this is on the fast line):

  1. Every justfile `mod` is CLASSIFIED — either a scope platform or an
     explicitly excluded tooling module. A new module has to be placed, rather
     than landing in neither list and being unreachable as a scope while
     looking like one.
  2. Every scope platform names a module that exists.
  3. Every name that is BOTH a preset and a platform expands to exactly that
     platform. `NROS_SCOPE_NO_BUILD=1` makes the expansion refuse to compile
     `lane-coords`, so a colliding preset whose meaning is only knowable after
     a build fails HERE — an unverifiable collision is itself the defect.

The preset list is not written here: it is `_NROS_LANES` in
`scripts/build/fixture-lane.sh`, read through the same shell functions the
recipes use, so this gate cannot check a vocabulary the verbs do not have.
"""

import os
import re
import subprocess
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
SCOPE_SH = os.path.join("scripts", "build", "scope.sh")
MOD = re.compile(r"^mod\s+([A-Za-z0-9_]+)\s", re.M)


def sh(snippet, env=None):
    """Run a snippet against the real scope.sh and return its stdout words."""
    full = dict(os.environ)
    full["NROS_SCOPE_NO_BUILD"] = "1"
    if env:
        full.update(env)
    out = subprocess.run(
        ["bash", "-c", f". {SCOPE_SH}; {snippet}"],
        cwd=ROOT,
        env=full,
        capture_output=True,
        text=True,
    )
    return out.returncode, out.stdout.split(), out.stderr


def justfile_modules():
    with open(os.path.join(ROOT, "justfile"), encoding="utf8") as fh:
        return sorted(set(MOD.findall(fh.read())))


def analyse(modules, platforms, non_platform, presets, expand):
    """The rule, as a pure function — so the selftest can exercise a red.

    `expand` maps a preset name to the platform set it denotes, or to None when
    it cannot be resolved without a build.
    """
    problems = []
    classified = set(platforms) | set(non_platform)
    for m in modules:
        if m not in classified:
            problems.append(
                f"justfile module `{m}` is in neither _NROS_SCOPE_PLATFORMS nor "
                f"_NROS_SCOPE_NON_PLATFORM_MODULES — classify it in {SCOPE_SH} "
                f"(a scope people can type, or a tooling module with the reason why not)"
            )
    for p in platforms:
        if p not in modules:
            problems.append(
                f"scope platform `{p}` names no `mod {p}` in the justfile — "
                f"`just {p} <verb>` cannot resolve, so no verb can dispatch to it"
            )
    for name in sorted(set(presets) & set(platforms)):
        got = expand.get(name)
        if got is None:
            problems.append(
                f"preset `{name}` collides with the platform of the same name and its "
                f"expansion needs a build to know — an unverifiable collision. Give it a "
                f"static arm in nros_scope_preset_expand, or rename one of the two."
            )
        elif set(got) != {name}:
            problems.append(
                f"preset `{name}` collides with the platform `{name}` and means something "
                f"ELSE: it expands to {' '.join(sorted(got)) or '(nothing)'}. One name, two "
                f"scopes — rename the preset or make it denote exactly `{name}`."
            )
    return problems


def self_test():
    """Prove the rule can go red, on the normal path, every run.

    A negative control nobody runs decays into a comment (check-board-tiers.py),
    so this is not behind a flag. It costs nothing: `analyse` is pure.
    """
    healthy = analyse(
        modules=["native", "zephyr", "check"],
        platforms=["native", "zephyr"],
        non_platform=["check"],
        presets=["all", "native"],
        expand={"native": ["native"]},
    )
    assert healthy == [], f"selftest: healthy input reported {healthy}"

    collide = analyse(
        modules=["native", "zephyr", "check"],
        platforms=["native", "zephyr"],
        non_platform=["check"],
        presets=["all", "native"],
        expand={"native": ["native", "zephyr"]},
    )
    assert len(collide) == 1 and "means something ELSE" in collide[0], collide

    unresolvable = analyse(
        modules=["native"],
        platforms=["native"],
        non_platform=[],
        presets=["native"],
        expand={"native": None},
    )
    assert len(unresolvable) == 1 and "unverifiable collision" in unresolvable[0], unresolvable

    unclassified = analyse(
        modules=["native", "newthing"],
        platforms=["native"],
        non_platform=[],
        presets=[],
        expand={},
    )
    assert len(unclassified) == 1 and "neither" in unclassified[0], unclassified

    missing_mod = analyse(
        modules=["native"],
        platforms=["native", "ghost"],
        non_platform=[],
        presets=[],
        expand={},
    )
    assert len(missing_mod) == 1 and "names no `mod" in missing_mod[0], missing_mod


def main():
    self_test()

    rc, platforms, err = sh("nros_scope_platforms")
    if rc != 0:
        print(f"check-scope-namespace: cannot read the platform list:\n{err}", file=sys.stderr)
        return 1
    rc, presets, err = sh("nros_scope_presets")
    if rc != 0:
        print(f"check-scope-namespace: cannot read the preset list:\n{err}", file=sys.stderr)
        return 1
    _, non_platform, _ = sh('printf "%s\\n" $_NROS_SCOPE_NON_PLATFORM_MODULES')
    modules = justfile_modules()

    expand = {}
    for name in sorted(set(presets) & set(platforms)):
        rc, got, _ = sh(f"nros_scope_preset_expand {name}")
        expand[name] = got if rc == 0 else None

    problems = analyse(modules, platforms, non_platform, presets, expand)
    if problems:
        print("check-scope-namespace: the scope namespace is not one namespace.", file=sys.stderr)
        print("", file=sys.stderr)
        for p in problems:
            print(f"  - {p}", file=sys.stderr)
        print("", file=sys.stderr)
        print(f"  platforms: {' '.join(platforms)}", file=sys.stderr)
        print(f"  presets  : {' '.join(presets)}", file=sys.stderr)
        return 1

    shared = sorted(set(presets) & set(platforms))
    print(
        f"check-scope-namespace: OK — {len(platforms)} platform(s), {len(presets)} preset(s), "
        f"{len(modules)} module(s); shared name(s): {' '.join(shared) or 'none'}"
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())

#!/usr/bin/env python3
"""Every declared CI tier must be RUN by something — phase-411.

RFC-0061 declares four tiers and `CiTier::just_recipe()` names the command for
each. A tier nothing invokes is a promise with no owner: the ladder says "host
only, every commit", the workflows say nothing, and the difference is invisible
because a lane that does not exist cannot go red.

Measured 2026-08-31, before this gate:

  Tier1  `just ci tier1`         0 references in .github/workflows  -- UNOWNED
  Tier2  `just ci matrix`        post-submit, skipped on the self-hosted interlock
  Tier2N `just ci matrix-nightly` nightly.yml
  Tier3  `just ci full`          0 references                       -- UNOWNED

Two of four tiers were run by nothing at all, and `host-tests` hand-rolled a
subset of tier 1 (`native build-fixture-rust-core` + `build-workspace-fixtures`
+ `test-integration`) instead of invoking it — so the tier's definition and CI's
behaviour could drift silently, and did.

WHAT COUNTS AS AN OWNER

A workflow that invokes the tier's recipe, in any spelling `just` accepts
(`just ci matrix` or the flat `just ci-matrix` forwarder), or a documented
OWNERLESS entry below with the reason. Being gated on an interlock still counts
as owned — `check-interlock-visibility` is what makes a skipped owner visible;
this gate answers the prior question of whether anyone runs it at all.

Buildless: the ladder is Rust source, the owners are YAML.
"""

import os
import re
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
BUCKETS = os.path.join(ROOT, "packages", "testing", "nros-tests", "src", "buckets.rs")
WORKFLOWS = os.path.join(ROOT, ".github", "workflows")

# A tier may be deliberately unowned, with the reason recorded HERE so the
# absence is a decision someone can read rather than an accident nobody saw.
OWNERLESS = {
    "ci full": (
        "tier 3 is pre-release and on demand by design (RFC-0061). It needs the "
        "full SDK set and hours; wiring it to an event would make it a lane "
        "nobody can afford, which is how a tier gets switched off."
    ),
}

RECIPE = re.compile(r'CiTier::\w+\s*=>\s*"([^"]+)"')


def declared_tiers(text):
    """The recipe each tier declares, from `just_recipe`'s match arms."""
    m = re.search(r"fn just_recipe\(self\).*?\{(.*?)\n    \}", text, re.S)
    return RECIPE.findall(m.group(1)) if m else []


def owners(workflow_texts, recipe):
    """Workflows invoking `recipe`, in any spelling just accepts."""
    flat = recipe.replace(" ", "-")          # `ci matrix` -> `ci-matrix`
    pats = (re.compile(rf"just\s+{re.escape(recipe)}(\s|$)"),
            re.compile(rf"just\s+{re.escape(flat)}(\s|$)"))
    return sorted(fn for fn, t in workflow_texts.items()
                  for line in t.split("\n")
                  if not line.lstrip().startswith("#")
                  and any(p.search(line) for p in pats))


def selftest():
    """Both verdicts, on the normal path — phase-395."""
    src = ('fn just_recipe(self) -> &\'static str {\n'
           '        match self {\n'
           '            CiTier::Tier1 => "ci tier1",\n'
           '            CiTier::Tier3 => "ci full",\n'
           '        }\n    }\n')
    assert declared_tiers(src) == ["ci tier1", "ci full"], declared_tiers(src)

    wf = {"a.yml": "      - run: just ci tier1\n"}
    assert owners(wf, "ci tier1") == ["a.yml"], "a direct invocation must count"
    assert owners({"a.yml": "  - run: just ci-tier1\n"}, "ci tier1") == ["a.yml"], \
        "the flat forwarder spelling must count"
    assert owners({"a.yml": "  # just ci tier1 is not run here\n"}, "ci tier1") == [], \
        "a comment is prose, not an owner"
    assert owners({"a.yml": "  - run: just ci tier1-extra\n"}, "ci tier1") == [], \
        "a longer recipe name must not count as this tier"


def main():
    selftest()
    with open(BUCKETS, encoding="utf-8") as fh:
        tiers = declared_tiers(fh.read())
    if not tiers:
        sys.exit("check-tier-has-ci-owner: could not parse CiTier::just_recipe")

    wfs = {}
    for fn in sorted(os.listdir(WORKFLOWS)):
        if fn.endswith((".yml", ".yaml")):
            with open(os.path.join(WORKFLOWS, fn), encoding="utf-8") as fh:
                wfs[fn] = fh.read()

    problems, lines = [], []
    for recipe in tiers:
        own = owners(wfs, recipe)
        if own:
            lines.append(f"  just {recipe:<20} <- {', '.join(own)}")
        elif recipe in OWNERLESS:
            lines.append(f"  just {recipe:<20} <- (deliberately unowned)")
        else:
            problems.append(
                f"`just {recipe}` is declared in the CI ladder but NO workflow "
                f"runs it. A tier nothing invokes is a promise with no owner, "
                f"and it cannot go red to tell you. Wire it to an event, or add "
                f"it to OWNERLESS in {os.path.basename(__file__)} with the reason.")

    if problems:
        sys.stderr.write("check-tier-has-ci-owner: FAILED\n")
        for p in problems:
            sys.stderr.write(f"  {p}\n")
        return 1
    print(f"tier CI owners: OK ({len(tiers)} tier(s))")
    for l in lines:
        print(l)
    return 0


if __name__ == "__main__":
    sys.exit(main())

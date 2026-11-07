#!/usr/bin/env python3
"""Every gate's scope is declared, and every capability has a home (phase-543 W3).

A gate under the nextest harness never passes by skipping: a lane's scope
decides what runs (`.config/gate-lanes.toml`). That only holds while three
things stay true, which this checks:

1. Every `[group('needs:<cap>')]` names a capability in `[capabilities]`.
2. Every capability a lane claims exists. A capability NO lane claims is
   listed on every run: the gates needing it never run anywhere, and the list
   is how that stays visible. A NEW unclaimed capability fails until it is
   added to the accepted set below, with a reason.
3. A `check` recipe whose body calls `nros_check_skip` / `nros_check_unverified`
   declares at least one `needs:`. Such a recipe can find its precondition
   absent, and its lane must be able to say so before running it. The recipes
   that did this before W3 are a ratchet baseline
   (`.config/gate-needs-baseline.txt`): an entry may leave it, never join it.
"""

from __future__ import annotations

import json
import re
import subprocess
import sys
from pathlib import Path

try:
    import tomllib
except ModuleNotFoundError:
    import tomli as tomllib

REPO = Path(__file__).resolve().parents[2]
LANES = REPO / ".config" / "gate-lanes.toml"
BASELINE = REPO / ".config" / "gate-needs-baseline.txt"

# Capabilities no lane claims, each with why. The gates needing one run nowhere.
UNCLAIMED_OK = {
    "fsp": "the NVIDIA Orin SPE FSP tree is provisioned on no runner (phase-418 418.4)",
    "nuttx": "no gate lane provisions NUTTX_DIR; the NuttX fixture lanes run their own builds",
}

SKIP_CALL = re.compile(r"\bnros_check_(skip|unverified)\b")


def check(groups: dict[str, set[str]], bodies: dict[str, str], cfg: dict,
          baseline: set[str]) -> tuple[list[str], list[str]]:
    caps = set(cfg.get("capabilities", {}))
    problems: list[str] = []
    notes: list[str] = []
    for name, g in sorted(groups.items()):
        for n in sorted(x.removeprefix("needs:") for x in g if x.startswith("needs:")):
            if n not in caps:
                problems.append(f"  {name}: needs `{n}`, which [capabilities] does not list")
    resources = cfg.get("resources", {})
    for name, g in sorted(groups.items()):
        for r in sorted(x.removeprefix("uses:") for x in g if x.startswith("uses:")):
            if r not in resources:
                problems.append(f"  {name}: uses `{r}`, which [resources] does not list")
        for c in sorted(x.removeprefix("cpu:") for x in g if x.startswith("cpu:")):
            if not c.isdigit() or int(c) < 1:
                problems.append(f"  {name}: `cpu:{c}` is not a positive integer")
    for r, n in sorted(resources.items()):
        if not isinstance(n, int) or n < 1:
            problems.append(f"  [resources] {r} = {n!r}: max holders must be a positive integer")
    claimed: set[str] = set()
    for lane, spec in cfg.get("lane", {}).items():
        for c in spec.get("claims", []):
            if c not in caps:
                problems.append(f"  lane `{lane}` claims `{c}`, which [capabilities] does not list")
            claimed.add(c)
    for c in sorted(caps - claimed):
        if c in UNCLAIMED_OK:
            users = sorted(n for n, g in groups.items() if f"needs:{c}" in g)
            notes.append(f"  NO LANE CLAIMS `{c}` ({UNCLAIMED_OK[c]}): runs nowhere: "
                         + (", ".join(users) or "no gate needs it"))
        else:
            problems.append(f"  no lane claims `{c}`: a gate needing it would run nowhere. "
                            "Claim it in a lane, or accept it in UNCLAIMED_OK with a reason")
    for name, body in sorted(bodies.items()):
        has_needs = any(x.startswith("needs:") for x in groups.get(name, set()))
        if SKIP_CALL.search(body) and not has_needs and name not in baseline:
            problems.append(f"  {name}: calls nros_check_skip/unverified but declares no "
                            "[group('needs:<cap>')], so no lane can scope it out")
    for name in sorted(baseline):
        if any(x.startswith("needs:") for x in groups.get(name, set())):
            problems.append(f"  {name}: declares needs: now; delete it from {BASELINE.name}")
    return problems, notes


def self_test() -> None:
    cfg = {"capabilities": {"a": "", "b": "", "z": ""},
           "lane": {"l": {"claims": ["a", "b"]}}}
    cfg["resources"] = {"r": 1}
    g = {"good": {"needs:a", "uses:r", "cpu:4"}, "bad": {"needs:nope", "uses:nope", "cpu:x"},
         "skipper": set(), "old": set()}
    bodies = {"skipper": "nros_check_skip skipper x", "old": "nros_check_unverified old y",
              "good": "nros_check_skip good x"}
    p, _ = check(g, bodies, cfg, {"old"})
    text = "\n".join(p)
    assert "bad: needs `nope`" in text, text
    assert "bad: uses `nope`" in text and "`cpu:x`" in text, text
    assert "good:" not in text, text
    assert "skipper: calls" in text, text
    assert "old:" not in text, text
    assert "no lane claims `z`" in text, text
    p, _ = check({"old": {"needs:a"}}, {}, {"capabilities": {"a": ""}, "lane": {"l": {"claims": ["a"]}}}, {"old"})
    assert any("delete it from" in x for x in p), p


def main() -> int:
    self_test()
    out = subprocess.run(["just", "--dump", "--dump-format", "json"], cwd=REPO,
                         capture_output=True, text=True, check=True).stdout
    recipes = json.loads(out)["modules"]["check"]["recipes"]
    groups = {n: {a["group"] for a in r.get("attributes", []) if isinstance(a, dict) and "group" in a}
              for n, r in recipes.items()}
    bodies = {n: "\n".join("".join(p if isinstance(p, str) else "" for p in line) for line in r["body"])
              for n, r in recipes.items()}
    cfg = tomllib.loads(LANES.read_text())
    baseline = {l.split("#")[0].strip() for l in BASELINE.read_text().splitlines()} - {""} \
        if BASELINE.is_file() else set()
    if "--write-baseline" in sys.argv:
        _, _ = check(groups, bodies, cfg, set())
        missing = sorted(n for n, b in bodies.items() if SKIP_CALL.search(b)
                         and not any(x.startswith("needs:") for x in groups.get(n, set())))
        BASELINE.write_text(
            "# Recipes that call nros_check_skip/unverified and declare no needs: yet\n"
            "# (phase-543 W3 ratchet, check-gate-needs). Remove a line when its\n"
            "# recipe gains [group('needs:<cap>')]; never add one.\n"
            + "".join(f"{n}\n" for n in missing))
        print(f"check-gate-needs: wrote {len(missing)} name(s) to {BASELINE.relative_to(REPO)}")
        return 0
    problems, notes = check(groups, bodies, cfg, baseline)
    for n in notes:
        print(n)
    if problems:
        print("check-gate-needs: FAILED\n" + "\n".join(problems), file=sys.stderr)
        return 1
    declared = sum(1 for g in groups.values() if any(x.startswith("needs:") for x in g))
    print(f"check-gate-needs OK — {declared} gate(s) declare needs:, "
          f"{len(baseline)} skip site(s) still in the ratchet baseline")
    return 0


if __name__ == "__main__":
    sys.exit(main())

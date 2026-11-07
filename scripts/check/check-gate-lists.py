#!/usr/bin/env python3
"""The fast lane is DERIVED from the recipes; only its exceptions are authored.

WHY THERE IS NO LONGER A `fast-serial:` REGISTRY
------------------------------------------------
`just/check.just` names every gate twice: once as a recipe, and once as a
dependency of the lane that runs it. That second name is the shape issues
0883/0884 are about — a line EVERY pull request appends to.

The first fix was to make the list one-name-per-line and sorted, on the
reasoning that two gates with different names then land at different lines.
Measured on 2026-09-05, that is only true when the names are far enough apart.
Two ADJACENT names insert at the SAME base line, git has no unchanged line
between them to anchor on, and it conflicts with certainty. PR #472 hit exactly
that: `codegen-stamp-inputs` against main's `codegen-tool-reconfigure`, one
hunk, three lines. And names are not independent — related work produces
related names (`codegen-*`, `xrce-*`, `zenoh-*`), so sorting turns name
correlation directly into position correlation.

No merge driver can fix it. 0884 established that GitHub rebases queue entries
SERVER-SIDE, where `.gitattributes` drivers do not run; the fix that reached the
queue for `open.md` was to stop committing the shared line at all. The same
answer applies here, and this file is it:

    a recipe in `just/check.just` is a FAST-LANE GATE unless it carries
    `[metadata("tier", "build")]` (it needs something built) or
    `[metadata("tier", "exempt", "<why>")]` (in no lane, with a reason), or
    declares parameters (a gate is invoked as `just check <name>` with no
    arguments, so a recipe that requires one cannot be one).

Adding a gate is now writing its recipe. Nothing shared is touched, so two
authors adding gates cannot collide on a registry that no longer exists.

WHAT THIS BUYS BEYOND THE MERGE QUEUE
-------------------------------------
Issue 1071: PR #431 deleted four gates by dropping their registry lines and
`check-gate-lists` stayed green, because it verified the list's SHAPE and never
its size. There is no fast-lane registry line to drop now. A fast gate can only
leave the lane by having its RECIPE deleted, which is visible in review, and
which a name-set ratchet over `gate_names()` catches outright.

The polarity flip also inverts the failure mode. Forgetting the registry line
used to mean the gate NEVER RAN — silent, and the repo's most-repeated defect
(issue 0196's class). Forgetting to exempt a new recipe now means it runs on the
fast lane, which is loud and lands on the author's own pull request.

phase-543 W6 finished the inversion: the build tier and the exemptions moved
from the `build-serial:` list and `.config/gate-lane-exempt.txt` onto the
recipes, as `tier` metadata read through `just --dump`. No authored list of
gates is left anywhere.

FOR A NAME-SET RATCHET
----------------------
`gate_names()` returns the full sorted set (fast + build). A baseline file of
names that may only grow compares against that one call — the same interface a
registry-parsing ratchet had, minus the registry.
"""

from __future__ import annotations

import json
import os
import re
import subprocess
import sys
from pathlib import Path

REPO = Path(__file__).resolve().parents[2]
JUSTFILE = REPO / "just" / "check.just"

# phase-425/issue 1071 — the deletion ratchet's baseline. It survived the
# 1072 rewrite unchanged in CONTENT: its 239 names are byte-identical to the
# derived fast+build set, so inverting where membership is WRITTEN did not
# move what the ratchet reads.
BASELINE = REPO / ".config" / "gate-registry-baseline.txt"
# Module-level so `check-baseline-shape` can hold the file to it.
BASELINE_HEADER = (
    "# The gate names `just/check.just`'s registries have carried.\n"
    "# May only GROW. A name here and not in a registry fails\n"
    "# `check-gate-lists` (issue 1071 -- a PR deleted four gates and every\n"
    "# gate stayed green, because sorted-and-one-per-line is a property a\n"
    "# deletion satisfies).\n"
    "#\n"
    "# Regenerate ONLY for a deliberate retirement, and say which gate went:\n"
    "#   python3 scripts/check/check-gate-lists.py --write-baseline\n"
)

# phase-543 W6 — a recipe's TIER is an attribute on the recipe itself:
#   [metadata("tier", "build")]             compiles, or needs something built
#   [metadata("tier", "exempt", "<why>")]   in no lane; the reason is required
# No attribute: a fast gate. There is no list to append to, so two pull
# requests adding gates never touch the same line (issues 0883/0884/1072), and
# the reason for an exemption sits beside the recipe it explains.
TIERS = ("build", "exempt")

# A recipe header at column 0. Variables use `:=`, attributes start with `[`,
# comments with `#`, and bodies are indented, so none of them match.
RECIPE_RE = re.compile(r"^([a-z_][a-z0-9_-]*)([^:\n]*):(?!=)")

def justfile_sources() -> list[Path]:
    """`just/check.just` and every topic file it imports.

    The gates live in `just/check/*.just` and are brought in with `import`,
    which MERGES definitions -- so a recipe there is `just check <name>` just
    like one written here. Reading only the index would derive a fast lane of
    the seven lane recipes and silently drop 286 gates, which is the exact
    shape of failure this file exists to prevent. Measured when the split
    landed: the derived list went 231 -> 8 before this function existed.

    Resolved relative to the importing file, and NOT globbed: the set is
    whatever `check.just` actually imports, so a topic file that stops being
    imported stops contributing gates here too, rather than counting from disk
    while `just` cannot see it.
    """
    text = JUSTFILE.read_text(encoding="utf-8")
    out = [JUSTFILE]
    for m in re.finditer(r"^import\s+'([^']+)'", text, re.MULTILINE):
        p = (JUSTFILE.parent / m.group(1)).resolve()
        if p.is_file():
            out.append(p)
    return out


def justfile_text() -> str:
    return "\n".join(p.read_text(encoding="utf-8") for p in justfile_sources())


def recipes(lines: list[str]) -> dict[str, bool]:
    """Every top-level recipe name -> whether it declares parameters.

    A recipe that takes an argument cannot be a gate: the runners invoke
    `just check <name>` with nothing after the name. `stack-elf elf top="30"`
    would fail on the missing `elf` every time, so excluding it is a property
    of the recipe rather than a decision someone has to remember to record.
    """
    out: dict[str, bool] = {}
    for line in lines:
        m = RECIPE_RE.match(line)
        if m:
            out[m.group(1)] = bool(m.group(2).strip())
    return out


def recipe_tiers() -> dict[str, list[str]]:
    """Every `check` recipe's `[metadata("tier", ...)]` values (without the
    leading "tier"), from `just --dump`, the one view of a recipe's attributes."""
    out = subprocess.run(
        ["just", "--dump", "--dump-format", "json"],
        cwd=REPO, capture_output=True, text=True, check=True,
    ).stdout
    recs = json.loads(out)["modules"]["check"]["recipes"]
    tiers: dict[str, list[str]] = {}
    for name, r in recs.items():
        for a in r.get("attributes", []):
            if isinstance(a, dict) and isinstance(a.get("metadata"), list) \
                    and a["metadata"][:1] == ["tier"]:
                tiers[name] = a["metadata"][1:]
    return tiers


def classify(just_text: str, tiers: dict[str, list[str]]) -> tuple[dict[str, list[str]], list[str]]:
    """Split every recipe into fast / build / exempt / parameterized.

    Returns (buckets, problems). A recipe lands in exactly one bucket: its
    `tier` metadata says build or exempt, a recipe with parameters can never
    be a gate, and the fast lane is whatever is left over.
    """
    rec = recipes(just_text.split("\n"))
    problems: list[str] = []
    parameterized = sorted(n for n, has_args in rec.items() if has_args)
    build: list[str] = []
    exempt: list[str] = []
    for name, vals in sorted(tiers.items()):
        tier = vals[0] if vals else ""
        if name not in rec:
            problems.append(f"  `{name}`: a tier for a recipe this file set does not define")
            continue
        if tier not in TIERS:
            problems.append(
                f"  `{name}`: tier `{tier}` is not one of {', '.join(TIERS)}.\n"
                f"      A fast gate carries no tier attribute at all."
            )
            continue
        if rec[name]:
            problems.append(
                f"  `{name}`: declares parameters but carries a tier.\n"
                f"      A recipe with arguments can never run as `just check <name>`,\n"
                f"      so it is excluded structurally and needs no attribute."
            )
            continue
        if tier == "exempt":
            if len(vals) < 2 or not vals[1].strip():
                problems.append(
                    f"  `{name}`: exempt with no reason.\n"
                    f'      Write `[metadata("tier", "exempt", "<why>")]`: say why it is not\n'
                    f"      a gate. A bare exemption is how a temporary one becomes permanent."
                )
            exempt.append(name)
        else:
            build.append(name)
    plain = {n for n, has_args in rec.items() if not has_args}
    fast = sorted(plain - set(build) - set(exempt))
    if not fast:
        problems.append(
            "  the derived FAST lane is EMPTY.\n"
            "      Refusing to report OK over nothing — the runner would too."
        )
    return (
        {"fast": fast, "build": sorted(build), "exempt": sorted(exempt),
         "parameterized": parameterized},
        problems,
    )


def recipe_groups() -> dict[str, set[str]]:
    """Every `check` recipe's `[group(...)]` values, read from `just --dump`,
    the one view of a recipe's attributes (phase-543)."""
    out = subprocess.run(
        ["just", "--dump", "--dump-format", "json"],
        cwd=REPO, capture_output=True, text=True, check=True,
    ).stdout
    recipes = json.loads(out)["modules"]["check"]["recipes"]
    return {
        name: {a["group"] for a in r.get("attributes", []) if isinstance(a, dict) and "group" in a}
        for name, r in recipes.items()
    }


def needs_of(groups: set[str]) -> set[str]:
    return {g.removeprefix("needs:") for g in groups if g.startswith("needs:")}


def gate_units(name: str) -> list[tuple[str, set[str]]] | None:
    """The units a `[group('units')]` gate lists, each with the capabilities
    it needs beyond the gate's own (a lister line is `<unit>` or
    `<unit>\tneeds:<cap>[,<cap>]`), or None when it cannot list them. `None`
    makes the gate ONE test, which then fails when run for the same reason, so
    a broken lister is a red and never a dropped gate."""
    env = dict(os.environ, NROS_GATE_LIST_UNITS="1")
    env.pop("NROS_GATE_UNIT", None)
    p = subprocess.run(
        ["just", f"check::{name}"], cwd=REPO, env=env,
        capture_output=True, text=True,
    )
    units = []
    for line in p.stdout.splitlines():
        if not line.strip():
            continue
        unit, _, extra = line.partition("\t")
        caps = set(extra.removeprefix("needs:").split(",")) - {""} if extra else set()
        units.append((unit, caps))
    if p.returncode != 0 or not units:
        print(f"check-gate-lists: `{name}` could not list its units "
              f"(rc={p.returncode}); listed as one test", file=sys.stderr)
        return None
    return units


LANES_FILE = REPO / ".config" / "gate-lanes.toml"


def load_lane(name: str | None) -> dict | None:
    """The lane's `{tiers, claims}`, or None for no lane: everything in scope."""
    if not name:
        return None
    try:
        import tomllib
    except ModuleNotFoundError:
        import tomli as tomllib
    lanes = tomllib.loads(LANES_FILE.read_text())["lane"]
    if name not in lanes:
        raise SystemExit(f"check-gate-lists: no lane `{name}` in {LANES_FILE.relative_to(REPO)}")
    return {"tiers": set(lanes[name]["tiers"]), "claims": set(lanes[name]["claims"])}


def scope(lane: dict | None, tier: str, needs: set[str]) -> str | None:
    """None when in scope, else the reason it is not."""
    if lane is None:
        return None
    if tier not in lane["tiers"]:
        return f"tier `{tier}` is not in this lane"
    missing = sorted(needs - lane["claims"])
    if missing:
        return "needs " + ", ".join(missing) + ", which this lane does not claim"
    return None


def test_names(b: dict[str, list[str]], lane_name: str | None = None) -> list[tuple[str, str | None]]:
    """phase-543 — the nextest harness's listing: `(name, out-of-scope reason)`.

    A name is `<tier>::<just target>`, or `<tier>::<just target>#<unit>` for
    a `[group('units')]` gate; the part after the first `::` and before `#` is
    the target. Lane-exempt recipes are listed as `exempt::`, so ONE listing
    names every gate. With a lane, a test whose tier or `needs:` the lane does
    not cover carries the reason, and the harness lists it as ignored. A gate
    out of scope as a whole is not asked for its units: a lister may need what
    the lane lacks."""
    lane = load_lane(lane_name)
    groups = recipe_groups()
    out: list[tuple[str, str | None]] = []
    for tier in ("fast", "build", "exempt"):
        for n in b[tier]:
            g = groups.get(n, set())
            needs = needs_of(g)
            why = scope(lane, tier, needs)
            units = gate_units(n) if "units" in g and why is None else None
            if units is None:
                out.append((f"{tier}::check::{n}", why))
            else:
                out.extend(
                    (f"{tier}::check::{n}#{u}", scope(lane, tier, needs | caps))
                    for u, caps in units
                )
    return out

TOOL = "nros-gates"


def junit_seconds(path: Path | None) -> dict[str, float]:
    """Each test's wall time in a previous run's junit, by test name. Empty
    when there is none: priority is an ordering hint, never a requirement."""
    if path is None or not path.is_file():
        return {}
    import xml.etree.ElementTree as ET
    try:
        root = ET.parse(path).getroot()
    except ET.ParseError:
        return {}
    return {c.get("name", ""): float(c.get("time") or 0) for c in root.iter("testcase")}


def junit_tsv(path: Path) -> str:
    """`name rc wall_ms cpu_ms` per test, the pre-push log's format."""
    import xml.etree.ElementTree as ET
    rows = []
    for c in ET.parse(path).getroot().iter("testcase"):
        failed = any(x.tag in ("failure", "error") for x in c)
        rows.append(f"{c.get('name', '')}\t{1 if failed else 0}\t"
                    f"{int(float(c.get('time') or 0) * 1000)}\t0")
    return "\n".join(rows) + ("\n" if rows else "")


def order_priorities(names: list[str], order: str) -> dict[str, int]:
    """Spread `names` over nextest's priority range (-100..=100) by NAME, in
    `asc` or `desc` order, so two runs start the tests in opposite orders
    (phase-543's order-independence check, `scripts/check/check-gate-order.sh`).
    Neighbours share a level when there are more tests than levels."""
    ordered = sorted(names, reverse=(order == "asc"))  # higher priority starts first
    n = len(ordered)
    return {name: (round(i * 200 / (n - 1)) - 100 if n > 1 else 0)
            for i, name in enumerate(ordered)}


def scratch_free_kib() -> int | None:
    """Free KiB under `$TMPDIR` (else /tmp), where the gates' scratch trees go
    (scripts/build/gate-scratch.sh). None when it cannot be measured."""
    import os
    try:
        st = os.statvfs(os.environ.get("TMPDIR") or "/tmp")
    except OSError:
        return None
    return st.f_bavail * st.f_frsize // 1024


def resource_capacity(name: str, spec, free_kib: int | None) -> int:
    """How many tests may hold resource `name` at once.

    An integer is taken as given. `{ scratch-gib = N }` is a resource sized
    from SCRATCH DISK (issue 2356): each holder keeps a tree of up to N GiB
    under `$TMPDIR`, so the count is TMPDIR's free space at config time over
    N, and never below 1. The config is generated per run, so the count is the
    free space of the host and moment that run has, not a number chosen for
    one machine. Unmeasurable free space keeps one holder: the safe side."""
    if isinstance(spec, bool):
        raise SystemExit(f"check-gate-lists: [resources] {name} = {spec!r} is not a count")
    if isinstance(spec, int):
        if spec < 1:
            raise SystemExit(f"check-gate-lists: [resources] {name} = {spec}; a resource needs >= 1 holder")
        return spec
    if isinstance(spec, dict) and set(spec) == {"scratch-gib"}:
        gib = spec["scratch-gib"]
        if isinstance(gib, bool) or not isinstance(gib, (int, float)) or gib <= 0:
            raise SystemExit(f"check-gate-lists: [resources] {name}: scratch-gib must be > 0, got {gib!r}")
        if free_kib is None:
            return 1
        return max(1, int(free_kib // (gib * 1024 * 1024)))
    raise SystemExit(f"check-gate-lists: [resources] {name} = {spec!r}; "
                     "want a count, or { scratch-gib = <GiB per holder> }")


def nextest_config(b: dict[str, list[str]], junit: Path | None = None,
                   order: str | None = None) -> str:
    """phase-543 W4 — the harness's nextest TOOL config, generated from the
    recipes' attributes so it cannot drift from them.

    * `[group('uses:<res>')]` puts every test of that gate in the test group
      `@tool:nros-gates:<res>`, whose `max-threads` is `[resources]` in
      `.config/gate-lanes.toml`. A gate blocked on a shared lock still holds a
      slot; bounding the holders leaves slots for gates that can run.
    * `[group('cpu:<n>')]` is `threads-required = n`: a gate that runs its own
      parallel build takes that many slots of `--test-threads`.
    * A previous run's junit orders the slowest tests first (`priority`), so the
      long pole starts at once instead of last.

    Names are matched by an anchored regex over the whole test name, units
    included, so a gate's override covers exactly its own tests."""
    try:
        import tomllib
    except ModuleNotFoundError:
        import tomli as tomllib
    resources = tomllib.loads(LANES_FILE.read_text()).get("resources", {})
    groups = recipe_groups()
    secs = junit_seconds(junit)
    lines = [
        "# GENERATED by scripts/check/check-gate-lists.py --list nextest-config",
        "# (phase-543 W4). Do not edit: change the recipe attributes or",
        "# .config/gate-lanes.toml [resources].",
        "",
        "[test-groups]",
    ]
    free_kib = scratch_free_kib()
    for r, spec in sorted(resources.items()):
        n = resource_capacity(r, spec, free_kib)
        lines.append(f'"@tool:{TOOL}:{r}" = {{ max-threads = {n} }}')
    lines.append("")
    for tier in ("fast", "build", "exempt"):
        for n in b[tier]:
            g = groups.get(n, set())
            uses = sorted(x.removeprefix("uses:") for x in g if x.startswith("uses:"))
            cpus = [int(x.removeprefix("cpu:")) for x in g if x.startswith("cpu:")
                    and x.removeprefix("cpu:").isdigit()]
            if len(uses) > 1:
                raise SystemExit(f"check-gate-lists: `{n}` uses {uses}; nextest puts a "
                                 "test in ONE group, so a gate may use one resource")
            if not uses and not cpus:
                continue
            lines.append("[[profile.default.overrides]]")
            lines.append(f"filter = 'test(/^{tier}::check::{n}(#.*)?$/)'")
            if uses:
                lines.append(f'test-group = "@tool:{TOOL}:{uses[0]}"')
            if cpus:
                lines.append(f"threads-required = {max(cpus)}")
            lines.append("")
    if order is not None:
        # The order-independence check: priority by NAME, not by last run's
        # time, so the two runs it compares differ in order and in nothing else.
        for name, prio in order_priorities([n for n, _ in test_names(b)], order).items():
            if "'" in name:
                continue
            lines.append("[[profile.default.overrides]]")
            lines.append(f"filter = 'test(={name})'")
            lines.append(f"priority = {prio}")
            lines.append("")
        return "\n".join(lines)
    # Slowest first. Bucketed to whole tens of seconds so a run-to-run jitter
    # does not reorder the whole list; nextest's range is -100..=100.
    for name, t in sorted(secs.items(), key=lambda kv: (-kv[1], kv[0])):
        prio = min(100, int(t // 10))
        if prio < 1 or "'" in name:
            continue
        lines.append("[[profile.default.overrides]]")
        lines.append(f"filter = 'test(={name})'")
        lines.append(f"priority = {prio}")
        lines.append("")
    return "\n".join(lines)


def buckets() -> tuple[dict[str, list[str]], list[str]]:
    return classify(justfile_text(), recipe_tiers())


def gate_names() -> list[str]:
    """Every gate `just check <name>` can run, fast lane and build lane.

    The hook a name-set ratchet reads (issue 1071). One call, one sorted list,
    with no knowledge of where the names are written down.
    """
    b, problems = buckets()
    if problems:
        raise SystemExit(
            "check-gate-lists: cannot derive the gate set:\n" + "\n".join(problems)
        )
    return sorted(b["fast"] + b["build"])


def load_baseline() -> set[str] | None:
    if not BASELINE.exists():
        return None
    return {
        line.strip()
        for line in BASELINE.read_text(encoding="utf-8").splitlines()
        if line.strip() and not line.startswith("#")
    }


def write_baseline(names: set[str]) -> None:
    BASELINE.parent.mkdir(parents=True, exist_ok=True)
    BASELINE.write_text(
        BASELINE_HEADER + "".join(f"{n}\n" for n in sorted(names)),
        encoding="utf-8",
    )


def ratchet(present: set[str], baseline: set[str] | None) -> list[str]:
    """Names the baseline knows and the registries no longer name."""
    if baseline is None:
        return [
            f"  the baseline is missing at {BASELINE.relative_to(REPO)}.\n"
            f"      Without it a deleted gate is invisible again. Create it with\n"
            f"      `python3 scripts/check/check-gate-lists.py --write-baseline`."
        ]
    gone = sorted(baseline - present)
    if not gone:
        return []
    return [
        "  {} gate(s) in the baseline are no longer in any registry:\n{}".format(
            len(gone), "".join(f"      {n}\n" for n in gone)
        ).rstrip()
    ]


def self_test() -> None:
    # phase-543 W4: junit times read by test name; a missing or broken file is
    # no priority, never an error.
    import tempfile
    with tempfile.TemporaryDirectory(prefix="nros-gate-lists-") as t:
        j = Path(t) / "junit.xml"
        j.write_text('<testsuites><testsuite name="nros-gates::gates">'
                     '<testcase name="build::check::x" classname="nros-gates::gates" time="125.5"/>'
                     '<testcase name="fast::check::y" classname="nros-gates::gates" time="0.4"/>'
                     '</testsuite></testsuites>')
        assert junit_seconds(j) == {"build::check::x": 125.5, "fast::check::y": 0.4}, junit_seconds(j)
        assert junit_tsv(j) == "build::check::x\t0\t125500\t0\nfast::check::y\t0\t400\t0\n", junit_tsv(j)
        j.write_text("<testsuites")
        assert junit_seconds(j) == {}
        assert junit_seconds(Path(t) / "absent.xml") == {}
    # phase-543 — the order check's two orders are exact opposites, and use
    # the whole priority range.
    asc, desc = order_priorities(["a", "b", "c"], "asc"), order_priorities(["a", "b", "c"], "desc")
    assert asc == {"a": 100, "b": 0, "c": -100}, asc
    assert desc == {"a": -100, "b": 0, "c": 100}, desc
    assert order_priorities(["x"], "asc") == {"x": 0}
    # issue 2356 — a scratch-sized resource admits what TMPDIR holds, at least
    # one, and one when free space cannot be measured; a count is as given.
    gib = 1024 * 1024
    assert resource_capacity("s", {"scratch-gib": 5}, 48 * gib) == 9
    assert resource_capacity("s", {"scratch-gib": 5}, 4 * gib) == 1
    assert resource_capacity("s", {"scratch-gib": 5}, None) == 1
    assert resource_capacity("s", {"scratch-gib": 2.5}, 10 * gib) == 4
    assert resource_capacity("r", 2, None) == 2
    for bad in (0, True, {"scratch-gib": 0}, {"scratch-gib": "5"}, {"gib": 5}, "2"):
        try:
            resource_capacity("x", bad, 48 * gib)
        except SystemExit:
            continue
        raise AssertionError(f"selftest: accepted [resources] x = {bad!r}")
    # Both directions, on synthetic input. `check-gate-selftests` requires
    # this on the normal path: a control nobody runs decays into a comment.
    mini = "\n".join([
        "NIGHTLY := `true`",
        "",
        "[private]",
        "alpha-gate:",
        "    @true",
        "",
        "[private]",
        "beta-gate:",
        "    @true",
        "",
        "[private]",
        "heavy-gate:",
        "    @true",
        "",
        "[private]",
        "some-verb:",
        "    @true",
        "",
        "[private]",
        'stack-elf elf top="30":',
        "    @true",
    ])
    t = {"heavy-gate": ["build"], "some-verb": ["exempt", "a verb"]}
    b, problems = classify(mini, t)
    assert problems == [], f"selftest: rejected a well-formed tree: {problems}"
    assert b["fast"] == ["alpha-gate", "beta-gate"], b["fast"]
    assert b["build"] == ["heavy-gate"], b["build"]
    assert b["exempt"] == ["some-verb"], b["exempt"]
    assert b["parameterized"] == ["stack-elf"], b["parameterized"]

    # A NEW recipe joins the fast lane with nothing else to edit.
    plus = mini + "\n\n[private]\ngamma-gate:\n    @true\n"
    assert classify(plus, t)[0]["fast"] == ["alpha-gate", "beta-gate", "gamma-gate"], (
        "selftest: a new recipe did not become a fast gate"
    )
    # A reason is mandatory.
    assert any("no reason" in p for p in classify(mini, {**t, "beta-gate": ["exempt"]})[1]), (
        "selftest: accepted an exemption with no reason"
    )
    assert any("no reason" in p for p in classify(mini, {**t, "beta-gate": ["exempt", " "]})[1]), (
        "selftest: accepted a blank reason"
    )
    # An unknown tier.
    assert any("is not one of" in p for p in classify(mini, {**t, "beta-gate": ["slow"]})[1]), (
        "selftest: accepted an unknown tier"
    )
    # A parameterized recipe cannot carry one.
    assert any("declares parameters" in p
               for p in classify(mini, {**t, "stack-elf": ["build"]})[1]), (
        "selftest: accepted a tier on a parameterized recipe"
    )
    # Everything tiered away: an empty fast lane is refused.
    assert any("EMPTY" in p for p in classify(mini, {
        "alpha-gate": ["build"], "beta-gate": ["build"], "heavy-gate": ["build"],
        "some-verb": ["build"]})[1]), "selftest: accepted an empty fast lane"

    # --- the deletion ratchet (issue 1071) -------------------------------
    # Carried through the 1072 rewrite. What changed is only WHERE the present
    # set comes from — a derived classification instead of a registry line —
    # so these cases are about the ratchet itself and stay as they were.
    #
    # A deletion, which every other check here accepts: sorted, one per line,
    # terminated, and missing `beta`.
    assert ratchet({"alpha", "gamma"}, {"alpha", "beta", "gamma"}), (
        "selftest: a gate that left the set was not reported"
    )
    # Growth is free — that is the whole point of a ratchet.
    assert ratchet({"alpha", "beta", "delta"}, {"alpha", "beta"}) == [], (
        "selftest: an ADDED gate was reported as a problem"
    )
    # A delete-plus-add netting to ZERO: #431's exact shape, and the case a
    # count ratchet cannot see.
    assert ratchet({"alpha", "delta"}, {"alpha", "beta"}), (
        "selftest: one added and one removed netted out and passed"
    )
    # A missing baseline fails, or the ratchet disappears the moment someone
    # deletes the file.
    assert ratchet({"alpha"}, None), "selftest: a missing baseline passed"

def main(argv: list[str]) -> int:
    if argv[:1] == ["--junit-tsv"]:
        print(junit_tsv(Path(argv[1])), end="")
        return 0
    b, problems = buckets()

    if "--write-baseline" in argv:
        if problems:
            print("check-gate-lists: refusing to write a baseline over a broken "
                  "classification:", file=sys.stderr)
            print("\n".join(problems), file=sys.stderr)
            return 1
        present = set(gate_names())
        write_baseline(present)
        print(
            f"check-gate-lists: wrote {len(present)} gate name(s) to "
            f"{BASELINE.relative_to(REPO)}.\n"
            "Say in the commit message which gate was retired and why — this "
            "file exists\nbecause a deletion nobody meant to make passed every "
            "gate (issue 1071)."
        )
        return 0

    if argv and argv[0] == "--list":
        # The runners' one source of truth. `fast-serial`/`build-serial` are
        # accepted as well as the verbs, because `NROS_GATE_LANE` is spelled
        # with the suffix and predates this.
        if problems:
            print("check-gate-lists: refusing to emit a list over a broken registry:",
                  file=sys.stderr)
            print("\n".join(problems), file=sys.stderr)
            return 1
        lane = (argv[1] if len(argv) > 1 else "fast").removesuffix("-serial")
        if lane == "tests":
            # `--list tests [--lane <lane>]`: one line per test, `<name>` when
            # in scope, `<name>\t<reason>` when not.
            lane_name = argv[argv.index("--lane") + 1] if "--lane" in argv else None
            for name, why in test_names(b, lane_name or None):
                print(name if why is None else f"{name}\t{why}")
            return 0
        if lane == "nextest-config":
            # `--list nextest-config [--junit <path>]`: the harness's tool config.
            junit = Path(argv[argv.index("--junit") + 1]) if "--junit" in argv else None
            order = argv[argv.index("--order") + 1] if "--order" in argv else None
            if order not in (None, "asc", "desc"):
                print(f"check-gate-lists: --order is asc or desc, not `{order}`", file=sys.stderr)
                return 2
            print(nextest_config(b, junit, order))
            return 0
        if lane == "all":
            names = sorted(b["fast"] + b["build"])
        elif lane in ("fast", "build"):
            names = b[lane]
        else:
            print(f"check-gate-lists: unknown lane `{lane}`", file=sys.stderr)
            return 2
        print("\n".join(names))
        return 0

    # The deletion ratchet (issue 1071) reads the DERIVED set, not a registry
    # line — that is the whole of its 1072 rebase. A gate can now only leave by
    # having its recipe deleted, and this is what notices.
    if not problems:
        problems = ratchet(set(b["fast"] + b["build"]), load_baseline())

    if problems:
        print("check-gate-lists: the gate classification does not hold\n")
        print("\n".join(problems))
        print(
            "\nA recipe in just/check.just is a FAST gate unless it carries\n"
            "`[metadata(\"tier\", \"build\")]`, `[metadata(\"tier\", \"exempt\", \"<why>\")]`,\n"
            "or declares parameters. There is deliberately no gate list to\n"
            "append to: a shared authored list is the\n"
            "conflict issues 0883/0884/1072 measured, and no merge driver can\n"
            "fix it because GitHub rebases queue entries server-side."
        )
        return 1

    print(
        f"check-gate-lists OK — {len(b['fast'])} fast gate(s) derived, "
        f"{len(b['build'])} build-tier, "
        f"{len(b['exempt'])} exempt with a reason, "
        f"{len(b['parameterized'])} parameterized recipe(s) excluded."
    )
    return 0


if __name__ == "__main__":
    self_test()
    sys.exit(main(sys.argv[1:]))

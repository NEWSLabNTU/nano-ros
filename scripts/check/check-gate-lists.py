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

    a recipe in `just/check.just` is a FAST-LANE GATE unless it is listed in
    `build-serial:` (it needs something built), named in
    `.config/gate-lane-exempt.txt` (with a reason), or declares parameters
    (a gate is invoked as `just check <name>` with no arguments, so a recipe
    that requires one cannot be one).

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

`build-serial:` stays an authored dependency list. It holds ~20 names, none of
the 13 registry additions across the pull requests open on 2026-09-05 went to
it, and `scripts/check-gate-visibility.py` reads its members directly.

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

EXEMPT_FILE = REPO / ".config" / "gate-lane-exempt.txt"
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

# The one lane whose membership is still authored. A build gate is the
# EXCEPTION — it cannot run without something compiled — so it is listed, and
# `check-gate-visibility` reads this same list to decide which gates no
# merge-gating job reaches.
BUILD_REGISTRY = "build-serial"

# The no-op `build-serial:` ends on, so the LAST real gate carries a trailing
# backslash like every other. Without it the final entry is the one line with
# different syntax, and a gate that sorts last must edit someone else's line to
# add the backslash — two such PRs conflict with CERTAINTY rather than by bad
# luck. The "trailing comma" fix.
SENTINEL = "_gate-list-end"

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


def dep_block(lines: list[str], name: str) -> tuple[int, int]:
    """(start, end) of `name:`'s dependency lines, end EXCLUSIVE.

    A recipe's dependencies run to the first line that does not end in a
    backslash — that last line is a dependency too, not the body.
    """
    for i, line in enumerate(lines):
        if line.startswith(f"{name}:"):
            j = i
            while lines[j].rstrip().endswith("\\"):
                j += 1
                if j >= len(lines):
                    raise SystemExit(f"{name}: unterminated continuation")
            return i, j + 1
    raise SystemExit(f"check-gate-lists: no recipe `{name}` in {JUSTFILE}")


def names_per_line(lines: list[str], s: int, e: int) -> list[list[str]]:
    """The dependency names, grouped by the line they sit on."""
    out = []
    for k in range(s, e):
        text = lines[k].strip().rstrip("\\").strip()
        if k == s:
            text = text.split(":", 1)[1]
        out.append([n for n in text.split() if n])
    return out


def parse_exempt(text: str) -> tuple[list[str], list[str]]:
    """(names, problems). Every entry needs a name and a `# reason`."""
    names: list[str] = []
    problems: list[str] = []
    for lineno, raw in enumerate(text.split("\n"), 1):
        line = raw.strip()
        if not line or line.startswith("#"):
            continue
        name, sep, reason = line.partition("#")
        name = name.strip()
        if " " in name or not re.fullmatch(r"[a-z_][a-z0-9_-]*", name or ""):
            problems.append(
                f"  {EXEMPT_FILE.name}:{lineno}: `{line}` is not `<name>  # reason`"
            )
            continue
        if not sep or not reason.strip():
            problems.append(
                f"  {EXEMPT_FILE.name}:{lineno}: `{name}` has no reason.\n"
                f"      Say why it is not a fast gate; a bare name is how a\n"
                f"      temporary exclusion becomes permanent."
            )
        names.append(name)
    if names != sorted(names):
        first = next((a for a, b in zip(names, sorted(names)) if a != b), "?")
        problems.append(
            f"  {EXEMPT_FILE.name}: not sorted (first out of order: `{first}`)."
        )
    dupes = sorted({n for n in names if names.count(n) > 1})
    if dupes:
        problems.append(f"  {EXEMPT_FILE.name}: listed twice: {', '.join(dupes)}")
    return names, problems


def check_registry(lines: list[str], name: str) -> tuple[list[str], list[str]]:
    """(gate names, problems) for an authored dependency-line registry."""
    s, e = dep_block(lines, name)
    per_line = names_per_line(lines, s, e)
    flat = [n for group in per_line for n in group]
    problems = []

    # The sentinel is REQUIRED and must be last. Checked before it is dropped:
    # if it drifts into the middle it stops terminating anything, and the last
    # real gate silently becomes the odd line again.
    if flat and flat[-1] == SENTINEL:
        flat = flat[:-1]
        per_line = per_line[:-1]
    elif SENTINEL in flat:
        problems.append(
            f"  {name}: `{SENTINEL}` is present but not LAST.\n"
            f"      It exists to give the final real gate a trailing backslash;\n"
            f"      anywhere else it terminates nothing."
        )
        flat = [n for n in flat if n != SENTINEL]
        per_line = [[n for n in g if n != SENTINEL] for g in per_line]
        per_line = [g for g in per_line if g]
    else:
        problems.append(
            f"  {name}: missing the `{SENTINEL}` terminator.\n"
            f"      End the list with it so every real gate's line looks the\n"
            f"      same; otherwise a gate that sorts last has to edit the\n"
            f"      previous line too, and two such PRs always conflict."
        )

    crowded = sum(1 for group in per_line if len(group) > 1)
    if crowded:
        problems.append(
            f"  {name}: {crowded} line(s) carry more than one gate.\n"
            f"      Two agents adding unrelated gates then edit the SAME line and\n"
            f"      git conflicts on changes that do not overlap in meaning."
        )

    if flat != sorted(flat):
        first = next((a for a, b in zip(flat, sorted(flat)) if a != b), "?")
        problems.append(
            f"  {name}: not sorted (first out of order: `{first}`).\n"
            f"      Sorted order is what makes a new gate's line position\n"
            f"      predictable, and so unlikely to abut someone else's."
        )

    dupes = sorted({n for n in flat if flat.count(n) > 1})
    if dupes:
        problems.append(f"  {name}: listed twice: {', '.join(dupes)}")

    # Issue 1882 -- the registry is a LIST, and its body never runs: the runner
    # reads the names and calls `just check <gate>` for each. `@just linux
    # check` sat in this body for six weeks after #0993 and linted nothing.
    # A command that must run is a gate on the list, not a line under it.
    k = e
    dead = []
    while k < len(lines) and (lines[k].startswith((" ", "\t")) or not lines[k].strip()):
        t = lines[k].strip()
        if t and not t.startswith("#"):
            dead.append(t)
        k += 1
    if dead:
        problems.append(
            f"  {name}: has a recipe body ({len(dead)} line(s), first: `{dead[0]}`).\n"
            f"      The runner derives its work from the dependency NAMES and never\n"
            f"      runs this body, so nothing under the list executes. Make the\n"
            f"      command a gate recipe and list it (issue 1882)."
        )

    return flat, problems


def classify(just_text: str, exempt_text: str) -> tuple[dict[str, list[str]], list[str]]:
    """Split every recipe into fast / build / exempt / parameterized.

    Returns (buckets, problems). This is the whole contract: a recipe lands in
    exactly one bucket, and the fast lane is whatever is left over.
    """
    lines = just_text.split("\n")
    rec = recipes(lines)
    build, problems = check_registry(lines, BUILD_REGISTRY)
    exempt, ex_problems = parse_exempt(exempt_text)
    problems += ex_problems

    parameterized = sorted(n for n, has_args in rec.items() if has_args)
    plain = {n for n, has_args in rec.items() if not has_args}

    ghost_build = [n for n in build if n not in rec]
    if ghost_build:
        problems.append(
            f"  {BUILD_REGISTRY}: names with no recipe: {', '.join(sorted(ghost_build))}.\n"
            f"      `just check <name>` would fail; delete the entry with the recipe."
        )
    ghost_exempt = [n for n in exempt if n not in rec]
    if ghost_exempt:
        problems.append(
            f"  {EXEMPT_FILE.name}: names with no recipe: "
            f"{', '.join(sorted(ghost_exempt))}.\n"
            f"      A stale exemption is a note nobody can act on — and the next\n"
            f"      recipe to take that name inherits it silently."
        )
    both = sorted(set(build) & set(exempt))
    if both:
        problems.append(
            f"  in `{BUILD_REGISTRY}` AND exempt: {', '.join(both)}.\n"
            f"      A gate has one home; pick the lane or the exemption."
        )
    args_claimed = sorted((set(build) | set(exempt)) & set(parameterized))
    if args_claimed:
        problems.append(
            f"  declares parameters but is listed as a gate/exemption: "
            f"{', '.join(args_claimed)}.\n"
            f"      A recipe with arguments can never run as `just check <name>`,\n"
            f"      so it is excluded structurally and needs no entry."
        )

    fast = sorted(plain - set(build) - set(exempt))
    if not fast:
        problems.append(
            "  the derived FAST lane is EMPTY.\n"
            "      Refusing to report OK over nothing — the runner would too."
        )
    return (
        {
            "fast": fast,
            "build": sorted(build),
            "exempt": sorted(exempt),
            "parameterized": parameterized,
        },
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


def nextest_config(b: dict[str, list[str]], junit: Path | None = None) -> str:
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
    for r, n in sorted(resources.items()):
        lines.append(f'"@tool:{TOOL}:{r}" = {{ max-threads = {int(n)} }}')
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
    return classify(
        justfile_text(),
        EXEMPT_FILE.read_text(encoding="utf-8") if EXEMPT_FILE.is_file() else "",
    )


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
        j.write_text("<testsuites")
        assert junit_seconds(j) == {}
        assert junit_seconds(Path(t) / "absent.xml") == {}
    """Both directions, on synthetic input.

    `check-gate-selftests` requires this on the normal path: a control nobody
    runs decays into a comment.
    """
    good = [f"{BUILD_REGISTRY}: \\", "    alpha \\", "    beta \\", "    gamma \\",
            f"    {SENTINEL}"]
    assert check_registry(good, BUILD_REGISTRY)[1] == [], "selftest: rejected a well-formed list"

    # Issue 1882: a body on the registry is code the runner never reaches.
    with_body = good + ["    @just linux check"]
    assert any("body" in p for p in check_registry(with_body, BUILD_REGISTRY)[1]), (
        "selftest: missed a registry body"
    )
    commented = good + ["    # a comment is not a command"]
    assert check_registry(commented, BUILD_REGISTRY)[1] == [], (
        "selftest: refused a comment under the registry"
    )

    crowded = [f"{BUILD_REGISTRY}: \\", "    alpha beta \\", "    gamma \\",
               f"    {SENTINEL}", "    @echo hi"]
    assert any("more than one gate" in p for p in check_registry(crowded, BUILD_REGISTRY)[1]), (
        "selftest: missed two names sharing a line"
    )

    unsorted = [f"{BUILD_REGISTRY}: \\", "    gamma \\", "    alpha \\",
                f"    {SENTINEL}", "    @echo hi"]
    assert any("not sorted" in p for p in check_registry(unsorted, BUILD_REGISTRY)[1]), (
        "selftest: missed an out-of-order list"
    )

    dup = [f"{BUILD_REGISTRY}: \\", "    alpha \\", "    alpha \\",
           f"    {SENTINEL}", "    @echo hi"]
    assert any("listed twice" in p for p in check_registry(dup, BUILD_REGISTRY)[1]), (
        "selftest: missed a duplicate"
    )

    no_sentinel = [f"{BUILD_REGISTRY}: \\", "    alpha \\", "    beta", "    @echo hi"]
    assert any("missing the" in p for p in check_registry(no_sentinel, BUILD_REGISTRY)[1]), (
        "selftest: missed an absent terminator"
    )

    misplaced = [f"{BUILD_REGISTRY}: \\", f"    {SENTINEL} \\", "    alpha \\",
                 "    beta", "    @echo hi"]
    assert any("not LAST" in p for p in check_registry(misplaced, BUILD_REGISTRY)[1]), (
        "selftest: missed a misplaced terminator"
    )
    assert not any("not sorted" in p for p in check_registry(good, BUILD_REGISTRY)[1]), (
        "selftest: the sentinel was sorted as if it were a gate"
    )

    body = [f"{BUILD_REGISTRY}: \\", "    alpha", "    zzz-not-a-dep", "    @echo hi"]
    assert dep_block(body, BUILD_REGISTRY) == (0, 2), "selftest: block boundary wrong"

    # --- the derivation itself -------------------------------------------
    # A miniature justfile: two plain recipes, one build gate, one exempt
    # verb, one parameterized helper.
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
        f"{BUILD_REGISTRY}: \\",
        "    heavy-gate \\",
        f"    {SENTINEL}",
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
        f"{SENTINEL}:",
        "",
        "[private]",
        'stack-elf elf top="30":',
        "    @true",
    ])
    ex = f"{SENTINEL}  # terminator\n{BUILD_REGISTRY}  # the registry\nsome-verb  # a verb\n"
    b, problems = classify(mini, ex)
    assert problems == [], f"selftest: rejected a well-formed tree: {problems}"
    assert b["fast"] == ["alpha-gate", "beta-gate"], b["fast"]
    assert b["build"] == ["heavy-gate"], b["build"]
    assert b["parameterized"] == ["stack-elf"], b["parameterized"]

    # A NEW recipe joins the fast lane with no list to edit — the whole point.
    plus = mini + "\n\n[private]\ngamma-gate:\n    @true\n"
    assert classify(plus, ex)[0]["fast"] == ["alpha-gate", "beta-gate", "gamma-gate"], (
        "selftest: a new recipe did not become a fast gate"
    )

    # A reason is mandatory.
    assert any("has no reason" in p for p in classify(mini, ex + "orphan\n")[1]), (
        "selftest: accepted an exemption with no reason"
    )
    # An exemption naming nothing.
    assert any("no recipe" in p for p in classify(mini, ex + "ghost  # gone\n")[1]), (
        "selftest: accepted an exemption for a recipe that does not exist"
    )
    # Exempt AND in the build registry.
    assert any("AND exempt" in p for p in classify(mini, ex + "heavy-gate  # both\n")[1]), (
        "selftest: accepted a gate claimed twice"
    )
    # A parameterized recipe cannot be listed.
    assert any("declares parameters" in p
               for p in classify(mini, ex + "stack-elf  # nope\n")[1]), (
        "selftest: accepted an entry for a parameterized recipe"
    )
    # The ledger must be sorted, like the registry.
    assert any("not sorted" in p
               for p in classify(mini, f"some-verb  # v\n{SENTINEL}  # t\n")[1]), (
        "selftest: missed an unsorted exemption ledger"
    )


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
            print(nextest_config(b, junit))
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
            "\nA recipe in just/check.just is a FAST gate unless it is in\n"
            "`build-serial:`, named in .config/gate-lane-exempt.txt with a\n"
            "reason, or declares parameters. There is deliberately no\n"
            "fast-lane registry to append to: a shared authored list is the\n"
            "conflict issues 0883/0884/1072 measured, and no merge driver can\n"
            "fix it because GitHub rebases queue entries server-side."
        )
        return 1

    print(
        f"check-gate-lists OK — {len(b['fast'])} fast gate(s) derived, "
        f"{len(b['build'])} in `{BUILD_REGISTRY}` (one per line and sorted), "
        f"{len(b['exempt'])} exempt with a reason, "
        f"{len(b['parameterized'])} parameterized recipe(s) excluded."
    )
    return 0


if __name__ == "__main__":
    self_test()
    sys.exit(main(sys.argv[1:]))

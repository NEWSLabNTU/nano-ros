#!/usr/bin/env python3
"""check-config-names -- every configuration name follows the naming rule (RFC-0140 D13 G1).

phase-581 W4. The rule is RFC-0140 D3's N1 to N8: a two-segment key under an
owner word, every other spelling mechanical from it, a unit in the name, no
build name that is also a run name, one `NROS_` prefix, no reused retired
spelling, no resource variable. The catalog loader enforces it on every
package it loads (`nros_knob_catalog::naming`), so a downstream package with a
bad name fails in its user's build; this gate runs the same rules, buildless,
over every in-tree contributor:

  * the merged in-tree registry (core's knobs.toml and each package's
    nros-knobs.toml, through scripts/lib/knob_registry.py),
  * every other tracked nros-knobs.toml (the test fixtures' packages),
  * every hand-written `config NROS_*` in zephyr/Kconfig outside the generated
    knob section.

The baseline is `packages/tooling/nros-knob-catalog/config-names.toml`'s
`[[baseline]]`: RFC-0140 Appendix A, which may only shrink. The gate fails on
a violation no entry holds, on an entry (or one rule of one) that no longer
applies, and on an entry count that is not the file's `baseline_entries`, so
growing the baseline is a visible edit of that number. A self-test plants one
violation of each rule in a temporary catalog first, and runs the data file's
`[[case]]` corpus, the one the Rust loader's tests also run.
"""

from __future__ import annotations

import subprocess
import sys
import tempfile
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "lib"))
import config_names as cn  # noqa: E402
import knob_registry as kr  # noqa: E402

REPO = cn.REPO
# RFC-0140 Appendix A's count. An entry beyond it is `appendix = "missed"`.
APPENDIX_A = 88


def tracked_package_files(known):
    """Tracked nros-knobs.toml files the merged registry does not hold."""
    out = subprocess.run(["git", "-C", str(REPO), "ls-files", "*nros-knobs.toml"],
                         capture_output=True, text=True, check=True).stdout.split()
    known = {Path(p).resolve() for p in known}
    return [REPO / f for f in out if (REPO / f).resolve() not in known]


def package_subjects(path: Path):
    """(owner, subjects, env names, retired spellings) of one package file."""
    doc = kr.read(path)
    owner = doc.get("owner")
    errors = []
    knobs, _, envs, _ = kr._parse_file(doc, kr._rel(path), {}, owner, errors)
    if errors:
        raise ValueError(f"{kr._rel(path)}: " + "; ".join(errors))
    subs = [cn._knob_subject(k, kr._rel(path)) for k in knobs]
    subs += [cn._env_subject(e, kr._rel(path)) for e in envs]
    retired = {e["name"] for row in knobs + envs for e in row["retired_entries"]}
    return owner, subs, {e["name"] for e in envs}, retired


def in_tree(rules):
    """(findings, row names) of every in-tree contributor."""
    reg = kr.registry()
    subjects = cn.registry_subjects(reg)
    extra = [package_subjects(p) for p in tracked_package_files(p["file"] for p in reg["packages"])]
    ctx = cn.registry_context(reg, cn.resource_vars(), extra_owners=[o for o, *_ in extra])
    for _, subs, envs, retired in extra:
        subjects += subs
        ctx["env_names"] |= envs
        ctx["retired"] |= retired
    live = {v for s in subjects for _, v in s["spellings"]}
    subjects += cn.kconfig_subjects(cn.KCONFIG.read_text(), live)
    rows = {s["subject"] for s in subjects if s["kind"] != "kconfig"}
    return cn.findings(subjects, ctx, rules), rows


def count_problems(rules, ceiling):
    base = rules["baseline"]
    out = []
    if len(base) != ceiling:
        out.append(f"the baseline holds {len(base)} entries but `baseline_entries = {ceiling}`: "
                   "it only shrinks, so lower the number with every entry you remove (and never "
                   "raise it to admit a new name)")
    appendix = [e for e in base if e["appendix"] != "missed"]
    if len(appendix) > APPENDIX_A:
        out.append(f"{len(appendix)} entries name an Appendix A section, which lists {APPENDIX_A}")
    for e in base:
        if e["appendix"] == "missed" and not str(e.get("why", "")).strip():
            out.append(f"`{e['subject']}`: a `missed` entry says `why` Appendix A lacks it")
    return out


# --- self-test -------------------------------------------------------------------

PLANTED_CORE = '''
owner = "core"

[owners]
trace = "the planted owner"

[[knob]]
key = "param.thing"

[[knob]]
key = "trace.deep.name"

[[knob]]
key = "trace.dir"

[[knob]]
key = "trace.level"

[[knob]]
key = "trace.old"

[[knob]]
key = "trace.renamed"
retired = [{ spelling = "env:NROS_TRACE_OLD" }]

[[knob]]
key = "trace.ring_size"

[[env]]
name = "NROS_PLANTED_FOREIGN"
foreign = "ros2"
disposition = "ignore"

[[env]]
key = "trace.level"
disposition = "listed"
'''
PLANTED_KCONFIG = "config NROS_EPOCH_PLANTED\n\tbool\n"
PLANTED = {  # rule -> the row that breaks it
    "N1": "knob `trace.deep.name`",
    "N3": "knob `param.thing`",
    "N4": "knob `trace.ring_size`",
    "N5": "knob `trace.level`",
    "N6": "env `NROS_PLANTED_FOREIGN`",
    "N7": "knob `trace.old`",
    "N8": "knob `trace.dir`",
}


def self_test(rules) -> list:
    fails = []
    # The corpus the Rust loader's tests run too.
    ctx = cn.case_context(rules)
    for c in rules["case"]:
        got = [r for r, _ in cn.violations(cn.case_subject(c), ctx, rules)]
        if got != c["expect"]:
            fails.append(f"[[case]] {c.get('name') or c.get('key')}: found {got}, expected {c['expect']}")
    # One violation of each rule, planted in a temporary catalog, each named
    # with its row and rule.
    with tempfile.TemporaryDirectory() as tmp:
        core = Path(tmp) / "knobs.toml"
        core.write_text(PLANTED_CORE)
        reg, _c3 = kr.parse(path=core, packages=[])  # N5/N7 also break C3; the lint names its rule
        # N2 cannot be planted in a registry any more: every spelling derives
        # from `key`, and the per-row override that could break it is refused
        # (phase-581 W7). The `[[case]]` corpus above still holds the rule.
        core.write_text('owner = "core"\n\n[owners]\ntrace = "x"\n\n[[knob]]\nkey = "trace.symbol"\n'
                        'transitional = { kconfig = "NROS_TRACE_SYM" }\n')
        _, errs = kr.parse(path=core, packages=[])
        if not any("`transitional` was deleted" in e for e in errs):
            fails.append(f"a per-row spelling override was not refused ({errs})")
    subjects = cn.registry_subjects(reg, core_where="planted/knobs.toml")
    subjects += cn.kconfig_subjects(PLANTED_KCONFIG, set(), where="planted/Kconfig")
    pctx = cn.registry_context(reg, {"NROS_TRACE_DIR"})
    found = cn.findings(subjects, pctx, rules)
    new, fixed = cn.compare(found, [], set())
    for rule, row in {**PLANTED, "N3 (Kconfig)": "kconfig `CONFIG_NROS_EPOCH_PLANTED`"}.items():
        r = rule.split()[0]
        if not any(line.startswith(row) and f": {r} - " in line for line in new):
            fails.append(f"planted {rule} on {row}: the gate did not name it ({new})")
    if fixed:
        fails.append(f"an empty baseline reported fixed entries: {fixed}")
    # The baseline both ways: a held violation passes, a fixed one fails, a
    # probe that finds nothing fails.
    held = [{"kind": "knob", "subject": "trace.ring_size", "rules": ["N4"]},
            {"kind": "knob", "subject": "trace.clean", "rules": ["N4"]},
            {"kind": "read", "subject": "PLANTED_GONE", "rules": ["N6"], "why": "x"}]
    new, fixed = cn.compare(found, held, set(), present=lambda e, rows: False)
    if any(line.startswith("knob `trace.ring_size`") for line in new):
        fails.append("a violation the baseline holds was reported as new")
    for want in ("knob `trace.clean`: N4 no longer applies", "read `PLANTED_GONE`"):
        if not any(line.startswith(want) for line in fixed):
            fails.append(f"a fixed baseline entry was not reported: {want} ({fixed})")
    return fails


def main() -> int:
    try:
        rules = cn.load()
    except ValueError as e:
        print(f"check-config-names: {e}", file=sys.stderr)
        return 1
    fails = self_test(rules)
    if fails:
        print("check-config-names: SELF-TEST FAILED", file=sys.stderr)
        for f in fails:
            print(f"  {f}", file=sys.stderr)
        return 1
    ceiling = rules["baseline_entries"]
    found, rows = in_tree(rules)
    new, fixed = cn.compare(found, rules["baseline"], rows)
    problems = count_problems(rules, ceiling)
    if new or fixed or problems:
        print("check-config-names: FAILED (RFC-0140 D3 naming rule; baseline "
              f"{cn.DATA.relative_to(REPO)})", file=sys.stderr)
        for line in new:
            print(f"  NEW    {line}", file=sys.stderr)
        for line in fixed:
            print(f"  FIXED  {line}", file=sys.stderr)
        for line in problems:
            print(f"  COUNT  {line}", file=sys.stderr)
        if new:
            print("  A new name follows the rule: rename it (RFC-0140 D3), never add it to the "
                  "baseline.", file=sys.stderr)
        return 1
    n = len(rules["baseline"])
    print(f"check-config-names: OK ({len(found)} rows break the rule, all in the baseline of {n}; "
          f"self-test planted N1 and N3-N8, refused an N2 override and ran {len(rules['case'])} cases)")
    return 0


if __name__ == "__main__":
    sys.exit(main())

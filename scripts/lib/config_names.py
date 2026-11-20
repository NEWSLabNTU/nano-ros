"""The naming rule's lint (RFC-0140 D3 N1-N8, D13 G1; phase-581 W4).

The rule's DATA and its baseline are one file,
`packages/tooling/nros-knob-catalog/config-names.toml`, read here and by the
Rust catalog loader (`nros_knob_catalog::naming`), which applies the same rules
when it loads a package so a downstream package with a bad name fails in the
user's own build. The rule logic is written once per language; the data file's
`[[case]]` corpus is run by both (`self_test()` here, `naming_cases_*` in the
crate's tests), which is what holds the two together.

A SUBJECT is one thing the rule judges, as a dict:

    kind       "knob" | "statement" | "env" | "kconfig"
    subject    its identity: a knob's or statement's key, an env row's
               variable, a hand Kconfig symbol (`CONFIG_NROS_...`)
    key        a knob's or keyed env row's key (else None)
    spellings  [(road, name)] of every LIVE spelling (road: toml, env,
               kconfig, cmake, run)
    foreign    the variable is another project's (never renamed)
    where      a file label, for messages

`violations(subject, ctx)` returns [(rule, why)], `ctx` holding the owner words,
the env rows' names (N5), every retired spelling (N7) and every resource
variable (N8).
"""

from __future__ import annotations

import re
from pathlib import Path

import git_grep

try:
    import tomllib
except ModuleNotFoundError:  # Python < 3.11
    import tomli as tomllib

REPO = Path(__file__).resolve().parents[2]
DATA = REPO / "packages/tooling/nros-knob-catalog/config-names.toml"
SDK_INDEX = REPO / "nros-sdk-index.toml"
KCONFIG = REPO / "zephyr/Kconfig"
KCONFIG_BEGIN = "# >>> knob registry >>>"
KCONFIG_END = "# <<< knob registry <<<"

RULES = ("N1", "N2", "N3", "N4", "N5", "N6", "N7", "N8")
WORD = re.compile(r"^[a-z][a-z0-9_]*$")
# Where a `read` / `build-read` probe looks for a name still in use, and what
# it never counts: the registry itself, its generated views and this lint's data.
PROBE_PATHS = ("packages", "cmake", "zephyr", "integrations")
PROBE_EXCLUDE = (
    "packages/tooling/nros-zephyr-build/knobs.toml",
    "packages/tooling/nros-zephyr-build/src/knob_registry.rs",
    "packages/tooling/nros-zephyr-build/src/config_rows.rs",
    "packages/tooling/nros-zephyr-build/src/registry_packages.rs",
    "packages/tooling/nros-knob-catalog/config-names.toml",
    "packages/api/nros/src/retired_env.rs",
    "cmake/NanoRosKnobRegistry.cmake",
    "zephyr/Kconfig.knob-prompts",
)
BUILD_READ = (r'option_env!\(\s*"{n}"', r'(?<![_A-Za-z])env!\(\s*"{n}"',
              r"rerun-if-env-changed={n}\b", r"\$ENV\{{{n}\}}")
COMMENT = re.compile(r"^\s*(//|#|\*|/\*)")


def derive_id(key: str) -> str:
    return key.upper().replace(".", "_")


def load(path: Path = DATA, text: str | None = None) -> dict:
    """The data file, validated: `measure`, `banned` {ending: reason},
    `allow` {term: reason}, `framework` {symbol: reason}, `baseline` [entry],
    `case` [case], `case_context`."""
    doc = tomllib.loads(text) if text is not None else tomllib.loads(path.read_text())
    units = doc.get("units", {})
    errors = []
    banned = {}
    for b in units.get("banned", []):
        if not b.get("ending") or not str(b.get("reason", "")).strip():
            errors.append(f"[[units.banned]] {b!r}: `ending` and a non-empty `reason`")
        banned[b.get("ending")] = b.get("reason")
    allow = {}
    for a in units.get("allow", []):
        if not a.get("term") or not str(a.get("reason", "")).strip():
            errors.append(f"[[units.allow]] {a!r}: `term` and a non-empty `reason`")
        allow[a.get("term")] = a.get("reason")
    framework = {}
    for f in doc.get("framework", []):
        if not f.get("symbol") or not str(f.get("reason", "")).strip():
            errors.append(f"[[framework]] {f!r}: `symbol` and a non-empty `reason`")
        framework[f.get("symbol")] = f.get("reason")
    baseline = doc.get("baseline", [])
    seen = set()
    for e in baseline:
        s = e.get("subject")
        if not s or e.get("kind") not in ("knob", "statement", "env", "kconfig", "read", "build-read"):
            errors.append(f"[[baseline]] {e!r}: `subject` and `kind` (knob, statement, env, kconfig, "
                          "read, build-read)")
        if (e.get("kind"), s) in seen:
            errors.append(f"[[baseline]] `{s}` is listed twice")
        seen.add((e.get("kind"), s))
        rules = e.get("rules", [])
        if not rules or any(r not in RULES for r in rules) or rules != sorted(set(rules)):
            errors.append(f"[[baseline]] `{s}`: `rules` is a sorted list of {list(RULES)}")
        if e.get("probe") not in (None, "row") or (e.get("probe") == "row" and e.get("kind") not in
                                                     ("knob", "statement", "env")):
            errors.append(f"[[baseline]] `{s}`: `probe = \"row\"` is the only probe a row entry takes")
        if (e.get("probe") or e.get("kind") in ("read", "build-read")) and not str(e.get("why", "")).strip():
            errors.append(f"[[baseline]] `{s}`: an entry the lint cannot see states `why`")
        if not e.get("appendix"):
            errors.append(f"[[baseline]] `{s}`: `appendix` names its RFC-0140 Appendix A section")
    if not isinstance(doc.get("baseline_entries"), int):
        errors.append("`baseline_entries` states how many [[baseline]] entries there are")
    if errors:
        raise ValueError("config-names.toml: " + "; ".join(errors))
    return {"baseline_entries": doc.get("baseline_entries"), "measure": set(units.get("measure", [])), "banned": banned, "allow": allow,
            "framework": framework, "baseline": baseline, "case": doc.get("case", []),
            "case_context": doc.get("case_context", {})}


def resource_vars(path: Path = SDK_INDEX) -> set:
    """Every `env = "<VAR>"` in nros-sdk-index.toml, at any depth: the one
    variable of each resource location (RFC-0103 D6)."""
    out = set()

    def walk(v):
        if isinstance(v, dict):
            for k, x in v.items():
                if k == "env" and isinstance(x, str):
                    out.add(x)
                else:
                    walk(x)
        elif isinstance(v, list):
            for x in v:
                walk(x)

    walk(tomllib.loads(path.read_text()))
    return out


# --- the rules -------------------------------------------------------------------

def _unit_problem(name: str, rules: dict):
    """N4 on a name's last word, or None."""
    last = name.lower().split("_")[-1]
    if last in rules["allow"]:
        return None
    if last in rules["banned"]:
        return f"ends `_{last}`: {rules['banned'][last]}"
    if last in rules["measure"]:
        return f"ends `_{last}`, which measures something, with no unit after it"
    return None


def _owner_of_symbol(rest: str, owners) -> str | None:
    """The longest owner word `w` with `rest == W` or `rest` starting `W_`."""
    best = None
    for w in owners:
        u = w.upper()
        if (rest == u or rest.startswith(u + "_")) and (best is None or len(w) > len(best)):
            best = w
    return best


def violations(s: dict, ctx: dict, rules: dict):
    """[(rule, why)] for one subject, at most one per rule, in rule order."""
    out = {}

    def add(rule, why):
        out.setdefault(rule, why)

    kind, key = s["kind"], s.get("key")
    spellings = s.get("spellings", [])
    if kind == "kconfig":
        sym = s["subject"]
        if sym in rules["framework"]:
            return []
        if not sym.startswith("CONFIG_NROS_"):
            add("N6", f"`{sym}` does not start CONFIG_NROS_")
        else:
            rest = sym[len("CONFIG_NROS_"):]
            if _owner_of_symbol(rest, ctx["owners"]) is None:
                add("N3", f"`{sym}` does not start with an owner word after CONFIG_NROS_")
            p = _unit_problem(rest, rules)
            if p:
                add("N4", f"`{sym}` {p}")
        spellings = [("kconfig", sym)]
    elif kind == "statement":
        if not WORD.match(key):
            add("N1", f"statement key `{key}` is one lower-case word")
        want = {"env": "NROS_ENTRY_" + key.upper(), "kconfig": "CONFIG_NROS_" + key.upper(),
                "toml": key}
        for road, v in spellings:
            if road in want and v != want[road]:
                add("N2", f"{road} spelling `{v}` is not `{want[road]}`, the one `{key}` derives")
    elif key is not None and (kind == "knob" or "." in key):
        segs = key.split(".")
        if len(segs) != 2 or not all(WORD.match(x) for x in segs):
            add("N1", f"key `{key}` is not `<owner>.<name>` (two lower snake case segments)")
        if segs[0] not in ctx["owners"]:
            add("N3", f"key `{key}`: `{segs[0]}` is not an owner word")
        p = _unit_problem(segs[-1], rules)
        if p:
            add("N4", f"key `{key}` {p}")
        did = derive_id(key)
        want = {"toml": key, "env": "NROS_" + did, "kconfig": "CONFIG_NROS_" + did,
                "cmake": "NROS_OVERRIDE_" + did, "run": "NROS_" + did}
        for road, v in spellings:
            if road in want and v != want[road]:
                add("N2", f"{road} spelling `{v}` is not `{want[road]}`, the one `{key}` derives")
    elif kind == "env" and key is not None:
        want = "NROS_" + key.upper()
        if s["subject"] != want:
            add("N2", f"`{s['subject']}` is not `{want}`, statement `{key}`'s run-time override")

    names = [v for road, v in spellings if road != "toml"]
    if kind in ("knob", "statement"):
        for v in names:
            if v in ctx["env_names"]:
                add("N5", f"build spelling `{v}` is also the name of an env row")
    if s.get("foreign"):
        if s["subject"].startswith("NROS_"):
            add("N6", f"`{s['subject']}` is foreign but starts NROS_, which is nano-ros's prefix")
    else:
        for v in names:
            if not v.startswith(("NROS_", "CONFIG_NROS_")):
                add("N6", f"`{v}` does not start NROS_ (CONFIG_NROS_ in Kconfig)")
    for road, v in spellings:
        if v in ctx["retired"]:
            add("N7", f"`{v}` is a retired spelling, kept for good, so never reused")
        if v in ctx["resources"]:
            add("N8", f"`{v}` is a resource location's variable (nros-sdk-index.toml)")
    return [(r, out[r]) for r in RULES if r in out]


# --- subjects --------------------------------------------------------------------

def _knob_subject(k, where):
    did = k["id"]
    sp = [("toml", k["key"]), ("env", "NROS_" + did), ("kconfig", "CONFIG_NROS_" + did)]
    if "type" in k:
        sp.append(("cmake", "NROS_OVERRIDE_" + did))
    return {"kind": "knob", "subject": k["key"], "key": k["key"], "spellings": sp,
            "foreign": False, "where": k.get("file") or where}


def _env_subject(e, where):
    return {"kind": "env", "subject": e["name"], "key": e.get("key"),
            "spellings": [("run", e["name"])], "foreign": e.get("foreign") is not None,
            "where": e.get("file") or where}


def registry_subjects(reg, core_where="packages/tooling/nros-zephyr-build/knobs.toml"):
    """Subjects of a parsed registry (`knob_registry.parse`). A `class =
    "retired"` row names a knob that no longer exists: its key is kept only as
    the error table (RFC-0140 D4), so it is no live name and is not judged
    (its spellings stay reserved through N7)."""
    out = [_knob_subject(k, core_where) for k in reg["knob"] if not k.get("retired")]
    for s in reg["statement"]:
        sp = [("toml", s["key"])]
        if s["carrier"]:
            sp.append(("env", s["carrier"]))
        if s["kconfig"]:
            sp.append(("kconfig", s["kconfig"]))
        out.append({"kind": "statement", "subject": s["key"], "key": s["key"], "spellings": sp,
                    "foreign": False, "where": core_where})
    out += [_env_subject(e, core_where) for e in reg["env"]]
    return out


def registry_context(reg, resources, extra_owners=()):
    owners = set(reg["owners"]) | {p["owner"] for p in reg["packages"]} | set(extra_owners)
    retired = set()
    for row in reg["knob"] + reg["statement"] + reg["env"]:
        retired |= {e["name"] for e in row["retired_entries"]}
    return {"owners": owners, "env_names": {e["name"] for e in reg["env"]}, "retired": retired,
            "resources": set(resources)}


def hand_kconfig_symbols(text: str):
    """[(CONFIG_<symbol>, line)] of every `config`/`menuconfig` outside the
    generated knob section of `zephyr/Kconfig`."""
    out, inside = [], False
    for i, line in enumerate(text.splitlines(), 1):
        if line.strip() == KCONFIG_BEGIN:
            inside = True
        elif line.strip() == KCONFIG_END:
            inside = False
        elif not inside:
            m = re.match(r"^\s*(?:menu)?config\s+([A-Z0-9_]+)\s*$", line)
            if m and m.group(1).startswith("NROS"):
                out.append(("CONFIG_" + m.group(1), i))
    return out


def kconfig_subjects(text: str, live_spellings, where="zephyr/Kconfig"):
    """Hand Kconfig symbols that are no registry row's spelling (those are
    judged as the row)."""
    return [{"kind": "kconfig", "subject": sym, "key": None, "spellings": [("kconfig", sym)],
             "foreign": False, "where": f"{where}:{line}"}
            for sym, line in hand_kconfig_symbols(text) if sym not in live_spellings]


def findings(subjects, ctx, rules):
    """{(kind, subject): {"rules": {rule: why}, "where": ...}} for every subject
    with a violation."""
    out = {}
    for s in subjects:
        v = violations(s, ctx, rules)
        if v:
            out[(s["kind"], s["subject"])] = {"rules": dict(v), "where": s["where"]}
    return out


# --- probes: the entries the lint cannot see -----------------------------------

def probe_present(entry, row_names, repo=REPO):
    """Is a baseline entry the lint cannot see still true? `probe = "row"`: the
    row exists. `read`: the name is used on a non-comment line of a tracked
    source file. `build-read`: in a build-time read (`option_env!`, `env!`,
    `rerun-if-env-changed=`, cmake `$ENV{}`)."""
    name = entry["subject"]
    if entry.get("probe") == "row":
        return name in row_names
    pats = [re.compile(p.format(n=re.escape(name))) for p in BUILD_READ]
    word = re.compile(r"(?<![A-Za-z0-9_])" + re.escape(name) + r"(?![A-Za-z0-9_])")
    args = ["-n", "-I", "-F", "-e", name, "--", *PROBE_PATHS, ":(exclude)*.md",
            *[f":(exclude){p}" for p in PROBE_EXCLUDE]]
    for line in git_grep.lines(args, repo):
        text = line.split(":", 2)[2] if line.count(":") >= 2 else line
        if COMMENT.match(text) or not word.search(text):
            continue
        if entry["kind"] == "read" or any(p.search(text) for p in pats):
            return True
    return False


# --- the comparison ----------------------------------------------------------------

def compare(found, baseline, row_names, present=probe_present):
    """(new, fixed): `new` lists a violation no baseline entry holds, `fixed` a
    baseline entry (or one of its rules) that no longer applies."""
    new, fixed = [], []
    base = {(e["kind"], e["subject"]): e for e in baseline}
    for (kind, subject), f in sorted(found.items()):
        e = base.get((kind, subject))
        held = set(e["rules"]) if e and not e.get("probe") else set()
        for rule, why in f["rules"].items():
            if rule not in held:
                new.append(f"{kind} `{subject}` ({f['where']}): {rule} - {why}")
    for (kind, subject), e in sorted(base.items()):
        if e.get("probe") or kind in ("read", "build-read"):
            if not present(e, row_names):
                fixed.append(f"{kind} `{subject}`: {', '.join(e['rules'])} - no longer present "
                             f"({e.get('probe') or kind} probe); remove it from the baseline")
            continue
        have = set(found.get((kind, subject), {"rules": {}})["rules"])
        for rule in e["rules"]:
            if rule not in have:
                fixed.append(f"{kind} `{subject}`: {rule} no longer applies; remove it from the "
                             "baseline")
    return new, fixed


def case_subject(c):
    """A `[[case]]` of the data file as a subject (spellings default to the
    ones its key derives)."""
    kind, key = c["kind"], c.get("key")
    subject = c.get("name") or key
    if "spellings" in c:
        sp = [tuple(x.split(":", 1)) for x in c["spellings"]]
    elif kind == "knob":
        did = derive_id(key)
        sp = [("toml", key), ("env", "NROS_" + did), ("kconfig", "CONFIG_NROS_" + did),
              ("cmake", "NROS_OVERRIDE_" + did)]
    elif kind == "statement":
        sp = [("toml", key), ("env", "NROS_ENTRY_" + key.upper())]
    elif kind == "env":
        sp = [("run", subject)]
    else:
        sp = [("kconfig", subject)]
    return {"kind": kind, "subject": subject, "key": key, "spellings": sp,
            "foreign": bool(c.get("foreign")), "where": "[[case]]"}


def case_context(rules):
    c = rules["case_context"]
    return {"owners": set(c.get("owners", [])), "env_names": set(c.get("env_names", [])),
            "retired": set(c.get("retired", [])), "resources": set(c.get("resources", []))}

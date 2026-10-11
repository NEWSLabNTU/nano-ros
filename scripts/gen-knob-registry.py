#!/usr/bin/env python3
"""Generate the knob registry's Rust table from its TOML (RFC-0103 D2b, phase-484 W5a).

    python3 scripts/gen-knob-registry.py           # write
    python3 scripts/gen-knob-registry.py --check   # fail if the committed file differs

Input:  packages/tooling/nros-zephyr-build/knobs.toml   (the ONLY hand-kept list)
Output: packages/tooling/nros-zephyr-build/src/knob_registry.rs
        cmake/NanoRosKnobRegistry.cmake   (the legacy spellings, for the cmake lanes)
        zephyr/Kconfig                    (the knob section, between its markers)

`nros-zephyr-build` is dependency-free on purpose (every build script that reads
a knob reaches it), so it cannot parse TOML at build time; the table is
generated and committed instead, and `just check knob-registry` holds the two
equal. Every spelling of a knob derives from its row:

  env      `NROS_<id>`, plus each `legacy` name, accepted until phase-484 W8
  Kconfig  `CONFIG_<kconfig>`, default `CONFIG_NROS_<id>`
  TOML     `[knobs.<toml>]`, where the ladder has a key for it

A row whose knob has a Zephyr Kconfig symbol also carries that symbol's
DEFINITION — `type`, `prompt`, the `if` it sits under (`kconfig_if`), its
attribute lines (`kconfig_attrs`, verbatim) and `help` — and the module's
`zephyr/Kconfig` knob section is rendered from them (phase-484 W5a; the prompts
stay until W5b makes the section prompt-less). `just check zephyr-kconfig-knobs`
holds the committed section equal to the render, and refuses a registry
symbol defined anywhere else in that file.

`KCONFIG_PAIRS` is emitted for every SPELLING whose Kconfig symbol is not
`CONFIG_<spelling>` — the shape `kconfig_key_for` and its harvesting gate
(`check-kconfig-knob-forwarding`) already read.
"""
import re
import sys
from pathlib import Path

try:
    import tomllib
except ModuleNotFoundError:  # Python < 3.11 — the repo's interpreter is 3.10
    import tomli as tomllib

REPO = Path(__file__).resolve().parents[1]
SRC = REPO / "packages/tooling/nros-zephyr-build/knobs.toml"
OUT = REPO / "packages/tooling/nros-zephyr-build/src/knob_registry.rs"
CMAKE_OUT = REPO / "cmake/NanoRosKnobRegistry.cmake"
KCONFIG_OUT = REPO / "zephyr/Kconfig"
KCONFIG_BEGIN = "# >>> knob registry >>>"
KCONFIG_END = "# <<< knob registry <<<"
KCONFIG_TYPES = {"bool", "int", "hex", "string"}

# The two messages a legacy spelling earns, written ONCE here and emitted into
# both outputs (phase-484 W5a): the Rust ladder (`nros-zephyr-build`) and the
# cmake lanes (`cmake/NanoRosKnobEnv.cmake`) substitute `{legacy}`, `{env}` and
# `{value}` and print the result, so the build script and the configure say the
# same words. Rust format braces are NOT used — both sides do a plain replace.
DEPRECATION = (
    "{legacy} is a deprecated spelling of {env} (RFC-0103 D2b); "
    "set {env} instead -- {legacy} is read until phase-484 W8 and then refused"
)
CONFLICT = (
    "{first}={first_value} and {other}={other_value} name ONE knob with two values; "
    "{other} is a deprecated spelling of {env} (RFC-0103 D2b) -- set only {env}"
)

ID = re.compile(r"^[A-Z][A-Z0-9_]*$")
ENV = re.compile(r"^(NROS|ZPICO)_[A-Z0-9_]+$")
TOML_KEY = re.compile(r"^[a-z][a-z0-9_]*(\.[a-z][a-z0-9_]*)+$")
FIELDS = {"id", "legacy", "kconfig", "toml",
          "type", "prompt", "kconfig_if", "kconfig_attrs", "help"}
# The Kconfig definition is all-or-nothing: `type` says the module defines the
# symbol, and the rest only mean something beside it.
KCONFIG_DEF_FIELDS = {"prompt", "kconfig_if", "kconfig_attrs", "help"}


def load(path=SRC):
    with open(path, "rb") as fh:
        return load_from_text(fh.read().decode())


def load_from_text(text):
    rows = tomllib.loads(text).get("knob", [])
    errors = []
    spellings = {}
    tomls = {}
    for i, r in enumerate(rows):
        where = f"knob #{i + 1} ({r.get('id', '?')})"
        extra = set(r) - FIELDS
        if extra:
            errors.append(f"{where}: unknown field(s) {sorted(extra)}")
        ident = r.get("id", "")
        if not ID.match(ident) or ident.startswith("NROS_"):
            errors.append(f"{where}: id must be the env name without `NROS_`")
        for s in ["NROS_" + ident] + list(r.get("legacy", [])):
            if not ENV.match(s):
                errors.append(f"{where}: `{s}` is not an env spelling")
            if s in spellings:
                errors.append(f"{where}: `{s}` is already a spelling of {spellings[s]}")
            spellings[s] = ident
        k = r.get("kconfig")
        if k is not None and (not ID.match(k) or k == "NROS_" + ident):
            errors.append(f"{where}: `kconfig` is written only where it differs from NROS_<id>")
        ty = r.get("type")
        if ty is None and KCONFIG_DEF_FIELDS & set(r):
            errors.append(f"{where}: {sorted(KCONFIG_DEF_FIELDS & set(r))} without `type`")
        if ty is not None:
            if ty not in KCONFIG_TYPES:
                errors.append(f"{where}: `type` must be one of {sorted(KCONFIG_TYPES)}")
            if '"' in r.get("prompt", ""):
                errors.append(f"{where}: a `prompt` cannot hold a double quote")
            for a in r.get("kconfig_attrs", []):
                if not a.strip() or a.strip() in ("help", "---help---"):
                    errors.append(f"{where}: `kconfig_attrs` holds attribute lines, not `{a}`")
        t = r.get("toml")
        if t is not None:
            if not TOML_KEY.match(t):
                errors.append(f"{where}: `toml` must be `<group>.<name>`")
            if t in tomls:
                errors.append(f"{where}: toml key `{t}` is already {tomls[t]}")
            tomls[t] = ident
    ids = [r.get("id") for r in rows]
    if ids != sorted(ids):
        errors.append("rows must be sorted by id (a merge then conflicts on one row, not the file)")
    return rows, errors


def q(s):
    return f'"{s}"'


def render(rows):
    out = [
        "// @generated by scripts/gen-knob-registry.py from knobs.toml — do not edit.",
        "// Edit knobs.toml and re-run the script; `just check knob-registry` holds the two equal.",
        "",
        "use crate::KnobRow;",
        "",
        "/// Every build-time knob, one row each (RFC-0103 D2b).",
        "pub const KNOBS: &[KnobRow] = &[",
    ]
    pairs = []
    for r in rows:
        ident = r["id"]
        env = "NROS_" + ident
        kconfig = "CONFIG_" + r.get("kconfig", env)
        legacy = r.get("legacy", [])
        toml = r.get("toml")
        out.append("    KnobRow {")
        out.append(f"        id: {q(ident)},")
        out.append(f"        env: {q(env)},")
        out.append("        legacy: &[" + ", ".join(q(x) for x in legacy) + "],")
        out.append(f"        kconfig: {q(kconfig)},")
        out.append(f"        toml: {'Some(' + q(toml) + ')' if toml else 'None'},")
        out.append("    },")
        for s in [env] + legacy:
            if kconfig != "CONFIG_" + s:
                pairs.append((s, kconfig))
    out.append("];")
    out.append("")
    out.append("/// The env spellings whose Kconfig symbol is not `CONFIG_<spelling>`,")
    out.append("/// derived from [`KNOBS`]. Read through [`crate::kconfig_key_for`].")
    out.append("pub const KCONFIG_PAIRS: &[(&str, &str)] = &[")
    for s, k in sorted(pairs):
        out.append(f"    ({q(s)}, {q(k)}),")
    out.append("];")
    out.append("")
    out.append("/// What a build prints when a knob arrives under a legacy spelling.")
    out.append("/// Substitute `{legacy}` and `{env}`; the cmake twin prints the same text.")
    out.append(f"pub const DEPRECATION_TEMPLATE: &str = {q(DEPRECATION)};")
    out.append("")
    out.append("/// The refusal when two spellings of one knob hold different values.")
    out.append("/// Substitute `{first}`, `{first_value}`, `{other}`, `{other_value}`, `{env}`.")
    out.append(f"pub const CONFLICT_TEMPLATE: &str = {q(CONFLICT)};")
    out.append("")
    return "\n".join(out)


def render_cmake(rows):
    out = [
        "# @generated by scripts/gen-knob-registry.py from",
        "# packages/tooling/nros-zephyr-build/knobs.toml -- do not edit.",
        "# Edit knobs.toml and re-run the script; `just check knob-registry` holds the",
        "# two equal. Read through `_nros_knob_env` (cmake/NanoRosKnobEnv.cmake).",
        "#",
        "# The LEGACY env spellings of each knob (RFC-0103 D2b, phase-484 W5a), keyed",
        "# both ways: NROS_KNOB_LEGACY_<canonical> lists the old names a value may",
        "# still arrive under, NROS_KNOB_CANONICAL_<legacy> names the row it belongs to.",
        "",
        f'set(NROS_KNOB_DEPRECATION_TEMPLATE "{DEPRECATION}")',
        f'set(NROS_KNOB_CONFLICT_TEMPLATE "{CONFLICT}")',
        "",
    ]
    for r in rows:
        legacy = r.get("legacy", [])
        if not legacy:
            continue
        env = "NROS_" + r["id"]
        out.append(f'set(NROS_KNOB_LEGACY_{env} "{";".join(legacy)}")')
        for l in legacy:
            out.append(f"set(NROS_KNOB_CANONICAL_{l} {env})")
    out.append("")
    return "\n".join(out)


def kconfig_symbol(r):
    return r.get("kconfig", "NROS_" + r["id"])


def render_kconfig_section(rows):
    """The knob section of `zephyr/Kconfig`, markers included, no trailing newline.

    Rows are grouped by the `if` they sit under (none first, then each
    condition in name order) and kept in registry (id) order inside a group."""
    out = [
        KCONFIG_BEGIN,
        "# GENERATED by scripts/gen-knob-registry.py from",
        "# packages/tooling/nros-zephyr-build/knobs.toml -- do not edit here: edit the",
        "# row's `type`/`prompt`/`kconfig_if`/`kconfig_attrs`/`help` and re-run the",
        "# script. `just check zephyr-kconfig-knobs` holds this section equal to the",
        "# render and refuses a registry knob's symbol defined anywhere else in the",
        "# file (RFC-0103 D2b, phase-484 W5a).",
        "",
    ]
    defined = [r for r in rows if r.get("type")]
    conds = [None] + sorted({r["kconfig_if"] for r in defined if r.get("kconfig_if")})
    for cond in conds:
        group = [r for r in defined if r.get("kconfig_if") == cond]
        if not group:
            continue
        if cond:
            out += [f"if {cond}", ""]
        for r in group:
            out.append(f"config {kconfig_symbol(r)}")
            prompt = r.get("prompt")
            out.append(f'    {r["type"]} "{prompt}"' if prompt is not None else f'    {r["type"]}')
            out += ["    " + a for a in r.get("kconfig_attrs", [])]
            if r.get("help") is not None:
                out.append("    help")
                out += [("      " + h) if h else "" for h in r["help"].split("\n")]
            out.append("")
        if cond:
            out += [f"endif # {cond}", ""]
    out.append(KCONFIG_END)
    return "\n".join(out)


def kconfig_section_span(text):
    """(start, end) character offsets of the marked section in `text`, `end`
    just past the END marker; None when either marker is missing or repeated."""
    b = [m.start() for m in re.finditer(r"(?m)^" + re.escape(KCONFIG_BEGIN) + r"$", text)]
    e = [m.end() for m in re.finditer(r"(?m)^" + re.escape(KCONFIG_END) + r"$", text)]
    if len(b) != 1 or len(e) != 1 or e[0] < b[0]:
        return None
    return b[0], e[0]


def splice_kconfig(text, rows):
    """`text` with its knob section replaced by the render; None without markers."""
    span = kconfig_section_span(text)
    if span is None:
        return None
    return text[:span[0]] + render_kconfig_section(rows) + text[span[1]:]


def kconfig_problems(text, rows):
    """Why the committed `zephyr/Kconfig` is not the registry's, as sentences."""
    span = kconfig_section_span(text)
    if span is None:
        return [f"zephyr/Kconfig: no single `{KCONFIG_BEGIN}` ... `{KCONFIG_END}` section"]
    problems = []
    if text[span[0]:span[1]] != render_kconfig_section(rows):
        problems.append(
            "zephyr/Kconfig: the knob section differs from what knobs.toml renders.\n"
            "  Edit the row in packages/tooling/nros-zephyr-build/knobs.toml (not the\n"
            "  section) and run `python3 scripts/gen-knob-registry.py`.")
    outside = text[:span[0]] + text[span[1]:]
    for r in rows:
        sym = kconfig_symbol(r)
        if re.search(r"(?m)^\s*(?:menu)?config\s+" + re.escape(sym) + r"\s*$", outside):
            where = "is also defined" if r.get("type") else "is defined"
            problems.append(
                f"zephyr/Kconfig: `config {sym}` (knob {r['id']}) {where} OUTSIDE the\n"
                "  generated knob section. A registry knob's symbol has one definition,\n"
                "  rendered from its row: move its type/prompt/attributes/help into the\n"
                "  row and delete the hand-written block.")
    return problems


def self_test():
    import tempfile
    bad = (
        '[[knob]]\nid = "B"\nlegacy = ["ZPICO_A"]\n\n'
        '[[knob]]\nid = "A"\nkconfig = "NROS_A"\nlegacy = ["ZPICO_A"]\ntoml = "x"\n'
    )
    with tempfile.NamedTemporaryFile("w", suffix=".toml", delete=False) as fh:
        fh.write(bad)
    _, errs = load(Path(fh.name))
    Path(fh.name).unlink()
    text = "\n".join(errs)
    for want in ("already a spelling", "only where it differs", "`<group>.<name>`", "sorted by id"):
        assert want in text, f"selftest: no `{want}` error in {errs!r}"
    rendered = render([{"id": "ZENOH_X", "legacy": ["ZPICO_X"], "kconfig": "NROS_X"}, {"id": "Y"}])
    assert '("NROS_ZENOH_X", "CONFIG_NROS_X")' in rendered
    assert '("ZPICO_X", "CONFIG_NROS_X")' in rendered
    assert '"NROS_Y"' in rendered and '("NROS_Y"' not in rendered, "selftest: a derived pair was emitted"
    kc_rows = [
        {"id": "Z", "type": "int", "prompt": "z", "kconfig_attrs": ["default 4"], "help": "a\n\nb"},
        {"id": "ZENOH_X", "kconfig": "NROS_X", "type": "bool", "prompt": "x",
         "kconfig_if": "NROS_RMW_ZENOH"},
        {"id": "Y"},
    ]
    sec = render_kconfig_section(kc_rows)
    assert "config NROS_Z\n    int \"z\"\n    default 4\n    help\n      a\n\n      b\n" in sec, sec
    assert "if NROS_RMW_ZENOH\n\nconfig NROS_X\n    bool \"x\"\n\nendif # NROS_RMW_ZENOH" in sec, sec
    assert "NROS_Y" not in sec, "selftest: a row with no `type` was rendered"
    good = "menuconfig NROS\nif NROS\n" + sec + "\nendif\n"
    assert kconfig_problems(good, kc_rows) == [], kconfig_problems(good, kc_rows)
    assert splice_kconfig(good, kc_rows) == good
    # NEGATIVE controls, each the failure this gate exists for:
    edited = good.replace("default 4", "default 5")
    assert any("differs from what knobs.toml renders" in p for p in kconfig_problems(edited, kc_rows))
    assert splice_kconfig(edited, kc_rows) == good, "the writer must restore the render"
    second = good + "config NROS_Z\n    int\n"
    assert any("NROS_Z` (knob Z) is also defined OUTSIDE" in p for p in kconfig_problems(second, kc_rows))
    stray = good + "config NROS_Y\n    int\n"
    assert any("NROS_Y` (knob Y) is defined OUTSIDE" in p for p in kconfig_problems(stray, kc_rows))
    unmarked = good.replace(KCONFIG_END, "")
    assert kconfig_problems(unmarked, kc_rows) and splice_kconfig(unmarked, kc_rows) is None
    _, errs = load_from_text('[[knob]]\nid = "A"\nprompt = "p"\n')
    assert any("without `type`" in e for e in errs), errs
    cm = render_cmake([{"id": "ZENOH_X", "legacy": ["ZPICO_X"]}, {"id": "Y"}])
    assert 'set(NROS_KNOB_LEGACY_NROS_ZENOH_X "ZPICO_X")' in cm, cm
    assert "set(NROS_KNOB_CANONICAL_ZPICO_X NROS_ZENOH_X)" in cm, cm
    assert "NROS_Y" not in cm, "selftest: a row with no legacy spelling reached the cmake table"


def main():
    self_test()
    rows, errors = load()
    if errors:
        sys.stderr.write("gen-knob-registry: FAILED — knobs.toml:\n")
        for e in errors:
            sys.stderr.write(f"    {e}\n")
        return 1
    outputs = [(OUT, render(rows)), (CMAKE_OUT, render_cmake(rows))]
    if "--check-kconfig" in sys.argv:
        problems = kconfig_problems(KCONFIG_OUT.read_text(), rows)
        for msg in problems:
            sys.stderr.write(f"zephyr-kconfig-knobs: FAILED — {msg}\n")
        if problems:
            return 1
        n = sum(1 for r in rows if r.get("type"))
        print(f"zephyr-kconfig-knobs: OK — {n} knob symbol(s) rendered from knobs.toml, "
              "none defined outside the section (selftest: 4 negative controls caught)")
        return 0
    if "--check" in sys.argv:
        stale = [p for p, text in outputs if (p.read_text() if p.exists() else "") != text]
        for p in stale:
            sys.stderr.write(
                f"gen-knob-registry: FAILED — {p.relative_to(REPO)} is stale against\n"
                f"  {SRC.relative_to(REPO)}. Run `python3 scripts/gen-knob-registry.py`\n"
                "  and commit both: a hand edit to a generated table is a second list.\n"
            )
        if stale:
            return 1
        print(f"knob-registry: OK — {len(rows)} knob(s), generated tables current")
        return 0
    for p, text in outputs:
        p.write_text(text)
        print(f"wrote {p.relative_to(REPO)} ({len(rows)} knobs)")
    kconfig = splice_kconfig(KCONFIG_OUT.read_text(), rows)
    if kconfig is None:
        sys.stderr.write(f"gen-knob-registry: FAILED — zephyr/Kconfig has no single "
                         f"`{KCONFIG_BEGIN}` ... `{KCONFIG_END}` section to render into\n")
        return 1
    KCONFIG_OUT.write_text(kconfig)
    print(f"wrote {KCONFIG_OUT.relative_to(REPO)} knob section "
          f"({sum(1 for r in rows if r.get('type'))} symbols)")
    return 0


if __name__ == "__main__":
    sys.exit(main())

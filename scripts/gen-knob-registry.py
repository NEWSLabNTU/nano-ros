#!/usr/bin/env python3
"""Generate the knob registry's Rust table from its TOML (RFC-0103 D2b, phase-484 W5a).

    python3 scripts/gen-knob-registry.py           # write
    python3 scripts/gen-knob-registry.py --check   # fail if the committed file differs

Input:  packages/tooling/nros-zephyr-build/knobs.toml   (the ONLY hand-kept list)
Output: packages/tooling/nros-zephyr-build/src/knob_registry.rs
        cmake/NanoRosKnobRegistry.cmake   (the retired spellings, for the cmake lanes)
        zephyr/Kconfig                    (the knob section, between its markers)
        zephyr/Kconfig.knob-prompts       (a guest's prompts, phase-484 W5b)

`nros-zephyr-build` is dependency-free on purpose (every build script that reads
a knob reaches it), so it cannot parse TOML at build time; the table is
generated and committed instead, and `just check knob-registry` holds the two
equal. Every spelling of a knob derives from its row:

  env      `NROS_<id>`. Each `legacy` name is RETIRED (phase-484 W8): set, it
           stops the build naming `NROS_<id>`. The column stays as that
           error table for good: an unlisted old name would be ignored.
  Kconfig  `CONFIG_<kconfig>`, default `CONFIG_NROS_<id>`
  TOML     `[knobs.<toml>]`, where the ladder has a key for it
  image    `[image.<id>.knobs.<group>] <name>` in a project's system.toml
           (phase-484 W8): the `toml` key where the row has one, so a project
           and a board spell a knob the same way, else the id split at its
           first `_` (`ZEPHYR_HEAP_SIZE` -> `zephyr.heap_size`). An
           `[image.<id>] env` row naming the knob is refused, in
           `IMAGE_ENV_RETIRED_TEMPLATE`'s words.

A row whose knob has a Zephyr Kconfig symbol also carries that symbol's
DEFINITION — `type`, `prompt`, the `if` it sits under (`kconfig_if`), its
attribute lines (`kconfig_attrs`, verbatim) and `help` — and the module's
`zephyr/Kconfig` knob section is rendered from them, PROMPT-LESS (phase-484
W5b, RFC-0103 D2a/D2b): it `osource`s `<build>/nros/Kconfig.from-system-toml`
first, which the module hook writes — the image's values as `default`s for a
project with a `system.toml`, or `zephyr/Kconfig.knob-prompts` (rendered here
too) for a guest, which adds each prompt back under the same `if`/`depends on`.
`just check zephyr-kconfig-knobs` holds the committed section and the prompts
file equal to the render, and refuses a registry symbol defined anywhere else
in `zephyr/Kconfig`.

`KCONFIG_PAIRS` is emitted for every CANONICAL spelling whose Kconfig symbol is
not `CONFIG_<spelling>` — the shape `kconfig_key_for` and its harvesting gate
(`check-kconfig-knob-forwarding`) already read. A retired spelling has no pair:
nothing reads one any more (phase-484 W8).

A row with `retired = "<reason>"` is a knob that no longer EXISTS, not one that
was renamed (RFC-0110 D2 retired the zenoh lease-task priority when zenoh-pico
1.10 folded the lease task into its executor). Every spelling of it -- the
canonical one included -- stops the build with `RETIRED_KNOB`'s sentence, in a
build script, a configure and the CLI alike. It carries no Kconfig definition,
so a prj.conf that still assigns the symbol fails Zephyr's own undefined-symbol
check. The row stays for good as the error table, like `legacy`.
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
PROMPTS_OUT = REPO / "zephyr/Kconfig.knob-prompts"
# Where the module hook writes the generated file, as Kconfig spells it
# (`KCONFIG_BINARY_DIR` is `<build>/Kconfig` on 3.7 and 4.4; D2b).
KCONFIG_GENERATED = "$(KCONFIG_BINARY_DIR)/../nros/Kconfig.from-system-toml"
KCONFIG_BEGIN = "# >>> knob registry >>>"
KCONFIG_END = "# <<< knob registry <<<"
KCONFIG_TYPES = {"bool", "int", "hex", "string"}

# The one message a retired spelling earns, written ONCE here and emitted into
# both outputs (phase-484 W8): the Rust ladder (`nros-zephyr-build`) and the
# cmake lanes (`cmake/NanoRosKnobEnv.cmake`) substitute `{legacy}` and `{env}`
# and stop the build with the result, so a build script and a configure refuse
# in the same words. Rust format braces are NOT used — both sides do a plain
# replace. Release 0.5.0-nros2 printed W5a's deprecation for one release; this
# is what that deprecation promised.
RETIRED = (
    "{legacy} is a retired spelling of {env} (RFC-0103 D2b; retired in phase-484 W8, "
    "after release 0.5.0-nros2 deprecated it) -- set {env} instead"
)
# The error an `[image.<id>] env` row earns when its key is a registry knob
# (phase-484 W8, RFC-0103 D6): the knob's one spelling is
# `[image.<id>.knobs.<group>] <name>`. Release 0.6.0 warned with these words
# minus the refusal; this is what that warning promised. `{image}`, `{env}`,
# `{group}` and `{name}` are replaced by the CLI.
# The error any spelling of a RETIRED knob earns (a row with `retired`): the
# knob is gone, so there is no replacement spelling to name -- the row's reason
# says what to do instead. `{spelling}` and `{reason}` are replaced.
RETIRED_KNOB = "{spelling} is a retired knob: {reason}"
IMAGE_ENV_RETIRED = (
    "[image.{image}] env {env} names the knob [image.{image}.knobs.{group}] {name} "
    "(RFC-0103 D6; refused in phase-484 W8, after release 0.6.0 deprecated it) -- "
    "move the row there"
)

ID = re.compile(r"^[A-Z][A-Z0-9_]*$")
ENV = re.compile(r"^(NROS|ZPICO)_[A-Z0-9_]+$")
TOML_KEY = re.compile(r"^[a-z][a-z0-9_]*(\.[a-z][a-z0-9_]*)+$")
FIELDS = {"id", "legacy", "kconfig", "toml", "names", "retired",
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
    image_keys = {}
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
        why = r.get("retired")
        if why is not None:
            if not isinstance(why, str) or not why.strip():
                errors.append(f"{where}: `retired` is the reason, a non-empty string")
            elif re.search(r'["\\$;]', why):
                errors.append(f"{where}: a `retired` reason cannot hold `\"`, `\\`, `$` or `;` "
                              "(it is emitted verbatim into Rust and cmake strings)")
            if ty is not None:
                errors.append(f"{where}: a `retired` knob has no Kconfig definition (drop `type`), "
                              "so a conf that still assigns it fails as an undefined symbol")
        t = r.get("toml")
        if t is not None:
            if not TOML_KEY.match(t):
                errors.append(f"{where}: `toml` must be `<group>.<name>`")
            if t in tomls:
                errors.append(f"{where}: toml key `{t}` is already {tomls[t]}")
        names = r.get("names")
        if names is not None and (ty != "int" or not names or not all(
                isinstance(n, str) and re.match(r"^[a-z][a-z0-9_]*$", n) for n in names)):
            errors.append(f"{where}: `names` is a non-empty list of lowercase words, on an int row")
        if ID.match(ident):
            key = image_key(r)
            if key is None:
                errors.append(f"{where}: an id with no `_` needs a `toml` key to be stated in a project")
            elif key in image_keys:
                errors.append(f"{where}: image key `{key}` is already {image_keys[key]}")
            else:
                image_keys[key] = ident
    ids = [r.get("id") for r in rows]
    if ids != sorted(ids):
        errors.append("rows must be sorted by id (a merge then conflicts on one row, not the file)")
    return rows, errors


def q(s):
    return f'"{s}"'


def image_key(r):
    """`<group>.<name>` under `[image.<id>.knobs]`: the row's `toml` key, else
    the id split at its first `_`, lowercased. None when neither exists."""
    if r.get("toml"):
        return r["toml"]
    head, sep, tail = r["id"].partition("_")
    return f"{head.lower()}.{tail.lower()}" if sep else None


def int_range(r):
    """The first `range <lo> <hi>` attribute line, as integers, or None."""
    for a in r.get("kconfig_attrs", []):
        m = re.match(r"^\s*range\s+(-?\d+)\s+(-?\d+)\s*$", a)
        if m:
            return int(m.group(1)), int(m.group(2))
    return None


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
        out.append(f"        image_key: {q(image_key(r))},")
        ty = r.get("type")
        out.append(f"        ty: {'Some(' + q(ty) + ')' if ty else 'None'},")
        rng = int_range(r)
        out.append(f"        range: {'Some((' + str(rng[0]) + ', ' + str(rng[1]) + '))' if rng else 'None'},")
        out.append("        names: &[" + ", ".join(q(x) for x in r.get("names", [])) + "],")
        why = r.get("retired")
        out.append(f"        retired: {'Some(' + q(why) + ')' if why else 'None'},")
        out.append("    },")
        if kconfig != "CONFIG_" + env and not r.get("retired"):
            pairs.append((env, kconfig))
    out.append("];")
    out.append("")
    out.append("/// The canonical env spellings whose Kconfig symbol is not `CONFIG_<spelling>`,")
    out.append("/// derived from [`KNOBS`]. Read through [`crate::kconfig_key_for`].")
    out.append("pub const KCONFIG_PAIRS: &[(&str, &str)] = &[")
    for s, k in sorted(pairs):
        out.append(f"    ({q(s)}, {q(k)}),")
    out.append("];")
    out.append("")
    out.append("/// The knobs the Zephyr module defines as Kconfig symbols (rows with a `type`),")
    out.append("/// in registry order. Read through [`crate::kconfig_knob`].")
    out.append("pub const KCONFIG_KNOBS: &[crate::KconfigKnob] = &[")
    for r in rows:
        if not r.get("type"):
            continue
        cond = r.get("kconfig_if")
        out.append("    crate::KconfigKnob {")
        out.append(f"        symbol: {q(kconfig_symbol(r))},")
        out.append(f"        id: {q(r['id'])},")
        out.append(f"        ty: {q(r['type'])},")
        out.append(f"        kconfig_if: {'Some(' + q(cond) + ')' if cond else 'None'},")
        out.append("        depends: &[" + ", ".join(q(d) for d in depends_on(r)) + "],")
        out.append("    },")
    out.append("];")
    out.append("")
    out.append("/// The refusal when a knob arrives under a RETIRED spelling (phase-484 W8).")
    out.append("/// Substitute `{legacy}` and `{env}`; the cmake twin refuses in the same words.")
    out.append(f"pub const RETIRED_TEMPLATE: &str = {q(RETIRED)};")
    out.append("")
    out.append("/// The refusal any spelling of a RETIRED knob earns (a row with `retired`).")
    out.append("/// Substitute `{spelling}` and `{reason}`; the cmake twin refuses in the same words.")
    out.append(f"pub const RETIRED_KNOB_TEMPLATE: &str = {q(RETIRED_KNOB)};")
    out.append("")
    out.append("/// The refusal an `[image.<id>] env` row earns when it names a registry knob")
    out.append("/// (phase-484 W8). Substitute `{image}`, `{env}`, `{group}` and `{name}`.")
    out.append(f"pub const IMAGE_ENV_RETIRED_TEMPLATE: &str = {q(IMAGE_ENV_RETIRED)};")
    out.append("")
    return "\n".join(out)


def render_cmake(rows):
    out = [
        "# @generated by scripts/gen-knob-registry.py from",
        "# packages/tooling/nros-zephyr-build/knobs.toml -- do not edit.",
        "# Edit knobs.toml and re-run the script; `just check knob-registry` holds the",
        "# two equal. Read through `_nros_knob_env` (cmake/NanoRosKnobEnv.cmake).",
        "#",
        "# The RETIRED env spellings of each knob (RFC-0103 D2b, phase-484 W8) -- the",
        "# error table, kept for good: NROS_KNOB_LEGACY_<canonical> lists the old",
        "# names that now stop a configure, NROS_KNOB_CANONICAL_<legacy> names the row",
        "# each belongs to.",
        "",
        f'set(NROS_KNOB_RETIRED_TEMPLATE "{RETIRED}")',
        f'set(NROS_KNOB_RETIRED_KNOB_TEMPLATE "{RETIRED_KNOB}")',
        "",
    ]
    typed = [r for r in rows if r.get("type")]
    out.append("# The knobs the Zephyr module defines as Kconfig symbols (phase-484 W5b):")
    out.append("# NROS_KNOB_KCONFIG_SYMBOL_<id> is the symbol `-DNROS_OVERRIDE_<id>` sets;")
    out.append("# NROS_KNOB_IMAGE_TABLE_<id> / NROS_KNOB_IMAGE_NAME_<id> are where a project")
    out.append("# states it: `[image.<img>.knobs.<table>] <name> = <value>` (phase-484 W8).")
    out.append('set(NROS_KNOB_KCONFIG_IDS "' + ";".join(r["id"] for r in typed) + '")')
    for r in typed:
        out.append(f"set(NROS_KNOB_KCONFIG_SYMBOL_{r['id']} {kconfig_symbol(r)})")
        table, _, name = image_key(r).rpartition(".")
        out.append(f"set(NROS_KNOB_IMAGE_TABLE_{r['id']} {table})")
        out.append(f"set(NROS_KNOB_IMAGE_NAME_{r['id']} {name})")
    out.append("")
    retired = [r for r in rows if r.get("retired")]
    out.append("# RETIRED knobs: every spelling below stops a configure, with its row's reason")
    out.append("# substituted into NROS_KNOB_RETIRED_KNOB_TEMPLATE.")
    spelled = [s for r in retired for s in ["NROS_" + r["id"]] + r.get("legacy", [])]
    out.append('set(NROS_KNOB_RETIRED_SPELLINGS "' + ";".join(spelled) + '")')
    for r in retired:
        for s in ["NROS_" + r["id"]] + r.get("legacy", []):
            out.append(f'set(NROS_KNOB_RETIRED_REASON_{s} "{r["retired"]}")')
    out.append("")
    for r in rows:
        legacy = r.get("legacy", [])
        if not legacy or r.get("retired"):
            continue
        env = "NROS_" + r["id"]
        out.append(f'set(NROS_KNOB_LEGACY_{env} "{";".join(legacy)}")')
        for l in legacy:
            out.append(f"set(NROS_KNOB_CANONICAL_{l} {env})")
    out.append("")
    return "\n".join(out)


def kconfig_symbol(r):
    return r.get("kconfig", "NROS_" + r["id"])


def depends_on(r):
    """The row's `depends on` expressions — a generated definition repeats them,
    or the symbol's dependency widens and a value leaks (D2b's M2)."""
    return [a.strip()[len("depends on"):].strip()
            for a in r.get("kconfig_attrs", []) if a.strip().startswith("depends on ")]


def _grouped(rows):
    """(condition, rows) per `if`, none first then by name; registry order inside."""
    defined = [r for r in rows if r.get("type")]
    conds = [None] + sorted({r["kconfig_if"] for r in defined if r.get("kconfig_if")})
    for cond in conds:
        group = [r for r in defined if r.get("kconfig_if") == cond]
        if group:
            yield cond, group


def render_kconfig_section(rows):
    """The knob section of `zephyr/Kconfig`, markers included, no trailing newline.

    Every symbol is PROMPT-LESS: its prompt (a guest) and its value (a
    system.toml project) come from the generated file the section `osource`s
    first (phase-484 W5b); the prompt text is kept as a comment above each
    definition. Rows are grouped by the `if` they sit under (none first, then
    each condition in name order) and kept in registry (id) order inside a
    group."""
    out = [
        KCONFIG_BEGIN,
        "# GENERATED by scripts/gen-knob-registry.py from",
        "# packages/tooling/nros-zephyr-build/knobs.toml -- do not edit here: edit the",
        "# row's `type`/`prompt`/`kconfig_if`/`kconfig_attrs`/`help` and re-run the",
        "# script. `just check zephyr-kconfig-knobs` holds this section equal to the",
        "# render and refuses a registry knob's symbol defined anywhere else in the",
        "# file (RFC-0103 D2b, phase-484 W5a).",
        "#",
        "# No symbol here has a prompt (phase-484 W5b). The module hook writes the",
        "# file sourced below BEFORE Kconfig runs, and its definitions come first:",
        "#   * a project with a system.toml -- each knob's value from the image",
        "#     (`[image.<id>.knobs.<group>] <name>`, or `-DNROS_OVERRIDE_<ID>` for one",
        "#     build) as a `default`. With no prompt anywhere, a prj.conf line or a",
        "#     -DCONFIG_<knob> is a configure error: system.toml is the one source.",
        "#   * a guest (no system.toml) -- a copy of zephyr/Kconfig.knob-prompts,",
        "#     which gives each knob back its prompt, so prj.conf, menuconfig and",
        "#     -DCONFIG_* work as before.",
        f'osource "{KCONFIG_GENERATED}"',
        "",
    ]
    for cond, group in _grouped(rows):
        if cond:
            out += [f"if {cond}", ""]
        for r in group:
            if r.get("prompt") is not None:
                out.append(f'# prompt: {r["prompt"]}')
            out.append(f"config {kconfig_symbol(r)}")
            out.append(f'    {r["type"]}')
            out += ["    " + a for a in r.get("kconfig_attrs", [])]
            if r.get("help") is not None:
                out.append("    help")
                out += [("      " + h) if h else "" for h in r["help"].split("\n")]
            out.append("")
        if cond:
            out += [f"endif # {cond}", ""]
    out.append(KCONFIG_END)
    return "\n".join(out)


def render_prompts(rows):
    """`zephyr/Kconfig.knob-prompts`: what a GUEST's generated file holds — each
    knob's prompt, under the module definition's own `if` and `depends on`, and
    nothing else (the module keeps the defaults, ranges and help)."""
    out = [
        "# @generated by scripts/gen-knob-registry.py from",
        "# packages/tooling/nros-zephyr-build/knobs.toml -- do not edit.",
        "#",
        "# A GUEST's knob prompts (phase-484 W5b, RFC-0103 D2a). The module's knob",
        "# section defines every knob symbol without a prompt; for an application",
        "# with no system.toml the module hook copies this file to",
        "# <build>/nros/Kconfig.from-system-toml, which that section sources, so",
        "# prj.conf, menuconfig and -DCONFIG_* configure the knobs as they always did.",
        "",
    ]
    for cond, group in _grouped(rows):
        if cond:
            out += [f"if {cond}", ""]
        for r in group:
            out.append(f"config {kconfig_symbol(r)}")
            prompt = r.get("prompt") or r["id"].replace("_", " ").lower()
            out.append(f'    {r["type"]} "{prompt}"')
            out += [f"    depends on {d}" for d in depends_on(r)]
            out.append("")
        if cond:
            out += [f"endif # {cond}", ""]
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
    # phase-484 W8 -- the image spelling: the board's `toml` key, else the id
    # split at its first `_`; two rows on one key, or `names` off an int row,
    # are refused.
    assert image_key({"id": "ZEPHYR_HEAP_SIZE"}) == "zephyr.heap_size"
    assert image_key({"id": "MAX_ARRAY_LEN", "toml": "params.max_array_len"}) == "params.max_array_len"
    _, errs = load_from_text(
        '[[knob]]\nid = "A_B"\ntoml = "zenoh.x"\n\n'
        '[[knob]]\nid = "ZENOH_X"\ntype = "bool"\nnames = ["off"]\n')
    text = "\n".join(errs)
    assert "image key `zenoh.x` is already A_B" in text, errs
    assert "`names` is a non-empty list" in text, errs
    rs = render([{"id": "LOG_X", "type": "int", "kconfig_attrs": ["range 0 6"], "names": ["off"]}])
    assert 'image_key: "log.x",\n        ty: Some("int"),\n        range: Some((0, 6)),\n        names: &["off"],' in rs, rs
    rendered = render([{"id": "ZENOH_X", "legacy": ["ZPICO_X"], "kconfig": "NROS_X"}, {"id": "Y"}])
    assert '("NROS_ZENOH_X", "CONFIG_NROS_X")' in rendered
    assert '("ZPICO_X"' not in rendered, "selftest: a retired spelling got a Kconfig pair"
    assert '"NROS_Y"' in rendered and '("NROS_Y"' not in rendered, "selftest: a derived pair was emitted"
    kc_rows = [
        {"id": "Z", "type": "int", "prompt": "z", "kconfig_attrs": ["default 4"], "help": "a\n\nb"},
        {"id": "ZENOH_X", "kconfig": "NROS_X", "type": "bool", "prompt": "x",
         "kconfig_if": "NROS_RMW_ZENOH", "kconfig_attrs": ["depends on NROS_Q", "default y"]},
        {"id": "Y"},
    ]
    sec = render_kconfig_section(kc_rows)
    assert "# prompt: z\nconfig NROS_Z\n    int\n    default 4\n    help\n      a\n\n      b\n" in sec, sec
    assert ("if NROS_RMW_ZENOH\n\n# prompt: x\nconfig NROS_X\n    bool\n    depends on NROS_Q\n"
            "    default y\n\nendif # NROS_RMW_ZENOH") in sec, sec
    assert f'osource "{KCONFIG_GENERATED}"' in sec, "selftest: the section sources no generated file"
    assert sec.index("osource") < sec.index("\nconfig "), "selftest: the generated file must parse FIRST"
    assert '    int "' not in sec and '    bool "' not in sec, "selftest: a knob kept its prompt"
    pr = render_prompts(kc_rows)
    assert 'config NROS_Z\n    int "z"\n' in pr, pr
    assert 'if NROS_RMW_ZENOH\n\nconfig NROS_X\n    bool "x"\n    depends on NROS_Q\n\nendif' in pr, pr
    assert "default" not in pr and "NROS_Y" not in pr, pr
    rs = render(kc_rows)
    assert 'symbol: "NROS_X",\n        id: "ZENOH_X",\n        ty: "bool",\n        kconfig_if: Some("NROS_RMW_ZENOH"),\n        depends: &["NROS_Q"],' in rs, rs
    assert 'id: "Y",\n        ty' not in rs, "selftest: an untyped row reached KCONFIG_KNOBS"
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
    cm = render_cmake([{"id": "ZENOH_X", "legacy": ["ZPICO_X"], "type": "int"}, {"id": "Y"}])
    assert 'set(NROS_KNOB_LEGACY_NROS_ZENOH_X "ZPICO_X")' in cm, cm
    assert "set(NROS_KNOB_CANONICAL_ZPICO_X NROS_ZENOH_X)" in cm, cm
    assert 'set(NROS_KNOB_KCONFIG_IDS "ZENOH_X")' in cm, cm
    assert "set(NROS_KNOB_KCONFIG_SYMBOL_ZENOH_X NROS_ZENOH_X)" in cm, cm
    assert "set(NROS_KNOB_IMAGE_TABLE_ZENOH_X zenoh)" in cm, cm
    assert "set(NROS_KNOB_IMAGE_NAME_ZENOH_X x)" in cm, cm
    assert "NROS_Y" not in cm, "selftest: an untyped row with no legacy spelling reached the cmake table"
    # A RETIRED knob: every spelling refuses, none maps to a canonical name, no
    # Kconfig definition, and a typed retired row is itself refused.
    ret = [{"id": "ZENOH_GONE", "legacy": ["ZPICO_GONE"], "retired": "no task left"}]
    cm = render_cmake(ret)
    assert 'set(NROS_KNOB_RETIRED_SPELLINGS "NROS_ZENOH_GONE;ZPICO_GONE")' in cm, cm
    assert 'set(NROS_KNOB_RETIRED_REASON_ZPICO_GONE "no task left")' in cm, cm
    assert "NROS_KNOB_CANONICAL_ZPICO_GONE" not in cm, "selftest: a retired knob's old name maps to it"
    rs = render(ret)
    assert 'retired: Some("no task left"),' in rs, rs
    assert "ZENOH_GONE" not in render_kconfig_section(ret), "selftest: a retired knob got a symbol"
    _, errs = load_from_text('[[knob]]\nid = "ZENOH_GONE"\nretired = "x"\ntype = "int"\n')
    assert any("has no Kconfig definition" in e for e in errs), errs
    _, errs = load_from_text('[[knob]]\nid = "ZENOH_GONE"\nretired = "say \\"x\\""\n')
    assert any("cannot hold" in e for e in errs), errs


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
        if (PROMPTS_OUT.read_text() if PROMPTS_OUT.exists() else "") != render_prompts(rows):
            problems.append(
                f"{PROMPTS_OUT.relative_to(REPO)} differs from what knobs.toml renders (a guest's\n"
                "  knob prompts). Run `python3 scripts/gen-knob-registry.py`.")
        for msg in problems:
            sys.stderr.write(f"zephyr-kconfig-knobs: FAILED — {msg}\n")
        if problems:
            return 1
        n = sum(1 for r in rows if r.get("type"))
        print(f"zephyr-kconfig-knobs: OK — {n} knob symbol(s) rendered from knobs.toml, "
              "none defined outside the section, all prompt-less, guest prompts current "
              "(selftest: 4 negative controls caught)")
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
    PROMPTS_OUT.write_text(render_prompts(rows))
    print(f"wrote {PROMPTS_OUT.relative_to(REPO)}")
    print(f"wrote {KCONFIG_OUT.relative_to(REPO)} knob section "
          f"({sum(1 for r in rows if r.get('type'))} symbols)")
    return 0


if __name__ == "__main__":
    sys.exit(main())

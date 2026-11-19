#!/usr/bin/env python3
"""Generate the knob registry's Rust table from its TOML (RFC-0103 D2b, phase-484 W5a).

    python3 scripts/gen-knob-registry.py           # write
    python3 scripts/gen-knob-registry.py --check   # fail if the committed file differs

Input:  packages/tooling/nros-zephyr-build/knobs.toml   (core's rows) and each
        in-tree package's nros-knobs.toml (RFC-0140 D2, phase-581 W2), merged
        and read through scripts/lib/knob_registry.py
Output: packages/tooling/nros-zephyr-build/src/knob_registry.rs
        packages/tooling/nros-zephyr-build/src/config_rows.rs  (statement + env rows)
        packages/api/nros/src/retired_env.rs  (RETIRED_ENV_VARS, from `run:` retirements)
        packages/tooling/nros-zephyr-build/src/registry_packages.rs  (core owner
                                          words, which package declares which rows)
        cmake/NanoRosKnobRegistry.cmake   (the retired spellings, for the cmake lanes)
        zephyr/Kconfig                    (the knob section, between its markers)
        zephyr/Kconfig.knob-prompts       (a guest's prompts, phase-484 W5b)

The schema is RFC-0140 D1-D4 (phase-581 W1): a knob row authors `key`, and
`id`, `toml` and `kconfig` are derived from it (today's spellings that differ
sit in `transitional`); `retired` lists every replaced spelling by road. The
registry module normalises each knob row to the shape the renders below read
(`id`, `legacy`, `kconfig`, `toml`, `retired` reason), so the views are what
they were before the schema moved.

`nros-zephyr-build` is dependency-free on purpose (every build script that reads
a knob reaches it), so it cannot parse TOML at build time; the table is
generated and committed instead, and `just check knob-registry` holds the two
equal. Every spelling of a knob derives from its (normalised) row:

  env      `NROS_<id>`. Each retired `env:` spelling (the normalised row's
           `legacy`) is RETIRED (phase-484 W8): set, it stops the build naming
           `NROS_<id>`. The list stays as that error table for good: an
           unlisted old name would be ignored.
  Kconfig  `CONFIG_<kconfig>`, default `CONFIG_NROS_<id>`
  TOML     `[knobs.<key>]` on a `ladder = true` row, where a typed tenant of the
           ladder reads it. A board's `[knobs]` takes every row by its image
           key; a row without `ladder` is carried in the image's rows (issue
           2194).
  image    `[image.<id>.knobs.<owner>] <name>` in a project's system.toml
           (phase-484 W8): the row's `key`, so a project and a board spell a
           knob the same way. An `[image.<id>] env` row naming the knob is
           refused, in `IMAGE_ENV_RETIRED_TEMPLATE`'s words.

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

A row with `class = "retired"` and `instead = "<reason>"` is a knob that no longer EXISTS, not one that
was renamed (RFC-0110 D2 retired the zenoh lease-task priority when zenoh-pico
1.10 folded the lease task into its executor). Every spelling of it -- the
canonical one included -- stops the build with `RETIRED_KNOB`'s sentence, in a
build script, a configure and the CLI alike. It carries no Kconfig definition,
so a prj.conf that still assigns the symbol fails Zephyr's own undefined-symbol
check. The row stays for good as the error table, like its retired spellings.
"""
import re
import sys
from pathlib import Path

REPO = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(REPO / "scripts" / "lib"))
import knob_registry  # noqa: E402  -- the one reader of knobs.toml
SRC = REPO / "packages/tooling/nros-zephyr-build/knobs.toml"
OUT = REPO / "packages/tooling/nros-zephyr-build/src/knob_registry.rs"
CMAKE_OUT = REPO / "cmake/NanoRosKnobRegistry.cmake"
KCONFIG_OUT = REPO / "zephyr/Kconfig"
PROMPTS_OUT = REPO / "zephyr/Kconfig.knob-prompts"
ROWS_OUT = REPO / "packages/tooling/nros-zephyr-build/src/config_rows.rs"
RETIRED_ENV_OUT = REPO / "packages/api/nros/src/retired_env.rs"
PACKAGES_OUT = REPO / "packages/tooling/nros-zephyr-build/src/registry_packages.rs"
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
# The text names the release, never the design ids (RFC-0103 D2b, phase-484
# W8): it is a user's build error.
RETIRED = (
    "{legacy} is a retired spelling of {env} (retired after release 0.5.0-nros2 "
    "deprecated it) -- set {env} instead"
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
    "(refused after release 0.6.0 deprecated it) -- "
    "move the row there"
)

ID = re.compile(r"^[A-Z][A-Z0-9_]*$")
ENV = re.compile(r"^(NROS|ZPICO)_[A-Z0-9_]+$")
TOML_KEY = re.compile(r"^[a-z][a-z0-9_]*(\.[a-z][a-z0-9_]*)+$")
# phase-568 W2c -- a knob with no meaning off one platform (`only_on`), and
# why. Stated for another platform's image, it is refused rather than ignored.
ONLY_ON_PLATFORMS = {"zephyr"}
# The Kconfig definition is all-or-nothing: `type` says the module defines the
# symbol, and the rest only mean something beside it. `prompt` and `help` may
# also stand on an untyped row (phase-569 W2): the catalog shows them, and no
# Kconfig symbol is rendered from them.
KCONFIG_DEF_FIELDS = {"kconfig_if", "kconfig_attrs"}

# The catalog (phase-569 W2, RFC-0133). Two fields are AUTHORED, because each
# is a judgement no source states:
#   scope   what the value describes: `image` (what the application needs),
#           `board` (the hardware) or `platform` (the RTOS port).
#   class   `public` (a user states it, at its surface), `carrier` (the
#           transport of a decision made elsewhere; `carries` names where),
#           `internal` (a test or tooling hook, never user surface) or
#           `retired` (the row has `instead`).
# Three are DERIVED, never authored (the manager's ruling, 2026-11-17):
#   default  the Kconfig definition's `default` lines, each with its `if`
#            condition, in order (`KCONFIG`); else the platform/board ladder's
#            builtin for the row's `toml` key, which the CLI asks the resolver
#            for (`LADDER`); else nothing is stated (`UNSTATED`).
#   values   the Kconfig type with its `range` or `names`; an untyped ladder
#            row is an unsigned integer; else unstated.
#   surface  the one user spelling: `[image.<id>.knobs.<group>] <name>` for a
#            public row; none for any other class.
SCOPES = ("image", "board", "platform")
CLASSES = ("public", "carrier", "internal", "retired")
DERIVED_FIELDS = ("default", "values", "surface")

# Public rows the catalog cannot yet describe in full. Each list may only
# SHRINK: a new public row must arrive with a stated default and help text,
# and a row that gains one must leave its list (the generator refuses both a
# new member and a stale one).
#   NO_DEFAULT  no Kconfig definition and no ladder key, so its default lives
#               only in the code that reads it (a build script, codegen).
#   NO_HELP     no `help` text, so `nros knob show` has nothing to explain.
NO_DEFAULT = {
    "CYCLONEDDS_MAX_DESCRIPTOR_TYPES", "CYCLONEDDS_MAX_FIELDS", "CYCLONEDDS_MAX_KINDS",
    "CYCLONEDDS_MAX_NESTED_DEPTH", "CYCLONEDDS_MAX_TYPES",
    "XRCE_BUILTIN_SERVICE_REQUEST_BUFFER_SIZE", "XRCE_BUILTIN_SERVICE_SERVERS",
    "ZENOH_MAX_SESSIONS", "ZENOH_MAX_TL_PUBLISHERS", "ZENOH_PUBLISHER_TX_BUFFER_SIZE",
    "ZENOH_SUBSCRIBER_SIZE_THRESHOLD", "ZENOH_TL_RETAIN_BYTES", "ZENOH_TL_RETAIN_DEPTH",
    # The board's FreeRTOSConfig.h states this default, per board.
    "FREERTOS_TASK_STACK_GUARD",
}
NO_HELP = (NO_DEFAULT - {"FREERTOS_TASK_STACK_GUARD"}) | {
    "KEYEXPR_STRING_SIZE", "LET_BUFFER_SIZE", "RMW_MAX_BACKENDS", "RMW_MAX_NODES",
    "RMW_MESSAGE_INFO_SLOTS", "RMW_SUBSCRIBER_SLOT_BYTES", "RUNTIME_COMPONENT_SLOT_BYTES",
    "RUNTIME_MAX_CELL_ENTITIES", "RUNTIME_MAX_CLASS_INSTANCES", "RUNTIME_MAX_COMPONENTS",
    "SERVICE_TIMEOUT_MS", "SMOLTCP_BUFFER_SIZE", "SMOLTCP_CONNECT_TIMEOUT_MS",
    "SMOLTCP_MAX_SOCKETS", "SMOLTCP_MAX_UDP_SOCKETS", "SMOLTCP_SOCKET_TIMEOUT_MS",
    "ZENOH_BATCH_MULTICAST_SIZE", "ZENOH_CONNECT_FOREVER", "ZENOH_CONNECT_TIMEOUT_MS",
}


def load(path=SRC, packages=()):
    with open(path, "rb") as fh:
        return load_from_text(fh.read().decode(), strict_ratchets=(path == SRC), packages=packages)


def load_from_text(text, strict_ratchets=False, packages=()):
    """(knob rows normalised by `scripts/lib/knob_registry.py`, errors). The
    registry module checks the schema (keys, owners, transitional spellings,
    retired entries, statements, env rows, one namespace); the checks below are
    the generator's own, over the normalised knob rows."""
    reg, errors = load_registry(text, strict_ratchets, packages)
    return reg["knob"], errors


def load_registry(text, strict_ratchets=False, packages=()):
    """The merged registry: core's `text` plus `packages` (`()`: none, the
    self-test's shape; `None`: every in-tree package file, what `main`
    renders from -- RFC-0140 D2, phase-581 W2)."""
    reg, errors = knob_registry.parse(text, packages=packages)
    rows = reg["knob"]
    tomls = {}
    image_keys = {}
    for i, r in enumerate(rows):
        where = f"knob #{i + 1} ({r.get('id', '?')})"
        ident = r.get("id", "")
        if not ID.match(ident) or ident.startswith("NROS_"):
            errors.append(f"{where}: id must be the env name without `NROS_`")
        for s in ["NROS_" + ident] + list(r.get("legacy", [])):
            if not ENV.match(s):
                errors.append(f"{where}: `{s}` is not an env spelling")
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
                errors.append(f"{where}: `instead` is the reason, a non-empty string")
            elif re.search(r'["\\$;]', why):
                errors.append(f"{where}: a retired row's `instead` cannot hold `\"`, `\\`, `$` or `;` "
                              "(it is emitted verbatim into Rust and cmake strings)")
            if ty is not None:
                errors.append(f"{where}: a `retired` knob has no Kconfig definition (drop `type`), "
                              "so a conf that still assigns it fails as an undefined symbol")
        only = r.get("only_on")
        only_why = r.get("only_on_why")
        if (only is None) != (only_why is None):
            errors.append(f"{where}: `only_on` and `only_on_why` come together")
        if only is not None:
            if only not in ONLY_ON_PLATFORMS:
                errors.append(f"{where}: `only_on` must be one of {sorted(ONLY_ON_PLATFORMS)}")
            if not isinstance(only_why, str) or not only_why.strip() or re.search(r'["\\$;]', only_why):
                errors.append(f"{where}: `only_on_why` is a non-empty reason without `\"`, `\\`, `$` or `;`")
        t = r.get("toml")
        if t is not None:
            if t in tomls:
                errors.append(f"{where}: toml key `{t}` is already {tomls[t]}")
            tomls[t] = ident
        names = r.get("names")
        if names is not None and (ty != "int" or not names or not all(
                isinstance(n, str) and re.match(r"^[a-z][a-z0-9_]*$", n) for n in names)):
            errors.append(f"{where}: `names` is a non-empty list of lowercase words, on an int row")
        errors += catalog_problems(r, where)
        if ID.match(ident):
            key = image_key(r)
            if key in image_keys:
                errors.append(f"{where}: image key `{key}` is already {image_keys[key]}")
            else:
                image_keys[key] = ident
    # Each file's rows are sorted by id (the registry module checks it), and
    # the merged set is rendered in id order, so a row's place in a view does
    # not depend on which file declares it.
    ids = [r.get("id") for r in rows]
    known = set(ids)
    for r in rows:
        c = r.get("carries", "")
        if ID.match(c) and c not in known:
            errors.append(f"knob {r.get('id')}: `carries` names knob {c}, which is no row")
    if strict_ratchets:
        errors += ratchet_problems(rows)
    return reg, errors


def catalog_problems(r, where):
    """The catalog fields of one row (phase-569 W2): the authored pair present
    and valid, the derived three never authored, and every default statable."""
    out = []
    for f in DERIVED_FIELDS:
        if f in r:
            src = ("its Kconfig definition" if r.get("type")
                   else "the ladder's builtin" if r.get("toml") else "the code that reads it")
            out.append(f"{where}: `{f}` is derived, never authored -- it comes from {src}. "
                       "Drop the field")
    scope, cls = r.get("scope"), r.get("class")
    if scope not in SCOPES:
        out.append(f"{where}: `scope` must be one of {list(SCOPES)}")
    if cls not in CLASSES:
        out.append(f"{where}: `class` must be one of {list(CLASSES)}")
    if (cls == "retired") != bool(r.get("retired")):
        out.append(f"{where}: `class = \"retired\"` goes with an `instead` reason, and only with one")
    if (cls == "carrier") != bool(r.get("carries")):
        out.append(f"{where}: a carrier names what it carries (`carries`), and only a carrier does")
    if r.get("type"):
        try:
            kconfig_defaults(r)
        except ValueError as e:
            out.append(f"{where}: {e}")
    return out


def ratchet_problems(rows):
    """NO_DEFAULT and NO_HELP may only shrink."""
    out = []
    public = {r["id"]: r for r in rows if r.get("class") == "public"}
    no_default = {i for i, r in public.items() if default_kind(r) == "UNSTATED"}
    no_help = {i for i, r in public.items() if not r.get("help")}
    for name, have, allowed, missing, gained in (
        ("NO_DEFAULT", no_default, NO_DEFAULT,
         "no default (no Kconfig definition and no ladder key)", "states a default"),
        ("NO_HELP", no_help, NO_HELP, "no `help` text", "has `help` text"),
    ):
        for i in sorted(have - allowed):
            out.append(f"knob {i}: a public row with {missing}. State it; {name} only shrinks")
        for i in sorted(allowed - have):
            out.append(f"knob {i}: now {gained}; remove it from {name} in "
                       "scripts/gen-knob-registry.py")
    return out


def kconfig_defaults(r):
    """The row's Kconfig `default` lines as [(value, condition or None)], in
    order. Continuation lines (a trailing `\\`) are joined and comment lines
    skipped. Kconfig takes the FIRST default whose condition holds, so a
    conditional default is kept whole, never collapsed to one value."""
    lines, cur = [], ""
    for a in r.get("kconfig_attrs", []):
        s = a.strip()
        if not cur and s.startswith("#"):
            continue
        if s.endswith("\\"):
            cur += s[:-1].strip() + " "
            continue
        lines.append((cur + s).strip())
        cur = ""
    if cur:
        raise ValueError("`kconfig_attrs` ends inside a continued line")
    out = []
    for s in lines:
        if not s.startswith("default "):
            continue
        body = s[len("default "):].strip()
        m = re.match(r'^("(?:[^"\\]|\\.)*"|\S+)(?:\s+if\s+(.+))?$', body)
        if not m:
            raise ValueError(f"cannot read the Kconfig default `{s}`")
        out.append((m.group(1), m.group(2).strip() if m.group(2) else None))
    if r.get("type") and not out:
        raise ValueError("a Kconfig definition with no `default` line")
    if out and out[-1][1] is not None:
        raise ValueError("the last Kconfig `default` is conditional, so some configurations "
                         "have no default; end with an unconditional one")
    return out


def default_kind(r):
    if r.get("type"):
        return "KCONFIG"
    if r.get("toml"):
        return "LADDER"
    return "UNSTATED"


def surface(r):
    """The one user spelling of a public row, else None."""
    key = image_key(r)
    if r.get("class") != "public" or key is None:
        return None
    group, _, name = key.rpartition(".")
    return f"[image.<id>.knobs.{group}] {name}"


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
        only = r.get("only_on")
        out.append(f"        only_on: {'Some((' + q(only) + ', ' + q(r['only_on_why']) + '))' if only else 'None'},")
        out += render_catalog(r)
        out += render_retired_spellings(r.get("retired_entries", []), "        ")
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


def rs_str(s):
    """A Rust string literal for any text (help is multi-line, with quotes)."""
    return '"' + s.replace("\\", "\\\\").replace('"', '\\"').replace("\n", "\\n") + '"'


def rs_opt(s):
    return f"Some({rs_str(s)})" if s is not None else "None"


def values_kind(r):
    ty = r.get("type")
    if ty:
        return {"bool": "Bool", "int": "Int", "hex": "Hex", "string": "String"}[ty]
    return "UInt" if r.get("toml") else "Unstated"


def render_catalog(r):
    """The phase-569 W2 fields of one `KnobRow` literal."""
    out = [
        f"        scope: crate::KnobScope::{r.get('scope', 'image').capitalize()},",
        f"        class: crate::KnobClass::{r.get('class', 'public').capitalize()},",
        f"        carries: {rs_opt(r.get('carries'))},",
        f"        prompt: {rs_opt(r.get('prompt'))},",
        f"        help: {rs_opt(r['help'].strip() if r.get('help') else None)},",
    ]
    kind = default_kind(r)
    if kind == "KCONFIG":
        out.append("        default: crate::KnobDefault::Kconfig(&[")
        for v, cond in kconfig_defaults(r):
            out.append(f"            crate::KconfigDefault {{ value: {rs_str(v)}, when: {rs_opt(cond)} }},")
        out.append("        ]),")
    elif kind == "LADDER":
        out.append("        default: crate::KnobDefault::Ladder,")
    else:
        out.append("        default: crate::KnobDefault::Unstated,")
    out.append(f"        values: crate::KnobValues::{values_kind(r)},")
    out.append(f"        surface: {rs_opt(surface(r))},")
    return out


def render_retired_spellings(entries, indent):
    """`retired_spellings: &[...]` (RFC-0140 D4): every spelling the row
    replaced, by road, with the factor `nros fix` multiplies by."""
    if not entries:
        return [f"{indent}retired_spellings: &[],"]
    out = [f"{indent}retired_spellings: &["]
    for e in entries:
        out.append(f"{indent}    crate::RetiredSpelling {{")
        out.append(f"{indent}        road: crate::RetiredRoad::{e['road'].capitalize()},")
        out.append(f"{indent}        spelling: {rs_str(e['name'])},")
        out.append(f"{indent}        factor: {e['factor']},")
        out.append(f"{indent}        from_unit: {rs_opt(e['from_unit'])},")
        out.append(f"{indent}        instead: {rs_opt(e['instead'])},")
        out.append(f"{indent}    }},")
    out.append(f"{indent}],")
    return out


def render_rows(reg):
    """`config_rows.rs`: the statement and env rows (RFC-0140 D1), for the
    loader's in-memory catalog. Nothing reads a variable through this table
    yet (phase-581 W10 adds the reader); it is the catalog's source."""
    out = [
        "// @generated by scripts/gen-knob-registry.py from knobs.toml — do not edit.",
        "// Edit knobs.toml and re-run the script; `just check knob-registry` holds the two equal.",
        "",
        "use crate::{EnvRow, StatementRow};",
        "",
        "/// The `system.toml` selections (RFC-0140 D1), sorted by key.",
        "pub const STATEMENTS: &[StatementRow] = &[",
    ]
    for s in reg["statement"]:
        out.append("    StatementRow {")
        out.append(f"        key: {q(s['key'])},")
        out.append(f"        carrier: {rs_opt(s['carrier'])},")
        out.append(f"        kconfig: {rs_opt(s['kconfig'])},")
        out.append(f"        help: {rs_opt(s['help'])},")
        out += render_retired_spellings(s["retired_entries"], "        ")
        out.append("    },")
    out.append("];")
    out.append("")
    out.append("/// The run-time variables (RFC-0140 Part B), sorted by name.")
    out.append("pub const ENV_ROWS: &[EnvRow] = &[")
    for e in reg["env"]:
        out.append("    EnvRow {")
        out.append(f"        name: {q(e['name'])},")
        out.append(f"        key: {rs_opt(e['key'])},")
        out.append(f"        foreign: {rs_opt(e['foreign'])},")
        out.append(f"        pattern: {'true' if e['pattern'] else 'false'},")
        out.append(f"        disposition: crate::EnvDisposition::{e['disposition'].capitalize()},")
        out.append(f"        stated: {rs_opt(e['stated'])},")
        out.append(f"        parse: {rs_opt(e['parse'])},")
        out.append(f"        instead: {rs_opt(e['instead'])},")
        out.append(f"        refuse_if: {rs_opt(e['refuse_if'])},")
        out.append(f"        help: {rs_opt(e['help'])},")
        out += render_retired_spellings(e["retired_entries"], "        ")
        out.append("    },")
    out.append("];")
    out.append("")
    return "\n".join(out)


def render_packages(reg):
    """`registry_packages.rs`: core's owner words and which in-tree package
    declares which rows (RFC-0140 D2, phase-581 W2). The rows themselves are in
    KNOBS and ENV_ROWS; this says whose they are, so the loader can tell a
    scanned copy of an in-tree package from a different package."""
    out = [
        "// @generated by scripts/gen-knob-registry.py from knobs.toml and the in-tree",
        "// packages' nros-knobs.toml -- do not edit. `just check knob-registry` holds the",
        "// file equal to the render.",
        "",
        "use crate::InTreePackage;",
        "",
        "/// The owner words core holds (`[owners]`, plus words a rename retires), sorted.",
        "pub const CORE_OWNERS: &[&str] = &[",
    ]
    words = sorted(set(reg["owners"]) | knob_registry.OWNERS_PENDING_RENAME)
    out += [f"    {q(w)}," for w in words]
    out.append("];")
    out.append("")
    out.append("/// The in-tree packages that declare rows, by owner word. `file` is relative")
    out.append("/// to the checkout.")
    out.append("pub const IN_TREE_PACKAGES: &[InTreePackage] = &[")
    for p in sorted(reg["packages"], key=lambda p: p["owner"]):
        rel = knob_registry._rel(p["file"])
        knobs = [k["key"] for k in reg["knob"] if k.get("file") == rel]
        envs = [e["name"] for e in reg["env"] if e.get("file") == rel]
        out.append("    InTreePackage {")
        out.append(f"        package: {q(p['package'])},")
        out.append(f"        owner: {q(p['owner'])},")
        out.append(f"        file: {q(rel)},")
        for field, names in (("knobs", knobs), ("env", envs)):
            if not names:
                out.append(f"        {field}: &[],")
                continue
            out.append(f"        {field}: &[")
            out += [f"            {q(n)}," for n in names]
            out.append("        ],")
        out.append("    },")
    out.append("];")
    out.append("")
    return "\n".join(out)


def retired_env_pairs(reg):
    """(retired, replacement) for every MECHANICAL `run:` retirement of an env
    row: the pairs the hosted reader refuses today. A manual entry (`instead`)
    has no replacement name, and its refusal is phase-581 W13's."""
    return [(x["name"], e["name"]) for e in reg["env"] for x in e["retired_entries"]
            if x["road"] == "run" and x["instead"] is None]


def render_retired_env(reg):
    """`packages/api/nros/src/retired_env.rs`, the `RETIRED_ENV_VARS` view."""
    out = [
        "// @generated by scripts/gen-knob-registry.py from",
        "// packages/tooling/nros-zephyr-build/knobs.toml -- do not edit: each pair is",
        "// a `run:` entry in an `[[env]]` row's `retired` list, and the gate",
        "// `just check knob-registry` holds this file equal to the render.",
        "",
        "//! The RETIRED runtime env aliases and their canonical names.",
        "//!",
        "//! ONE table, read by the hosted env reader (`crate::env`). It was shared by",
        "//! two readers until they were merged.",
        "//! A set retired name is [`BootConfigError::RetiredEnvVar`], never a silent",
        "//! skip: an ignored `ZENOH_LOCATOR` dials the backend's DEFAULT router, which",
        "//! reads as \"no delivery\" rather than as a configuration mistake.",
        "//!",
        "//! The gate `check-retired-runtime-aliases` refuses any other tracked reader",
        "//! of these names, so this file is the one place they are spelled in code.",
        "//!",
        "//! [`BootConfigError::RetiredEnvVar`]: nros_node::BootConfigError::RetiredEnvVar",
        "",
        "/// `(retired, replacement)` pairs, in the order the readers check them.",
        "pub const RETIRED_ENV_VARS: &[(&str, &str)] = &[",
    ]
    out += [f"    ({q(a)}, {q(b)})," for a, b in retired_env_pairs(reg)]
    out.append("];")
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


# A registry's two header lines, for the self-test's fixtures.
HDR = 'owner = "core"\n\n[owners]\na = "x"\nb = "y"\n\n'


def self_test():
    import tempfile
    bad = HDR + (
        '[[knob]]\nkey = "b.x"\nretired = [{ spelling = "env:ZPICO_A" }]\n\n'
        '[[knob]]\nkey = "a.x"\ntransitional = { kconfig = "NROS_A_X" }\n'
        'retired = [{ spelling = "env:ZPICO_A" }]\ntoml = "x"\n'
    )
    with tempfile.NamedTemporaryFile("w", suffix=".toml", delete=False) as fh:
        fh.write(bad)
    _, errs = load(Path(fh.name))
    Path(fh.name).unlink()
    text = "\n".join(errs)
    for want in ("spelled by both", "is what `key` derives -- delete it",
                 "`toml` is derived from `key`, never stated", "sorted by id"):
        assert want in text, f"selftest: no `{want}` error in {errs!r}"
    # phase-581 W1 (RFC-0140 D4/D8) -- the retired entries, the env rows and
    # the statement each env row is compared with. NEGATIVE controls:
    _, errs = load_from_text(HDR + '[[knob]]\nkey = "a.x"\nscope = "image"\nclass = "internal"\n'
                             'retired = [{ spelling = "env:A_OLD", factor = 8, instead = "no" },'
                             ' { spelling = "nope:A" }, { spelling = "env:B", factor = 0 }]\n'
                             'legacy = ["X"]\n')
    text = "\n".join(errs)
    for want in ("states both `instead` and `factor`", "must start with one of",
                 "`factor` is a positive integer", "`legacy` became `retired"):
        assert want in text, f"selftest: no `{want}` error in {errs!r}"
    _, errs = load_from_text(HDR + '[[statement]]\nkey = "domain_id"\n\n'
                             '[[env]]\nname = "ROS_DOMAIN_ID"\nforeign = "ros2"\n'
                             'disposition = "respect"\nstated = "domian_id"\n\n'
                             '[[env]]\nkey = "a.x"\ndisposition = "respect"\n'
                             'transitional = { name = "NROS_A_X" }\n')
    text = "\n".join(errs)
    for want in ("`stated = \"domian_id\"` is neither a statement key nor a knob key",
                 "transitional `name = \"NROS_A_X\"` is what `key` derives", "sorted by variable name"):
        assert want in text, f"selftest: no `{want}` error in {errs!r}"
    _, errs = load_from_text(HDR + '[[knob]]\nkey = "nowhere.x"\nscope = "image"\nclass = "internal"\n')
    assert any("owner word `nowhere` is not in [owners]" in e for e in errs), errs
    good = HDR + ('[[knob]]\nkey = "a.y"\nscope = "image"\nclass = "internal"\n'
                  'retired = [{ spelling = "env:NROS_A_Y_KB", factor = 1024, from_unit = "KiB" }]\n\n'
                  '[[statement]]\nkey = "locator"\ncarrier = true\n\n'
                  '[[env]]\nkey = "locator"\ndisposition = "respect"\nstated = "locator"\n'
                  'retired = [{ spelling = "run:OLD_LOCATOR" }, { spelling = "run:GONE", instead = "x" }]\n')
    reg, errs = load_registry(good)
    assert not errs, errs
    assert retired_env_pairs(reg) == [("OLD_LOCATOR", "NROS_LOCATOR")], retired_env_pairs(reg)
    assert reg["knob"][0]["retired_entries"][0]["factor"] == 1024
    rs = render(reg["knob"])
    assert ("retired_spellings: &[\n            crate::RetiredSpelling {\n"
            "                road: crate::RetiredRoad::Env,\n"
            '                spelling: "NROS_A_Y_KB",\n                factor: 1024,\n'
            '                from_unit: Some("KiB"),\n') in rs, rs
    assert 'carrier: Some("NROS_ENTRY_LOCATOR"),' in render_rows(reg)
    assert '    ("OLD_LOCATOR", "NROS_LOCATOR"),' in render_retired_env(reg)
    # phase-484 W8 -- the image spelling: the board's `toml` key, else the id
    # split at its first `_`; two rows on one key, or `names` off an int row,
    # are refused.
    assert image_key({"id": "ZEPHYR_HEAP_SIZE"}) == "zephyr.heap_size"
    assert image_key({"id": "MAX_ARRAY_LEN", "toml": "params.max_array_len"}) == "params.max_array_len"
    _, errs = load_from_text(HDR +
        '[[knob]]\nkey = "zenoh.x"\ntransitional = { id = "A_B", kconfig = "NROS_A_B" }\nladder = true\n\n'
        '[[knob]]\nkey = "zenoh.x"\ntype = "bool"\nnames = ["off"]\n')
    text = "\n".join(errs)
    assert "`zenoh.x` is spelled by both knob #1 zenoh.x and knob #2 zenoh.x" in text, errs
    assert "image key `zenoh.x` is already A_B" in text, errs
    assert "`names` is a non-empty list" in text, errs
    rs = render([{"id": "LOG_X", "type": "int", "kconfig_attrs": ["range 0 6", "default 0"], "names": ["off"]}])
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
    _, errs = load_from_text(HDR + '[[knob]]\nkey = "a.b"\nkconfig_attrs = ["default 1"]\n')
    assert any("without `type`" in e for e in errs), errs
    # phase-569 W2 -- the catalog. A conditional Kconfig default is kept WHOLE,
    # continued lines joined and comments skipped (ZENOH_GRAPH_DISCOVERY's shape).
    gd = {"id": "ZENOH_GD", "type": "bool", "kconfig_attrs": [
        "# a comment", "default n if (A || B) && \\", "             !C && \\", "             !D",
        "default y"]}
    assert kconfig_defaults(gd) == [("n", "(A || B) && !C && !D"), ("y", None)], kconfig_defaults(gd)
    hs = {"id": "ZEPHYR_HEAP", "type": "int", "kconfig_attrs": [
        "default 1048576 if X", "default 196608 if Y || Z", "default 131072 if RUST", "default 65536"]}
    assert [v for v, _ in kconfig_defaults(hs)] == ["1048576", "196608", "131072", "65536"]
    assert kconfig_defaults({"id": "S", "type": "string", "kconfig_attrs": ['default "a b"']}) == [('"a b"', None)]
    rs = render([dict(gd, scope="image", **{"class": "public"})])
    assert ('default: crate::KnobDefault::Kconfig(&[\n'
            '            crate::KconfigDefault { value: "n", when: Some("(A || B) && !C && !D") },\n'
            '            crate::KconfigDefault { value: "y", when: None },\n        ]),') in rs, rs
    assert 'surface: Some("[image.<id>.knobs.zenoh] gd"),' in rs, rs
    # NEGATIVE controls: an authored derived field, a default that can run out,
    # a carrier with nothing carried, a bad scope.
    _, errs = load_from_text(HDR + '[[knob]]\nkey = "a.b"\nscope = "image"\nclass = "public"\n'
                             'type = "int"\nkconfig_attrs = ["default 1"]\ndefault = 2\n')
    assert any("`default` is derived, never authored -- it comes from its Kconfig definition" in e
               for e in errs), errs
    _, errs = load_from_text(HDR + '[[knob]]\nkey = "a.b"\nscope = "image"\nclass = "public"\n'
                             'type = "int"\nkconfig_attrs = ["default 1 if X"]\n')
    assert any("last Kconfig `default` is conditional" in e for e in errs), errs
    _, errs = load_from_text(HDR + '[[knob]]\nkey = "a.b"\nscope = "room"\nclass = "carrier"\n')
    assert any("`scope` must be" in e for e in errs) and any("names what it carries" in e for e in errs), errs
    assert rs_str('a "b"\\c\nd') == '"a \\"b\\"\\\\c\\nd"', rs_str('a "b"\\c\nd')
    rows = [{"id": "A_B", "scope": "image", "class": "public", "help": "h", "toml": "a.b"},
            {"id": "ZENOH_MAX_SESSIONS", "scope": "image", "class": "public"},
            {"id": "ZENOH_NEW", "scope": "image", "class": "public"}]
    probs = ratchet_problems(rows)
    assert not any("ZENOH_MAX_SESSIONS" in p for p in probs), probs
    assert any("ZENOH_NEW: a public row with no default (" in p for p in probs), probs
    assert any("ZENOH_NEW: a public row with no `help` text" in p for p in probs), probs
    assert any("CYCLONEDDS_MAX_TYPES: now states a default" in p for p in probs), probs
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
    _, errs = load_from_text(HDR + '[[knob]]\nkey = "zenoh.gone"\nclass = "retired"\ninstead = "x"\ntype = "int"\n')
    assert any("has no Kconfig definition" in e for e in errs), errs
    _, errs = load_from_text(HDR + '[[knob]]\nkey = "zenoh.gone"\nclass = "retired"\ninstead = "say \\"x\\""\n')
    assert any("cannot hold" in e for e in errs), errs
    # phase-581 W2 (RFC-0140 D2) -- package files. NEGATIVE controls, one per
    # rule: C1 two packages on one owner word, C2 a row under another word (and
    # a package taking a core word), C3 a spelling two files use, C4 another
    # library's variable.
    with tempfile.TemporaryDirectory() as tmp:
        def pkg(name, owner, body, package=None):
            f = Path(tmp) / name / "nros-knobs.toml"
            f.parent.mkdir()
            f.write_text(f'owner = "{owner}"\n\n' + body)
            return {"file": f, "package": package or name, "owner": owner}
        row = '[[knob]]\nkey = "{k}"\nscope = "image"\nclass = "internal"\n'
        core = HDR + row.format(k="a.x")
        one = pkg("acme_one", "acme", row.format(k="acme.x"))
        two = pkg("acme_two", "acme", row.format(k="acme.y"))
        _, errs = load_from_text(core, packages=[one, two])
        assert any("`acme` is claimed by two packages: acme_one" in e and "acme_two" in e
                   and str(one["file"]) in e and str(two["file"]) in e for e in errs), errs
        wrong = pkg("wrong", "wrong", row.format(k="acme.z"))
        corew = pkg("corew", "a", row.format(k="a.z"))
        _, errs = load_from_text(core, packages=[wrong, corew])
        assert any("key `acme.z` is not under this package's owner word `wrong`" in e for e in errs), errs
        assert any("owner word `a` is a core owner" in e for e in errs), errs
        dup = pkg("dup", "dup", row.format(k="dup.x") + 'retired = [{ spelling = "env:NROS_A_X" }]\n')
        _, errs = load_from_text(core, packages=[dup])
        assert any("`NROS_A_X` is spelled by both knob #1 a.x and knob #2 dup.x (" in e for e in errs), errs
        alien = pkg("alien", "alien", '[[env]]\nname = "ROS_X"\nforeign = "ros2"\ndisposition = "ignore"\n')
        _, errs = load_from_text(core, packages=[alien])
        assert any("declares only its own library's variables" in e for e in errs), errs
        _, errs = load_from_text(core, packages=[one])
        assert not errs, errs


def main():
    self_test()
    reg, errors = load_registry(SRC.read_text(), strict_ratchets=True, packages=None)
    rows = reg["knob"]
    if errors:
        sys.stderr.write("gen-knob-registry: FAILED — knobs.toml and the package files:\n")
        for e in errors:
            sys.stderr.write(f"    {e}\n")
        return 1
    outputs = [(OUT, render(rows)), (CMAKE_OUT, render_cmake(rows)),
               (ROWS_OUT, render_rows(reg)), (RETIRED_ENV_OUT, render_retired_env(reg)),
               (PACKAGES_OUT, render_packages(reg))]
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
        print(f"knob-registry: OK — {len(rows)} knob(s), {len(reg['statement'])} statement(s), "
              f"{len(reg['env'])} env row(s), generated tables current")
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

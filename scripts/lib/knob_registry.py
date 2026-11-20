"""The one Python reader of the configuration registry (RFC-0140 D1-D4, phase-581 W1).

The registry is core's `packages/tooling/nros-zephyr-build/knobs.toml` plus each
in-tree package's `nros-knobs.toml`, found beside its descriptor
(`nros-rmw.toml`, `nros-platform.toml`, `nros-board.toml`) under IN_TREE_DIRS
(RFC-0140 D2, phase-581 W2). A package file holds `owner = "<word>"`,
`[[knob]]` and `[[env]]` rows; its owner word is DERIVED from the package's
`package.xml` announcements (an RMW's or platform's first name, a board's key)
and checked, every key sits under it (C2), and a foreign variable it declares
is its own library's (C4). Two packages on one word (C1) and one spelling used
twice (C3) are refused. `parse()` returns the merged set, knob rows sorted by
id and env rows by name, so a view does not depend on which file holds a row.
The Rust loader (`nros_cli_core::knob_catalog`) applies the same rules to the
packages a workspace's provider scan finds.

The registry holds three row kinds in one namespace: `[[knob]]` (a build value with the RFC-0049 ladder), `[[statement]]`
(a `system.toml` selection) and `[[env]]` (a variable a running process reads).
Every script that needs the rows asks this module; none parses the file itself.

A knob row AUTHORS one name, `key` (`<owner>.<name>`), and every other spelling
derives from it: id `<KEY>` (dots to `_`, upper case), env `NROS_<id>`, Kconfig
`CONFIG_NROS_<id>`, TOML `key`. A row never states `id`, `toml` or `kconfig`.
Today's spellings that the naming rule would rename stay in the row's
`transitional` table until phases W5 to W7 rename them and empty it; a
transitional value equal to the derived one is refused, so the column cannot
outlive its rename. `ladder = true` says the platform/board ladder
(`nros-platform-config`'s `Knobs`) has a typed tenant at `key`.

`retired` holds every spelling a row replaced, tagged by road (`toml:`, `env:`,
`kconfig:`, `cmake:`, `run:`), as `{ spelling, factor, from_unit }` (a
mechanical rename: new = old x factor, factor absent = 1) or
`{ spelling, instead }` (manual: the text says what to do). Both `instead` and
`factor` on one entry is refused. A row with `class = "retired"` names what to
do instead in its own `instead`, and every entry on it is manual.

`knob_rows()` returns the knob rows NORMALISED to the shape the generated views
and the older gates read: `id`, `toml` (ladder rows), `kconfig` (only where it
differs from `NROS_<id>`), `legacy` (the `env:` retired spellings) and
`retired` (the reason, on a retired row), plus `key` and `retired_entries`.
"""

from __future__ import annotations

import re
import xml.etree.ElementTree as ET
from pathlib import Path

try:
    import tomllib
except ModuleNotFoundError:  # Python < 3.11 -- the repo's interpreter is 3.10
    import tomli as tomllib

REPO = Path(__file__).resolve().parents[2]
SRC = REPO / "packages/tooling/nros-zephyr-build/knobs.toml"

ROADS = ("toml", "env", "kconfig", "cmake", "run")
DISPOSITIONS = ("respect", "ignore", "refuse", "listed")
# C4: core declares the variables of ROS 2, rcutils and Rust; a library's own
# variable (`ZENOH_*`, `CYCLONEDDS_URI`) is its package's, `foreign = "<owner>"`.
FOREIGN_CORE = ("ros2", "rcutils", "rust")
PARSES = ("str", "int", "bool")

# Owner words of core rows the naming rule renames (W5/W6). Only shrinks; empty
# since phase-581 W6 renamed `param.*` to `params.*`.
OWNERS_PENDING_RENAME: set = set()

# RFC-0140 D2: a package declares its rows in `nros-knobs.toml` beside its
# descriptor; the descriptor says which kind of package it is. The in-tree
# packages live under these directories (RFC-0071 D14 for the RMW one).
PACKAGE_FILE = "nros-knobs.toml"
DESCRIPTORS = {"nros-rmw.toml": "rmw", "nros-platform.toml": "platform", "nros-board.toml": "board"}
IN_TREE_DIRS = ("packages/rmw", "packages/platform", "packages/boards")

KEY = re.compile(r"^[a-z][a-z0-9_]*(\.[a-z][a-z0-9_]*)+$")
WORD = re.compile(r"^[a-z][a-z0-9_]*$")
ENV_NAME = re.compile(r"^[A-Z][A-Z0-9_]*(<N>[A-Z0-9_]*)?$")
SYMBOL = re.compile(r"^[A-Z][A-Z0-9_]*$")
SPELLING = {
    "toml": re.compile(r"^[a-z][a-z0-9_]*(\.[a-z][a-z0-9_]*)+$"),
    "env": re.compile(r"^[A-Z][A-Z0-9_]*$"),
    "kconfig": re.compile(r"^CONFIG_[A-Z0-9_]+$"),
    "cmake": re.compile(r"^[A-Z][A-Z0-9_]*$"),
    "run": re.compile(r"^[A-Z][A-Z0-9_]*$"),
}

KNOB_FIELDS = {"key", "transitional", "ladder", "retired", "instead", "only_on", "only_on_why",
               "type", "prompt", "kconfig_if", "kconfig_attrs", "help", "names",
               "scope", "class", "carries"}
STATEMENT_FIELDS = {"key", "carrier", "zephyr", "transitional", "retired", "help"}
ENV_FIELDS = {"key", "name", "foreign", "pattern", "disposition", "stated", "parse",
              "instead", "refuse_if", "transitional", "retired", "help"}
KCONFIG_FIELDS = {"symbol", "env", "retired", "help"}
DERIVED_SPELLINGS = ("id", "toml", "kconfig")


def derive_id(key: str) -> str:
    """`zenoh.tx_batch` -> `ZENOH_TX_BATCH` (RFC-0140 D3 N2)."""
    return key.upper().replace(".", "_")


def read(path: Path = SRC, text: str | None = None) -> dict:
    if text is None:
        with open(path, "rb") as fh:
            return tomllib.load(fh)
    return tomllib.loads(text)


def _retired_entries(raw, where, row_is_retired, errors):
    """The row's `retired` list, validated, as dicts with `road`/`name`."""
    if raw is None:
        return []
    if not isinstance(raw, list):
        errors.append(f"{where}: `retired` is a list of {{ spelling = \"<road>:<name>\" }} entries")
        return []
    out = []
    for e in raw:
        if not isinstance(e, dict) or "spelling" not in e:
            errors.append(f"{where}: a retired entry is {{ spelling = \"<road>:<name>\", ... }}, not `{e!r}`")
            continue
        extra = set(e) - {"spelling", "factor", "from_unit", "instead"}
        if extra:
            errors.append(f"{where}: retired entry {e['spelling']!r}: unknown field(s) {sorted(extra)}")
        road, sep, name = str(e["spelling"]).partition(":")
        if not sep or road not in ROADS:
            errors.append(f"{where}: retired spelling `{e['spelling']}` must start with one of "
                          f"{', '.join(r + ':' for r in ROADS)}")
            continue
        if not SPELLING[road].match(name):
            errors.append(f"{where}: `{e['spelling']}` is not a {road} spelling")
        factor = e.get("factor")
        if factor is not None and (not isinstance(factor, int) or isinstance(factor, bool) or factor < 1):
            errors.append(f"{where}: `{e['spelling']}`: `factor` is a positive integer (new = old x factor)")
        if "instead" in e and "factor" in e:
            errors.append(f"{where}: `{e['spelling']}` states both `instead` and `factor`: an entry is "
                          "a mechanical rename (factor) or a manual one (instead), never both")
        if "from_unit" in e and "factor" not in e:
            errors.append(f"{where}: `{e['spelling']}`: `from_unit` only describes a `factor`")
        if row_is_retired and "factor" in e:
            errors.append(f"{where}: `{e['spelling']}`: a retired row has no replacement, so its "
                          "entries carry no `factor`")
        instead = e.get("instead")
        if instead is not None and (not isinstance(instead, str) or not instead.strip()):
            errors.append(f"{where}: `{e['spelling']}`: `instead` is non-empty text")
        out.append({"road": road, "name": name, "factor": factor if factor is not None else 1,
                    "from_unit": e.get("from_unit"), "instead": instead})
    return out


def _transitional(row, allowed, derived, where, errors):
    t = row.get("transitional")
    if t is None:
        return {}
    if not isinstance(t, dict) or not t:
        errors.append(f"{where}: `transitional` is a table of today's spellings")
        return {}
    extra = set(t) - set(allowed)
    if extra:
        errors.append(f"{where}: `transitional` holds only {sorted(allowed)}, not {sorted(extra)}")
    for k, v in t.items():
        if k in derived and v == derived[k]:
            errors.append(f"{where}: transitional `{k} = \"{v}\"` is what `key` derives -- delete it")
    return t


def _owner_problem(key, owners, where, file_owner=None):
    """RFC-0140 D2 C2: a core row's first key segment is a core owner word; a
    package row's is the package's own word."""
    owner = key.split(".")[0]
    if file_owner is not None:
        if owner == file_owner:
            return None
        return (f"{where}: key `{key}` is not under this package's owner word `{file_owner}` "
                "(a package declares only its own rows, RFC-0140 D2 C2)")
    if owner in owners or owner in OWNERS_PENDING_RENAME:
        return None
    return (f"{where}: owner word `{owner}` is not in [owners] (a core row's first key segment "
            "names a core owner, RFC-0140 D2)")


def _rel(path):
    if path is None:
        return "<text>"
    try:
        return str(Path(path).resolve().relative_to(REPO))
    except ValueError:
        return str(path)


# --- In-tree contributors (RFC-0140 D2, phase-581 W2) ------------------------

def _provisions(package_xml):
    """(`<name>`, [(kind, name)]) of a package.xml: the announcements the
    provider scan reads (`<nano_ros_provides kind=... name=...>`), in order."""
    root = ET.parse(package_xml).getroot()
    name = (root.findtext("name") or "").strip()
    provides = [(e.get("kind", ""), e.get("name", "")) for e in root.iter("nano_ros_provides")]
    return name, provides


def derive_owner(kind, provides, stated):
    """RFC-0140 D2's owner word of a package, or (None, reason). An RMW or
    platform package's word is its canonical name (the first it provides); a
    board package's is one of its board keys, the one its `owner` line names.
    `-` reads as `_`."""
    names = [n.replace("-", "_") for k, n in provides if k == kind]
    if not names:
        return None, f"the package provides no {kind} (no `<nano_ros_provides kind=\"{kind}\">`)"
    if kind == "board":
        if stated in names:
            return stated, None
        return None, f"a board package's owner is one of its board keys ({', '.join(names)})"
    return names[0], None


def in_tree_packages(repo=REPO):
    """Every in-tree package that declares rows: an `nros-knobs.toml` beside a
    descriptor under IN_TREE_DIRS. Returns ([{file, package, kind, owner}],
    errors). The Rust provider scan finds the same set (a unit test of
    `nros_cli_core::knob_catalog` holds the two equal)."""
    out, errors = [], []
    seen = set()
    for d in IN_TREE_DIRS:
        for pattern in ("*/" + PACKAGE_FILE, "*/*/" + PACKAGE_FILE):
            for f in sorted((repo / d).glob(pattern)):
                if f in seen:
                    continue
                seen.add(f)
                where = _rel(f)
                kinds = [k for desc, k in DESCRIPTORS.items() if (f.parent / desc).is_file()]
                if len(kinds) != 1:
                    errors.append(f"{where}: an nros-knobs.toml sits beside exactly one descriptor "
                                  f"({', '.join(DESCRIPTORS)}), found {len(kinds)}")
                    continue
                if not (f.parent / "package.xml").is_file():
                    errors.append(f"{where}: no package.xml beside it, so no scan finds the package")
                    continue
                package, provides = _provisions(f.parent / "package.xml")
                try:
                    stated = read(f).get("owner")
                except Exception as e:  # a parse error is reported by parse()
                    stated = None
                    errors.append(f"{where}: {e}")
                owner, why = derive_owner(kinds[0], provides, stated)
                if owner is None:
                    errors.append(f"{where}: {why}")
                    continue
                out.append({"file": f, "package": package, "kind": kinds[0], "owner": owner})
    return out, errors


def _parse_file(doc, where_file, owners, file_owner, errors):
    """The rows of ONE file (core when `file_owner` is None, else a package's),
    with the checks that need nothing outside it."""
    is_core = file_owner is None
    tag = "" if is_core else f" in {where_file}"
    knobs = []
    for i, r in enumerate(doc.get("knob", [])):
        where = f"knob #{i + 1} ({r.get('key', '?')}){tag}"
        bad = set(r) - KNOB_FIELDS
        for f in DERIVED_SPELLINGS:
            if f in r:
                errors.append(f"{where}: `{f}` is derived from `key`, never stated -- a spelling "
                              "that differs today goes in `transitional`")
        if "legacy" in r:
            errors.append(f"{where}: `legacy` became `retired = [{{ spelling = \"env:<name>\" }}]`")
        bad -= set(DERIVED_SPELLINGS) | {"legacy"}
        if bad:
            errors.append(f"{where}: unknown field(s) {sorted(bad)}")
        key = r.get("key", "")
        if not KEY.match(key):
            errors.append(f"{where}: `key` is `<owner>.<name>`, lower snake case")
            continue
        p = _owner_problem(key, owners, where, file_owner)
        if p:
            errors.append(p)
        if "ladder" in r and r["ladder"] is not True:
            errors.append(f"{where}: `ladder` is written only as `ladder = true`")
        did = derive_id(key)
        t = _transitional(r, ("id", "kconfig"), {"id": did}, where, errors)
        ident = t.get("id", did)
        if "kconfig" in t and t["kconfig"] == "NROS_" + did:
            errors.append(f"{where}: transitional `kconfig = \"{t['kconfig']}\"` is what `key` derives -- delete it")
        if ident != did and "kconfig" not in t:
            errors.append(f"{where}: transitional `id` without `kconfig`: today's symbol is "
                          f"NROS_{ident}, so state it (`kconfig = \"NROS_{ident}\"`)")
        kconfig = t.get("kconfig", "NROS_" + did)
        is_retired = r.get("class") == "retired"
        entries = _retired_entries(r.get("retired"), where, is_retired, errors)
        instead = r.get("instead")
        if instead is not None and not is_retired:
            errors.append(f"{where}: a row-level `instead` belongs to a `class = \"retired\"` row")
        if is_retired:
            for e in entries:
                if e["instead"] is None:
                    e["instead"] = instead
        n = {k: v for k, v in r.items()
             if k not in ("key", "transitional", "ladder", "retired", "instead")}
        n["id"] = ident
        if r.get("ladder"):
            n["toml"] = key
        if kconfig != "NROS_" + ident:
            n["kconfig"] = kconfig
        legacy = [e["name"] for e in entries if e["road"] == "env"]
        if legacy:
            n["legacy"] = legacy
        if instead is not None:
            n["retired"] = instead
        n["key"] = key
        n["retired_entries"] = entries
        n["file"] = where_file
        knobs.append(n)
    ids = [k["id"] for k in knobs]
    if ids != sorted(ids):
        errors.append(f"rows must be sorted by id{tag} (a merge then conflicts on one row, not the file)")

    statements = []
    for i, r in enumerate(doc.get("statement", [])):
        where = f"statement #{i + 1} ({r.get('key', '?')})"
        bad = set(r) - STATEMENT_FIELDS
        if bad:
            errors.append(f"{where}: unknown field(s) {sorted(bad)}")
        key = r.get("key", "")
        if not WORD.match(key):
            errors.append(f"{where}: a statement key is one lower-case word (`domain_id`)")
            continue
        for f in ("carrier", "zephyr"):
            if f in r and r[f] is not True:
                errors.append(f"{where}: `{f}` is written only as `{f} = true`")
        t = _transitional(r, ("kconfig",), {"kconfig": "NROS_" + key.upper()}, where, errors)
        if t and not r.get("zephyr"):
            errors.append(f"{where}: a transitional `kconfig` needs `zephyr = true`")
        statements.append({
            "key": key,
            "carrier": ("NROS_ENTRY_" + key.upper()) if r.get("carrier") else None,
            "kconfig": ("CONFIG_" + t.get("kconfig", "NROS_" + key.upper())) if r.get("zephyr") else None,
            "help": r.get("help"),
            "retired_entries": _retired_entries(r.get("retired"), where, False, errors),
        })
    st_keys = [s["key"] for s in statements]
    if st_keys != sorted(st_keys) or len(set(st_keys)) != len(st_keys):
        errors.append("statement rows must be sorted by key, each key once")

    envs = []
    for i, r in enumerate(doc.get("env", [])):
        where = f"env #{i + 1} ({r.get('key') or r.get('name', '?')}){tag}"
        bad = set(r) - ENV_FIELDS
        if bad:
            errors.append(f"{where}: unknown field(s) {sorted(bad)}")
        if ("key" in r) == ("name" in r):
            errors.append(f"{where}: an env row states `key` (a nano-ros variable, named "
                          "NROS_<KEY>) or `name` (a foreign or patterned one), exactly one")
            continue
        foreign = r.get("foreign")
        if foreign is not None:
            if is_core and foreign not in FOREIGN_CORE:
                errors.append(f"{where}: `foreign` is one of {list(FOREIGN_CORE)} in core; a "
                              "library's own variable is declared by its package (RFC-0140 D2 C4)")
            if not is_core and foreign != file_owner:
                errors.append(f"{where}: a package declares only its own library's variables "
                              f"(`foreign = \"{file_owner}\"`), not `{foreign}`'s (RFC-0140 D2 C4)")
        if "pattern" in r and r["pattern"] is not True:
            errors.append(f"{where}: `pattern` is written only as `pattern = true`")
        if not is_core and r.get("pattern"):
            errors.append(f"{where}: a patterned variable is core's (RFC-0140 D2)")
        if "key" in r:
            key = r["key"]
            if foreign is not None:
                errors.append(f"{where}: a foreign variable keeps its own `name`, it has no `key`")
            if WORD.match(key):
                if not is_core:
                    errors.append(f"{where}: a one-word key is a statement's override, which is core's")
                # resolved against the merged statements in parse()
            elif KEY.match(key):
                p = _owner_problem(key, owners, where, file_owner)
                if p:
                    errors.append(p)
            else:
                errors.append(f"{where}: `key` is a statement key or `<owner>.<name>`")
                continue
            derived = "NROS_" + derive_id(key)
            t = _transitional(r, ("name",), {"name": derived}, where, errors)
            name = t.get("name", derived)
        else:
            name = r["name"]
            if r.get("transitional"):
                errors.append(f"{where}: only a `key` row has a transitional `name`")
            if foreign is None and not r.get("pattern"):
                errors.append(f"{where}: a `name` row is foreign (`foreign = ...`) or patterned "
                              "(`pattern = true`); a nano-ros variable states `key`")
            if not ENV_NAME.match(name) or (("<N>" in name) != bool(r.get("pattern"))):
                errors.append(f"{where}: `{name}` is not an env name (`<N>` marks a patterned one)")
        disp = r.get("disposition")
        if disp not in DISPOSITIONS:
            errors.append(f"{where}: `disposition` is one of {list(DISPOSITIONS)}")
        if r.get("parse") is not None and r["parse"] not in PARSES:
            errors.append(f"{where}: `parse` is one of {list(PARSES)}")
        if disp == "refuse" and not r.get("instead"):
            errors.append(f"{where}: a refused variable says what to do `instead`")
        if r.get("refuse_if") is not None and disp != "refuse":
            errors.append(f"{where}: `refuse_if` belongs to a `disposition = \"refuse\"` row")
        envs.append({
            "name": name,
            "key": r.get("key"),
            "foreign": foreign,
            "pattern": bool(r.get("pattern")),
            "disposition": disp,
            "stated": r.get("stated"),
            "parse": r.get("parse"),
            "instead": r.get("instead"),
            "refuse_if": r.get("refuse_if"),
            "help": r.get("help"),
            "retired_entries": _retired_entries(r.get("retired"), where, False, errors),
            "file": where_file,
            "_where": where,
        })
    names = [e["name"] for e in envs]
    if names != sorted(names):
        errors.append(f"env rows must be sorted by variable name{tag} (a merge then conflicts on one row)")

    # `[[kconfig]]` (phase-581 W6): a hand-written `zephyr/Kconfig` symbol that
    # replaced others, holding their `kconfig:` retired spellings (RFC-0140 D4:
    # a retired spelling lives on what replaced it). Until phase-581 W9 folds
    # the hand symbols into knob rows, this is the row that replaced them.
    hand = []
    for i, r in enumerate(doc.get("kconfig", [])):
        where = f"kconfig #{i + 1} ({r.get('symbol', '?')})"
        bad = set(r) - KCONFIG_FIELDS
        if bad:
            errors.append(f"{where}: unknown field(s) {sorted(bad)}")
        sym = r.get("symbol", "")
        if not re.match(r"^CONFIG_NROS_[A-Z0-9_]+$", sym):
            errors.append(f"{where}: `symbol` is a CONFIG_NROS_* Kconfig symbol")
            continue
        env = r.get("env")
        if env is not None and not re.match(r"^NROS_[A-Z0-9_]+$", env):
            errors.append(f"{where}: `env` is the NROS_* build variable the module forwards it as")
        entries = _retired_entries(r.get("retired"), where, False, errors)
        if not entries:
            errors.append(f"{where}: a `[[kconfig]]` row exists to hold `retired` spellings")
        for e in entries:
            if e["road"] != "kconfig":
                errors.append(f"{where}: `{e['road']}:{e['name']}`: a hand Kconfig symbol retires "
                              "only `kconfig:` spellings")
            if e["factor"] != 1:
                errors.append(f"{where}: `{e['name']}`: a hand Kconfig rename carries no factor")
        hand.append({"symbol": sym, "env": env, "help": r.get("help"), "retired_entries": entries})
    syms = [h["symbol"] for h in hand]
    if syms != sorted(syms) or len(set(syms)) != len(syms):
        errors.append("kconfig rows must be sorted by symbol, each symbol once")
    if hand and not is_core:
        errors.append(f"`[[kconfig]]` rows are core's{tag}")
    return knobs, statements, envs, hand


def parse(text: str | None = None, path: Path = SRC, packages=None):
    """(registry, errors) of the merged in-tree registry: core's file (`text`,
    else `path`) plus each package file (`packages`: [{file, package, owner}],
    default every in-tree one, `in_tree_packages()`). `registry` has `owner`,
    `owners`, `packages`, `knob` (normalised rows, sorted by id), `statement`
    and `env` (sorted by name). Errors are sentences."""
    doc = read(path, text)
    errors = []
    if packages is None:
        packages, perr = in_tree_packages()
        errors += perr
    extra = set(doc) - {"owner", "owners", "knob", "statement", "env", "kconfig"}
    if extra:
        errors.append(f"unknown top-level key(s) {sorted(extra)}")
    owner = doc.get("owner")
    if owner != "core":
        errors.append("the core registry's `owner` line is `owner = \"core\"`")
    owners = doc.get("owners", {})
    if not isinstance(owners, dict) or not owners:
        errors.append("an `[owners]` table names the core owner words, one description each")
        owners = {}
    for w, d in owners.items():
        if not WORD.match(w) or not isinstance(d, str) or not d.strip():
            errors.append(f"[owners] `{w}`: a lower-case word with a one-line description")

    knobs, statements, envs, hand = _parse_file(doc, _rel(path), owners, None, errors)

    # Rule C1: one owner word, one package. C2: no package takes a core word.
    by_owner = {}
    for p in packages:
        rel = _rel(p["file"])
        if p["owner"] in owners:
            errors.append(f"{rel}: owner word `{p['owner']}` is a core owner ([owners] in "
                          f"{_rel(path)}); a package cannot declare a core owner's rows "
                          "(RFC-0140 D2 C2)")
        if p["owner"] in by_owner and by_owner[p["owner"]]["package"] != p["package"]:
            other = by_owner[p["owner"]]
            errors.append(f"owner word `{p['owner']}` is claimed by two packages: "
                          f"{other['package']} ({_rel(other['file'])}) and {p['package']} ({rel}) "
                          "(one owner, one package, RFC-0140 D2 C1)")
        by_owner.setdefault(p["owner"], p)
        pdoc = read(p["file"])
        pextra = set(pdoc) - {"owner", "knob", "env"}
        if pextra:
            errors.append(f"{rel}: a package file holds `owner`, `[[knob]]` and `[[env]]` rows, "
                          f"not {sorted(pextra)} (statements and [owners] are core's)")
        if pdoc.get("owner") != p["owner"]:
            errors.append(f"{rel}: `owner = \"{pdoc.get('owner')}\"`, but the package's derived "
                          f"owner word is `{p['owner']}` (RFC-0140 D2: stated so the file reads on "
                          "its own, checked so it cannot drift)")
        k, _, e, _ = _parse_file(pdoc, rel, owners, p["owner"], errors)
        knobs += k
        envs += e
    knobs.sort(key=lambda r: r["id"])
    envs.sort(key=lambda r: r["name"])

    st_keys = {s["key"] for s in statements}
    knob_keys = {k["key"] for k in knobs}
    for e in envs:
        key, where = e["key"], e.pop("_where")
        if key is not None and WORD.match(key) and key not in st_keys:
            errors.append(f"{where}: a one-word key is the run-time override of a statement "
                          f"(NROS_<KEY>), and `{key}` is no statement")
        stated = e["stated"]
        if stated is not None and stated not in st_keys and stated not in knob_keys:
            errors.append(f"{where}: `stated = \"{stated}\"` is neither a statement key nor a knob "
                          "key (RFC-0140 D8)")

    errors += spelling_collisions(knobs, statements, envs, hand)
    reg = {"owner": owner, "owners": owners, "packages": packages, "knob": knobs,
           "statement": statements, "env": envs, "kconfig": hand}
    return reg, errors


def _label(kind, n, name, file):
    core = file is None or file == _rel(SRC)
    return f"{kind} #{n} {name}" + ("" if core else f" ({file})")


def spellings(reg_or_parts):
    """[(spelling, row label)] of every spelled name: the live ones of each row
    and every retired one (RFC-0140 D1 one namespace, D2 rule C3)."""
    knobs, statements, envs, *rest = reg_or_parts
    hand = rest[0] if rest else []
    out = []
    for i, k in enumerate(knobs):
        label = _label("knob", i + 1, k["key"], k.get("file"))
        out.append(("NROS_" + k["id"], label))
        out.append(("CONFIG_" + k.get("kconfig", "NROS_" + k["id"]), label))
        out.append((k["key"], label))
        out += [(e["name"], label) for e in k["retired_entries"]]
    for i, s in enumerate(statements):
        label = f"statement #{i + 1} {s['key']}"
        out.append((s["key"], label))
        if s["carrier"]:
            out.append((s["carrier"], label))
        if s["kconfig"]:
            out.append((s["kconfig"], label))
        out += [(e["name"], label) for e in s["retired_entries"]]
    for i, e in enumerate(envs):
        label = _label("env", i + 1, e["name"], e.get("file"))
        out.append((e["name"], label))
        out += [(x["name"], label) for x in e["retired_entries"]]
    for i, h in enumerate(hand):
        label = f"kconfig #{i + 1} {h['symbol']}"
        out.append((h["symbol"], label))
        out += [(x["name"], label) for x in h["retired_entries"]]
    return out


def spelling_collisions(knobs, statements, envs, hand=()):
    seen, errors = {}, []
    for s, label in spellings((knobs, statements, envs, list(hand))):
        if s in seen and seen[s] != label:
            errors.append(f"`{s}` is spelled by both {seen[s]} and {label} (one spelling, one row)")
        seen.setdefault(s, label)
    return errors


def knob_rows(path: Path = SRC, text: str | None = None, packages=None):
    """The knob rows of the merged in-tree registry, normalised (see the module
    doc). Raises on a registry that does not parse, so a gate never reads half
    a table."""
    return registry(path, text, packages)["knob"]


def registry(path: Path = SRC, text: str | None = None, packages=None):
    reg, errors = parse(text, path, packages)
    if errors:
        raise ValueError("knob registry: " + "; ".join(errors))
    return reg


def registry_files(repo=REPO):
    """Core's file and every in-tree package file, core first: the files a
    writer that edits a row must choose among."""
    pkgs, errors = in_tree_packages(repo)
    if errors:
        raise ValueError("knob registry: " + "; ".join(errors))
    return [SRC] + [p["file"] for p in pkgs]


def file_knob_rows(path: Path, text: str | None = None):
    """The knob rows of ONE registry file (core's or a package's), normalised,
    in the file's order: for a writer that edits a row in place. Only the
    checks one file can answer run, and their errors are not raised; the
    merged `registry()` is what a reader trusts."""
    doc = read(path, text)
    owner = doc.get("owner")
    file_owner = None if owner == "core" else owner
    owners = doc.get("owners", {}) if file_owner is None else {}
    knobs, _, _, _ = _parse_file(doc, _rel(path), owners, file_owner, [])
    return knobs

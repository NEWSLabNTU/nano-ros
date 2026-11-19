"""The one Python reader of the configuration registry (RFC-0140 D1-D4, phase-581 W1).

`packages/tooling/nros-zephyr-build/knobs.toml` holds three row kinds in one
namespace: `[[knob]]` (a build value with the RFC-0049 ladder), `[[statement]]`
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
from pathlib import Path

try:
    import tomllib
except ModuleNotFoundError:  # Python < 3.11 -- the repo's interpreter is 3.10
    import tomli as tomllib

REPO = Path(__file__).resolve().parents[2]
SRC = REPO / "packages/tooling/nros-zephyr-build/knobs.toml"

ROADS = ("toml", "env", "kconfig", "cmake", "run")
DISPOSITIONS = ("respect", "ignore", "refuse", "listed")
FOREIGN = ("ros2", "rcutils", "rust", "zenoh", "cyclonedds")
PARSES = ("str", "int", "bool")

# Owner words of core rows that belong to an in-tree PACKAGE, not to core: the
# rows move to that package's `nros-knobs.toml` in phase-581 W2, which empties
# this set. And owner words the naming rule renames (W5/W6). Both only shrink.
PACKAGE_OWNERS_PENDING_MOVE = {"zenoh", "xrce", "cyclonedds", "freertos", "threadx", "zephyr"}
OWNERS_PENDING_RENAME = {"param"}

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


def _owner_problem(key, owners, where):
    owner = key.split(".")[0]
    if owner in owners or owner in PACKAGE_OWNERS_PENDING_MOVE or owner in OWNERS_PENDING_RENAME:
        return None
    return (f"{where}: owner word `{owner}` is not in [owners] (a core row's first key segment "
            "names a core owner, RFC-0140 D2)")


def parse(text: str | None = None, path: Path = SRC):
    """(registry, errors). `registry` has `owner`, `owners`, `knob` (normalised
    rows), `statement` and `env`. Errors are sentences."""
    doc = read(path, text)
    errors = []
    extra = set(doc) - {"owner", "owners", "knob", "statement", "env"}
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
        if w in PACKAGE_OWNERS_PENDING_MOVE:
            errors.append(f"[owners] `{w}` is a package's word (RFC-0140 D2), not a core one")

    knobs = []
    for i, r in enumerate(doc.get("knob", [])):
        where = f"knob #{i + 1} ({r.get('key', '?')})"
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
        p = _owner_problem(key, owners, where)
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
        knobs.append(n)

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

    knob_keys = {k["key"] for k in knobs}
    envs = []
    for i, r in enumerate(doc.get("env", [])):
        where = f"env #{i + 1} ({r.get('key') or r.get('name', '?')})"
        bad = set(r) - ENV_FIELDS
        if bad:
            errors.append(f"{where}: unknown field(s) {sorted(bad)}")
        if ("key" in r) == ("name" in r):
            errors.append(f"{where}: an env row states `key` (a nano-ros variable, named "
                          "NROS_<KEY>) or `name` (a foreign or patterned one), exactly one")
            continue
        foreign = r.get("foreign")
        if foreign is not None and foreign not in FOREIGN:
            errors.append(f"{where}: `foreign` is one of {list(FOREIGN)}")
        if "pattern" in r and r["pattern"] is not True:
            errors.append(f"{where}: `pattern` is written only as `pattern = true`")
        if "key" in r:
            key = r["key"]
            if foreign is not None:
                errors.append(f"{where}: a foreign variable keeps its own `name`, it has no `key`")
            if WORD.match(key):
                if key not in st_keys:
                    errors.append(f"{where}: a one-word key is the run-time override of a statement "
                                  f"(NROS_<KEY>), and `{key}` is no statement")
            elif KEY.match(key):
                p = _owner_problem(key, owners, where)
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
        stated = r.get("stated")
        if stated is not None and stated not in st_keys and stated not in knob_keys:
            errors.append(f"{where}: `stated = \"{stated}\"` is neither a statement key nor a knob "
                          "key (RFC-0140 D8)")
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
            "stated": stated,
            "parse": r.get("parse"),
            "instead": r.get("instead"),
            "refuse_if": r.get("refuse_if"),
            "help": r.get("help"),
            "retired_entries": _retired_entries(r.get("retired"), where, False, errors),
        })
    names = [e["name"] for e in envs]
    if names != sorted(names):
        errors.append("env rows must be sorted by variable name (a merge then conflicts on one row)")

    errors += spelling_collisions(knobs, statements, envs)
    reg = {"owner": owner, "owners": owners, "knob": knobs, "statement": statements, "env": envs}
    return reg, errors


def spellings(reg_or_parts):
    """[(spelling, row label)] of every spelled name: the live ones of each row
    and every retired one (RFC-0140 D1 one namespace, D2 rule C3)."""
    knobs, statements, envs = reg_or_parts
    out = []
    for i, k in enumerate(knobs):
        label = f"knob #{i + 1} {k['key']}"
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
        label = f"env #{i + 1} {e['name']}"
        out.append((e["name"], label))
        out += [(x["name"], label) for x in e["retired_entries"]]
    return out


def spelling_collisions(knobs, statements, envs):
    seen, errors = {}, []
    for s, label in spellings((knobs, statements, envs)):
        if s in seen and seen[s] != label:
            errors.append(f"`{s}` is spelled by both {seen[s]} and {label} (one spelling, one row)")
        seen.setdefault(s, label)
    return errors


def knob_rows(path: Path = SRC, text: str | None = None):
    """The knob rows, normalised (see the module doc). Raises on a registry
    that does not parse, so a gate never reads half a table."""
    reg, errors = parse(text, path)
    if errors:
        raise ValueError("knobs.toml: " + "; ".join(errors))
    return reg["knob"]


def registry(path: Path = SRC, text: str | None = None):
    reg, errors = parse(text, path)
    if errors:
        raise ValueError("knobs.toml: " + "; ".join(errors))
    return reg

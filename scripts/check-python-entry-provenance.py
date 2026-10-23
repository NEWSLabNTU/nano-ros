#!/usr/bin/env python3
"""A `[python.*]` entry names WHOSE dependency it is — issue 1484.

The maintainer's rule, arriving with issue 1483:

    Our index must not carry transitive dependencies. Where a package is a
    transitive dep of a direct apt ROS dependency, install the apt ROS package
    and let apt resolve it.

`nros-sdk-index.toml`'s `[python.*]` layer had no way to tell the two apart.
`catkin_pkg` and `PyYAML` are ours — 1 and 11 of this repo's own scripts import
them, several of the latter gates on the `check-fast` line — while `empy` and
`lark` are `rosidl_adapter`'s and `rosidl_parser`'s own `<exec_depend>`s, which
nothing here imports. Every entry's `why` read the same way regardless, so
"which of these are ours?" was a question only a fresh census could answer, and
that census kept being re-derived — three times in one day, wrongly twice.

Issue 1483 then moved those two OUT of `[python.*]`. They sit under
`[source.rosidl.python.<rosdep key>]`, beside the tree whose code needs them,
with no authored apt name. So the gate asks three things, every run:

  1. IS EACH `[python.*]` ENTRY OURS?  For each entry with no `check` command
     (the ones the index probes by IMPORTING their module, i.e. claims about
     the ambient interpreter), count direct importers in this repo's tracked
     `*.py` (`third-party/` and `docs/` excluded, parsed with `ast` rather than
     grepped). Zero importers is an upstream dependency posing as ours, and so
     is an entry carrying a `rosdep` key: both belong under the `[source.*]`
     whose code imports them.

  2. DOES EACH SOURCE KEY RESOLVE?  A `[source.*.python.<key>]` must be a key
     of the pinned `nros-rosdep-snapshot.toml`. That file, not the index, names
     the apt package (`python3-lark-parser` -> `python3-lark`).

  3. IS THE MIRROR UPSTREAM'S MANIFEST?  Each key must be an `<exec_depend>`
     of the `package.xml` its `from` names, and every `python3-*`
     `<exec_depend>` of those manifests must be mirrored. Read from the clone
     `nros setup --source <name>` provisions at the pinned `ref`
     (`$NROS_HOME/sources/<name>/<version>`). Without that clone the check is
     NOT VERIFIED through the skip ledger, never silently OK: this gate also
     runs on the pristine offline worktree, and a pin move is the only thing
     that can change the answer.

Run:  python3 scripts/check-python-entry-provenance.py [--self-test] [--census]
"""

import ast
import os
import subprocess
import sys

try:
    import tomllib
except ModuleNotFoundError:  # python3.10
    import tomli as tomllib

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
sys.path.insert(0, os.path.join(ROOT, "scripts", "lib"))
import check_skip  # noqa: E402  phase-472 F2 — the NOT VERIFIED ledger
INDEX = os.path.join(ROOT, "nros-sdk-index.toml")
SNAPSHOT = os.path.join(ROOT, "nros-rosdep-snapshot.toml")

# A census over OUR code, so upstream's vendored trees and prose examples in
# docs do not vote. `third-party/` is someone else's imports by definition,
# which is the very distinction this gate draws.
EXCLUDED_PREFIXES = ("third-party/", "docs/")


def module_name(entry):
    """The import name, derived exactly as `PythonDep::module()` does."""
    return entry.get("module") or entry["pip"].replace("-", "_")


def tracked_python_files(root):
    out = subprocess.run(
        ["git", "-C", root, "ls-files", "--", "*.py"],
        capture_output=True,
        text=True,
        check=True,
    )
    return [f for f in out.stdout.split() if not f.startswith(EXCLUDED_PREFIXES)]


def top_level_imports(source):
    """Every top-level package an `import` / `from ... import` in `source` names.

    `ast`, not a regex: a commented-out import is not an import, and
    `from .x import y` names no top-level package at all.
    """
    names = set()
    try:
        tree = ast.parse(source)
    except SyntaxError:
        return names
    for node in ast.walk(tree):
        if isinstance(node, ast.Import):
            for a in node.names:
                names.add(a.name.split(".")[0])
        elif isinstance(node, ast.ImportFrom) and node.level == 0 and node.module:
            names.add(node.module.split(".")[0])
    return names


def census(root, modules):
    """module -> [files in this repo that import it]."""
    hits = {m: [] for m in modules}
    for rel in tracked_python_files(root):
        try:
            with open(os.path.join(root, rel), encoding="utf8", errors="replace") as fh:
                names = top_level_imports(fh.read())
        except OSError:
            continue
        for m in names & set(modules):
            hits[m].append(rel)
    return hits


def declared_apt(entry):
    """The apt names an entry declares, flattened across the release table."""
    apt = entry.get("apt")
    if apt is None:
        return []
    if isinstance(apt, list):
        return list(apt)
    # `{ default = [..], <release> = [..] }` — every arm must be the snapshot's
    # answer, since the key does not vary by release.
    flat = []
    for names in apt.values():
        for n in names:
            if n not in flat:
                flat.append(n)
    return flat


def offenders(python_entries, snapshot_keys, hits):
    """Rule 1 over `[python.*]`, as (alias, what) — the testable core."""
    found = []
    for alias, entry in sorted(python_entries.items()):
        check = entry.get("check")
        is_tool = isinstance(check, dict) and "cmd" in check
        module = module_name(entry)
        if entry.get("rosdep"):
            found.append(
                (
                    "python.%s" % alias,
                    '`rosdep = "%s"` says this is upstream\'s dependency, so it is '
                    'not an entry of ours: move it to `[source.<name>.python.%s]` '
                    'under the tree whose code imports `%s` (issue 1483)'
                    % (entry["rosdep"], entry["rosdep"], module),
                )
            )
        elif not is_tool and not hits.get(module):
            found.append(
                (
                    "python.%s" % alias,
                    'no file in this repo imports `%s`, so it reads as ours and is '
                    'not. Move it under the `[source.*]` whose code needs it, keyed '
                    'by its rosdep key, or remove it and let apt resolve it behind '
                    'the ROS package that needs it' % module,
                )
            )
    return found


def exec_depends(xml_text):
    """The `<exec_depend>` names of one `package.xml` (attributes ignored)."""
    import xml.etree.ElementTree as ET

    root = ET.fromstring(xml_text)
    return {(e.text or "").strip() for e in root.iter("exec_depend")}


def source_offenders(sources, snapshot_keys, read_manifest):
    """Rules 2 and 3 over `[source.*.python]`, as (where, what).

    `read_manifest(name, version, rel)` returns the manifest's text, or None
    when that source is not provisioned here. Returns `(found, unverified)`,
    the second naming each source whose manifests could not be read.
    """
    found, unverified = [], []
    for name, src in sorted(sources.items()):
        deps = src.get("python") or {}
        if not deps:
            continue
        for key in sorted(deps):
            if key not in snapshot_keys:
                found.append(
                    (
                        "source.%s.python.%s" % (name, key),
                        "not a key in nros-rosdep-snapshot.toml, so nothing names "
                        "its apt package; an unresolvable key names no owner",
                    )
                )
        declared = {}
        for rel in sorted({d["from"] for d in deps.values()}):
            text = read_manifest(name, src.get("version"), rel)
            if text is None:
                unverified.append(name)
                declared = None
                break
            declared[rel] = exec_depends(text)
        if declared is None:
            continue
        for key, dep in sorted(deps.items()):
            if key not in declared[dep["from"]]:
                found.append(
                    (
                        "source.%s.python.%s" % (name, key),
                        "%s at %s declares no `<exec_depend>%s</exec_depend>`: the "
                        "mirror names an edge upstream does not have"
                        % (dep["from"], src.get("version"), key),
                    )
                )
        for rel, names in sorted(declared.items()):
            for key in sorted(n for n in names if n.startswith("python3-")):
                if key not in deps:
                    found.append(
                        (
                            "source.%s" % name,
                            "%s at %s declares `<exec_depend>%s</exec_depend>`, and "
                            "no `[source.%s.python.%s]` mirrors it: the ROS-less host "
                            "would be missing it" % (rel, src.get("version"), key, name, key),
                        )
                    )
    return found, unverified


def store_manifest_reader(store_root):
    """`read_manifest` over the store clones `nros setup --source` writes."""

    def read(name, version, rel):
        path = os.path.join(store_root, "sources", name, str(version), rel)
        try:
            with open(path, encoding="utf8") as fh:
                return fh.read()
        except OSError:
            return None

    return read


def self_test():
    """Negative controls — every rule fails on the shape it is about."""
    snap = {"python3-lark-parser": ["python3-lark"], "python3-empy": ["python3-empy"]}
    hits = {"yaml": ["scripts/x.py"], "lark": [], "em": [], "west": []}

    ours = {"pyyaml": {"pip": "PyYAML", "module": "yaml", "apt": ["python3-yaml"]}}
    assert offenders(ours, snap, hits) == [], offenders(ours, snap, hits)

    orphan = {"lark": {"pip": "lark", "apt": ["python3-lark"]}}
    assert len(offenders(orphan, snap, hits)) == 1, "an unimported entry passes"

    # The pre-1483 shape: upstream's, attributed, and still in OUR layer.
    attributed = {
        "lark": {"pip": "lark", "rosdep": "python3-lark-parser", "apt": ["python3-lark"]}
    }
    assert len(offenders(attributed, snap, hits)) == 1, "a rosdep entry in [python.*] passes"

    # A TOOL is an executable some verb installs, not a module we import — it
    # owes no importer (`west`, `clang-format`, `colcon`).
    tool = {"west": {"pip": "west", "apt_refused": "PyPI only", "check": {"cmd": "west"}}}
    assert offenders(tool, snap, hits) == [], offenders(tool, snap, hits)

    adapter = "<package><exec_depend>python3-empy</exec_depend><exec_depend>rosidl_cli</exec_depend></package>"
    parser = "<package><exec_depend>python3-lark-parser</exec_depend></package>"
    tree = {"rosidl_adapter/package.xml": adapter, "rosidl_parser/package.xml": parser}
    reader = lambda _n, _v, rel: tree.get(rel)  # noqa: E731
    good = {"rosidl": {"version": "v", "python": {
        "python3-empy": {"from": "rosidl_adapter/package.xml", "pip": "empy"},
        "python3-lark-parser": {"from": "rosidl_parser/package.xml", "pip": "lark"},
    }}}
    assert source_offenders(good, snap, reader) == ([], []), source_offenders(good, snap, reader)

    wrong_from = {"rosidl": {"version": "v", "python": {
        "python3-empy": {"from": "rosidl_parser/package.xml", "pip": "empy"},
        "python3-lark-parser": {"from": "rosidl_parser/package.xml", "pip": "lark"},
    }}}
    assert len(source_offenders(wrong_from, snap, reader)[0]) >= 1, "an edge upstream lacks passes"

    dropped = {"rosidl": {"version": "v", "python": {
        "python3-empy": {"from": "rosidl_adapter/package.xml", "pip": "empy"},
    }}}
    assert len(source_offenders(dropped, snap, reader)[0]) == 0, \
        "only the manifests a mirror names are read"
    tree["rosidl_adapter/package.xml"] = adapter.replace(
        "</package>", "<exec_depend>python3-new</exec_depend></package>")
    assert len(source_offenders(good, snap, reader)[0]) == 1, "an unmirrored upstream key passes"
    tree["rosidl_adapter/package.xml"] = adapter

    unknown = {"rosidl": {"version": "v", "python": {
        "python3-nope": {"from": "rosidl_adapter/package.xml", "pip": "x"}}}}
    assert any("snapshot" in w for _, w in source_offenders(unknown, snap, reader)[0]), \
        "an unresolvable key passes"

    absent = lambda *_: None  # noqa: E731
    assert source_offenders(good, snap, absent) == ([], ["rosidl"]), \
        "an unprovisioned clone is NOT VERIFIED, not OK"

    # The census reads imports, not prose.
    assert top_level_imports("import yaml\n") == {"yaml"}
    assert top_level_imports("# import yaml\n") == set()
    assert top_level_imports("from catkin_pkg.package import parse_package\n") == {
        "catkin_pkg"
    }
    assert top_level_imports("from .local import x\n") == set()

    print("check-python-entry-provenance self-test: OK (14 cases)")
    return 0


def main():
    # On the NORMAL path, not behind the flag: a negative control nobody runs
    # decays into a comment, and this gate's controls are the only thing that
    # says its two rules still bite.
    self_test()
    if "--self-test" in sys.argv:
        return 0

    with open(INDEX, "rb") as fh:
        index = tomllib.load(fh)
    with open(SNAPSHOT, "rb") as fh:
        snapshot = tomllib.load(fh)

    python_entries = index.get("python", {})
    if not python_entries:
        sys.stderr.write(
            "check-python-entry-provenance: FAILED — no [python.*] entries in "
            "%s. A gate that finds nothing to check reports OK forever.\n" % INDEX
        )
        return 1

    snapshot_keys = {
        key: list(body.get("apt", [])) for key, body in snapshot.get("key", {}).items()
    }
    modules = {module_name(e) for e in python_entries.values()}
    hits = census(ROOT, modules)

    if "--census" in sys.argv:
        for alias, entry in sorted(python_entries.items()):
            m = module_name(entry)
            print("%-14s %-12s %d importer(s) %s" % (alias, m, len(hits[m]), hits[m][:3]))
        return 0

    found = offenders(python_entries, snapshot_keys, hits)
    store = os.environ.get("NROS_HOME") or os.path.join(os.path.expanduser("~"), ".nros")
    src_found, unverified = source_offenders(
        index.get("source", {}), snapshot_keys, store_manifest_reader(store)
    )
    found += src_found
    if found:
        sys.stderr.write("check-python-entry-provenance: FAILED\n\n")
        for alias, what in found:
            sys.stderr.write("  [%s]: %s\n\n" % (alias, what))
        sys.stderr.write(
            "  The rule (issues 1483/1484): our index must not carry another\n"
            "  project's dependencies as if they were ours. Where apt can\n"
            "  resolve them behind a ROS package, install that instead; where\n"
            "  it cannot — the ROS-LESS host [source.rosidl] serves — say whose\n"
            "  they are and take the name from the pinned rosdep snapshot.\n"
            "  Census:  python3 scripts/check-python-entry-provenance.py --census\n"
        )
        return 1

    ours = sum(1 for e in python_entries.values() if hits[module_name(e)])
    upstream = sum(len(s.get("python") or {}) for s in index.get("source", {}).values())
    print(
        "check-python-entry-provenance: OK ([python.*]: %d entries, %d imported "
        "here, the rest tool-probed; [source.*.python]: %d upstream key(s))"
        % (len(python_entries), ours, upstream)
    )
    if unverified:
        return check_skip.unverified(
            "python-entry-provenance",
            "no provisioned clone of [source.%s] under %s/sources, so its "
            "python mirror was not compared with upstream's package.xml "
            "(nros setup --source %s)" % (unverified[0], store, unverified[0]),
        )
    return 0


if __name__ == "__main__":
    sys.exit(main())

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

So the gate asks it, from the tree, every run:

  1. WHOSE IS IT?  For each entry with no `check` command — the ones the index
     probes by IMPORTING their module, i.e. claims about the ambient
     interpreter — count direct importers in this repo's tracked `*.py`
     (`third-party/` and `docs/` excluded, parsed with `ast` rather than
     grepped). Zero importers and no `rosdep` key is the state the rule
     forbids: an entry that reads as ours and is not.

  2. DOES THE UPSTREAM NAME COME FROM UPSTREAM?  An entry that declares
     `rosdep = "<key>"` must resolve, through the pinned vendored
     `nros-rosdep-snapshot.toml`, to EXACTLY the `apt` names it declares.
     `[python.lark]` is why: upstream's key is `python3-lark-parser` and the apt
     package is `python3-lark`, so the entry was hand-copying one edge of a
     rename whose owner publishes it. A hand-copy drifts in the direction that
     reads as correct; this comparison is what makes that impossible.

WHY THE TWO UPSTREAM ENTRIES ARE STILL HERE AT ALL, since the rule says install
the ROS package instead: `[source.rosidl]` exists for the host that HAS NO ROS
APT REPO (issue 0368 / phase-327), where there is no `ros-humble-rosidl-adapter`
to install and nothing for apt to resolve. The runner container is exactly that
host. So the honest end state is not silence — it is an entry that says whose
dependency it is and takes the name from whoever owns it.

NOT CHECKED HERE, because it needs the network: that the `rosdep` keys are the
ones upstream's own `package.xml` declares at the pinned `ref`. Measured by hand
at `humble-5621b26` (rosidl_adapter -> `python3-empy`, rosidl_parser ->
`python3-lark-parser`) and recorded in `[source.rosidl]`'s comment; re-measuring
it is a pin-move action, like `gen-rosdep-snapshot.py --verify`. A gate that
reaches the network fails on the pristine offline worktree this tier supports.

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
    """Every violation, as (alias, what) — the testable core."""
    found = []
    for alias, entry in sorted(python_entries.items()):
        rosdep = entry.get("rosdep")
        check = entry.get("check")
        is_tool = isinstance(check, dict) and "cmd" in check
        module = module_name(entry)
        importers = hits.get(module, [])

        if not is_tool and not importers and not rosdep:
            found.append(
                (
                    alias,
                    'no file in this repo imports `%s`, and the entry names no '
                    'upstream `rosdep` key — so it reads as ours and is not. '
                    'Either add `rosdep = "<upstream key>"` (whose dependency '
                    'it is, with the apt name taken from the pinned snapshot), '
                    'or remove the entry and let apt resolve it behind the ROS '
                    'package that needs it' % module,
                )
            )
        if rosdep is None:
            continue
        if rosdep not in snapshot_keys:
            found.append(
                (
                    alias,
                    '`rosdep = "%s"` is not a key in nros-rosdep-snapshot.toml '
                    '— an unresolvable key names no owner' % rosdep,
                )
            )
            continue
        want = snapshot_keys[rosdep]
        got = declared_apt(entry)
        if got != want:
            found.append(
                (
                    alias,
                    '`rosdep = "%s"` resolves to %s in the pinned snapshot, but '
                    'the entry declares apt = %s. The key is upstream\'s and the '
                    'name is the snapshot\'s answer to it; a hand-written third '
                    'spelling is the drift this field exists to stop'
                    % (rosdep, want, got),
                )
            )
    return found


def self_test():
    """Negative controls — every rule fails on the shape it is about."""
    snap = {"python3-lark-parser": ["python3-lark"], "python3-empy": ["python3-empy"]}
    hits = {"yaml": ["scripts/x.py"], "lark": [], "em": [], "west": []}

    ours = {"pyyaml": {"pip": "PyYAML", "module": "yaml", "apt": ["python3-yaml"]}}
    assert offenders(ours, snap, hits) == [], offenders(ours, snap, hits)

    orphan = {"lark": {"pip": "lark", "apt": ["python3-lark"]}}
    assert len(offenders(orphan, snap, hits)) == 1, "an unimported entry with no owner passes"

    attributed = {
        "lark": {"pip": "lark", "rosdep": "python3-lark-parser", "apt": ["python3-lark"]}
    }
    assert offenders(attributed, snap, hits) == [], offenders(attributed, snap, hits)

    # The rename, hand-copied wrong — the case the whole field is for.
    drifted = {
        "lark": {
            "pip": "lark",
            "rosdep": "python3-lark-parser",
            "apt": ["python3-lark-parser"],
        }
    }
    assert len(offenders(drifted, snap, hits)) == 1, "a drifted apt name passes"

    unknown = {"x": {"pip": "x", "rosdep": "python3-nope", "apt": ["python3-x"]}}
    assert len(offenders(unknown, snap, hits)) == 1, "an unresolvable key passes"

    # A TOOL is an executable some verb installs, not a module we import — it
    # owes no importer (`west`, `clang-format`, `colcon`).
    tool = {"west": {"pip": "west", "apt_refused": "PyPI only", "check": {"cmd": "west"}}}
    assert offenders(tool, snap, hits) == [], offenders(tool, snap, hits)

    # The census reads imports, not prose.
    assert top_level_imports("import yaml\n") == {"yaml"}
    assert top_level_imports("# import yaml\n") == set()
    assert top_level_imports("from catkin_pkg.package import parse_package\n") == {
        "catkin_pkg"
    }
    assert top_level_imports("from .local import x\n") == set()

    print("check-python-entry-provenance self-test: OK (8 cases)")
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
    if found:
        sys.stderr.write("check-python-entry-provenance: FAILED\n\n")
        for alias, what in found:
            sys.stderr.write("  [python.%s]: %s\n\n" % (alias, what))
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
    upstream = sum(1 for e in python_entries.values() if e.get("rosdep"))
    print(
        "check-python-entry-provenance: OK (%d entries: %d imported here, %d "
        "attributed upstream by rosdep key, the rest tool-probed)"
        % (len(python_entries), ours, upstream)
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())

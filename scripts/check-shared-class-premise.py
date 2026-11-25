#!/usr/bin/env python3
"""issue 2840 (c2) — the premise a lane-wide class root rests on, checked.

`NROS_BUILD_CLASS_ROOT` (`builder::shared_target`) gives every image of one
build class ONE cargo `build.build-dir`, across cargo roots. Cargo names a
unit by `-C metadata`, which hashes a path package's location: RELATIVE to the
workspace root when the package is inside it, ABSOLUTE otherwise. So sharing
is safe on exactly two facts, and this gate checks both over every cargo row
the fixture lane builds (`examples/fixtures.toml` `[[workspace_fixture]]`,
`lang = "rust"`):

(a) ONE SPELLING. Every path dependency that leaves its leaf is reached by a
    lexical path that is already its real path (no symlink in the spelling)
    and lies inside no lane leaf. Two spellings of one crate are two units of
    it side by side — issue 0616, with `nros-platform`'s `#[global_allocator]`
    in both.
(b) NO SHARED ROOT UNIT. A root package is hashed root-relative, i.e. by name
    and version alone, so two roots that are equal in both would write one
    unit. A root is distinct when it depends on a package of its OWN leaf that
    lies outside the root (hashed by that absolute path), or when its
    (name, version) is unique among the lane's roots. A generated entry
    (`<image>_entry`, version 0.0.0) depends on its leaf's member packages, so
    it is distinct while the leaf has one.

Measured 2026-11-25 (host 118): three leaves of one class in one build dir
gave each nros crate the same unit identity in every leaf.

Buildless: reads tracked manifests only. Exit 1 on a violation.
"""

import os
import subprocess
import sys

try:
    import tomllib
except ModuleNotFoundError:  # Python < 3.11
    import tomli as tomllib

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
DEP_TABLES = ("dependencies", "dev-dependencies", "build-dependencies")


def path_deps(doc):
    """Every `path =` a manifest names: dependency tables, per-target tables
    and `[patch.*]`."""
    out = []

    def table(t):
        for spec in (t or {}).values():
            if isinstance(spec, dict) and "path" in spec:
                out.append(spec["path"])

    for k in DEP_TABLES:
        table(doc.get(k))
    for tgt in (doc.get("target") or {}).values():
        for k in DEP_TABLES:
            table(tgt.get(k))
    for src in (doc.get("patch") or {}).values():
        table(src)
    return out


def inside(path, d):
    return path == d or path.startswith(d.rstrip(os.sep) + os.sep)


def check(leaves, manifests, realpath=os.path.realpath):
    """`leaves`: absolute leaf dirs. `manifests`: {abs Cargo.toml: parsed doc}.
    Returns a list of problem strings."""
    problems = []
    roots = []  # (leaf, root manifest or None, name, version, distinct)
    for leaf in leaves:
        own = {m: d for m, d in manifests.items() if inside(m, leaf)}
        members = 0
        leaf_roots = []
        for m, doc in sorted(own.items()):
            mdir = os.path.dirname(m)
            pkg = doc.get("package") or {}
            is_root = "workspace" in doc and "package" in doc
            if "package" in doc and not is_root:
                members += 1
            local_outside_root = False
            for p in path_deps(doc):
                target = os.path.normpath(os.path.join(mdir, p))
                if inside(target, leaf):
                    if is_root and not inside(target, mdir):
                        local_outside_root = True
                    continue
                rel = os.path.relpath(m, ROOT)
                if realpath(target) != target:
                    problems.append(
                        f"{rel}: `{p}` reaches {target} through a symlink "
                        f"({realpath(target)}): a second spelling is a second unit (issue 0616)"
                    )
                for other in leaves:
                    if other != leaf and inside(target, other):
                        problems.append(
                            f"{rel}: `{p}` reaches into another lane leaf "
                            f"({os.path.relpath(other, ROOT)}), which hashes it root-relative "
                            "while this leaf hashes it absolutely: two units of one crate"
                        )
            if is_root:
                leaf_roots.append(
                    (leaf, m, pkg.get("name"), str(pkg.get("version", "0.0.0")), local_outside_root)
                )
        if leaf_roots:
            roots.extend(leaf_roots)
        else:
            # A generated entry: distinct while it has a member to depend on.
            roots.append((leaf, None, "<image>_entry", "0.0.0", members > 0))
    seen = {}
    for r in roots:
        seen.setdefault((r[2], r[3]), []).append(r)
    for (name, ver), rs in sorted(seen.items()):
        if len(rs) < 2:
            continue
        for leaf, m, _, _, distinct in rs:
            if not distinct:
                where = os.path.relpath(m or leaf, ROOT)
                problems.append(
                    f"{where}: root `{name}` {ver} is shared by {len(rs)} lane roots and depends "
                    "on no package of its own leaf, so cargo gives every one of them ONE unit "
                    "in a shared build dir. Add a leaf-local dependency, or rename it."
                )
    return problems


def lane_leaves():
    with open(os.path.join(ROOT, "examples/fixtures.toml"), "rb") as fh:
        m = tomllib.load(fh)
    return sorted(
        {
            os.path.join(ROOT, r["dir"])
            for r in m.get("workspace_fixture", [])
            if r.get("lang") == "rust" and r.get("dir")
        }
    )


def tracked_manifests(leaves):
    rels = [os.path.relpath(leaf, ROOT) for leaf in leaves]
    out = subprocess.run(
        ["git", "-C", ROOT, "ls-files", "-z", "--"] + [f"{r}/**Cargo.toml" for r in rels]
        + [f"{r}/Cargo.toml" for r in rels],
        check=True,
        capture_output=True,
    ).stdout.decode()
    docs = {}
    for rel in sorted(set(filter(None, out.split("\0")))):
        p = os.path.join(ROOT, rel)
        with open(p, "rb") as fh:
            docs[p] = tomllib.load(fh)
    return docs


def self_test():
    shared = os.path.join(ROOT, "packages/core/nros-core")
    a, b = "/L/a", "/L/b"
    ok_root = {"package": {"name": "a", "version": "0.1.0"}, "workspace": {},
               "dependencies": {"x": {"path": "../../generated/x"}, "c": {"path": os.path.relpath(shared, "/L/a/src/a")}}}
    cases = [
        # a clean leaf-root with a leaf-local dep
        ([a], {"/L/a/src/a/Cargo.toml": ok_root}, None, 0),
        # a symlinked spelling of a shared crate
        ([a], {"/L/a/src/a/Cargo.toml": ok_root}, lambda p: p + "-real" if p == shared else p, 1),
        # two roots of one name, neither with a leaf-local dep
        ([a, b], {
            "/L/a/src/r/Cargo.toml": {"package": {"name": "r", "version": "0.1.0"}, "workspace": {}},
            "/L/b/src/r/Cargo.toml": {"package": {"name": "r", "version": "0.1.0"}, "workspace": {}},
        }, None, 2),
        # two generated entries, each leaf with a member: distinct
        ([a, b], {
            "/L/a/src/m/Cargo.toml": {"package": {"name": "m", "version": "0.1.0"}},
            "/L/b/src/m/Cargo.toml": {"package": {"name": "m", "version": "0.1.0"}},
        }, None, 0),
        # a dep reaching into another lane leaf
        ([a, b], {"/L/a/src/a/Cargo.toml": {"package": {"name": "a"}, "workspace": {},
                                            "dependencies": {"y": {"path": "../../../b/src/y"}}}}, None, 1),
    ]
    for i, (leaves, mans, rp, want) in enumerate(cases):
        got = check(leaves, mans, realpath=rp or (lambda p: p))
        if len(got) != want:
            sys.exit(f"check-shared-class-premise self-test case {i}: want {want} problem(s), got {got}")
    print(f"check-shared-class-premise self-test: OK ({len(cases)} cases)")


def main():
    self_test()
    leaves = lane_leaves()
    problems = check(leaves, tracked_manifests(leaves))
    if problems:
        sys.stderr.write(
            f"check-shared-class-premise: {len(problems)} problem(s) with sharing one build dir "
            "per class across lane leaves (issue 2840):\n\n"
        )
        for p in problems:
            sys.stderr.write(f"  {p}\n")
        return 1
    print(f"check-shared-class-premise: OK — {len(leaves)} lane leaves, one spelling per shared crate, no shared root unit")
    return 0


if __name__ == "__main__":
    sys.exit(main())

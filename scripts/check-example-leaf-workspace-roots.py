#!/usr/bin/env python3
"""issue 0948 — every tracked example leaf resolves against a workspace root.

# The rule

A `[package]` manifest under `examples/` must resolve to SOME workspace root.
Cargo walks up from the leaf to the nearest ancestor manifest carrying a
`[workspace]` table; if that walk lands on the REPO root, the leaf has to be
named there — in `members` or in `exclude`. A leaf that reaches the repo root
while named in neither is STRANDED, and every cargo command in it fails with

    current package believes it's in a workspace when it's not

# Why a gate

This is issue 0894 a second time. That one was `examples/workspaces/launch`,
whose root phase-383 W10.a deleted; the fix added its two leaves to the repo
root's `exclude`. 0894 checked the siblings and wrote down "bridge-cyclonedds:
root EXISTS / bridge-xrce: root EXISTS" — true when written. A later migration
deleted those roots too, and `bridge-{cyclonedds,xrce}/src/talker_pkg` stranded
exactly as `launch` had, breaking `just format` repo-wide again.

So the recurring shape is not "someone forgot an exclude". It is that DELETING
a workspace root strands leaves in a directory the deleter is not editing, and
nothing connects the two. A survey answers the question on the day it is run;
this gate answers it on every run.

# Scope

Tracked manifests only (`git ls-files`). An untracked one is build output —
`_deps/corrosion-src/test/**` alone contributes ~90 package manifests that no
cargo command in this repo ever resolves.

Run: python3 scripts/check-example-leaf-workspace-roots.py
     python3 scripts/check-example-leaf-workspace-roots.py --self-test
"""

import os
import re
import subprocess
import sys


def _has(path, pattern, _cache={}):
    """Does `path` contain a line matching `pattern`?"""
    if path not in _cache:
        try:
            with open(path, encoding="utf-8", errors="replace") as fh:
                _cache[path] = fh.read()
        except OSError:
            _cache[path] = ""
    return re.search(pattern, _cache[path], re.M) is not None


PACKAGE = r"^\[package\]"
WORKSPACE = r"^\[workspace\]"
# `workspace = "…"` / `workspace.path` — the leaf names its root explicitly.
WORKSPACE_KEY = r"^\s*workspace\s*="


def _root_list(root_txt, key):
    """The string entries of the repo root's `members` / `exclude` array."""
    m = re.search(key + r"\s*=\s*\[(.*?)^\]", root_txt, re.S | re.M)
    if not m:
        return set()
    return set(re.findall(r'"([^"]+)"', m.group(1)))


def owning_root(leaf_dir, exists=os.path.exists):
    """The manifest whose `[workspace]` table claims `leaf_dir`, or None.

    Mirrors cargo's walk-up: nearest ancestor manifest with a `[workspace]`
    table wins. Returns a path relative to the repo root, so the repo root
    itself is the literal "Cargo.toml".
    """
    d = os.path.dirname(leaf_dir)
    while True:
        cand = os.path.join(d, "Cargo.toml") if d else "Cargo.toml"
        if exists(cand) and _has(cand, WORKSPACE):
            return cand
        if not d:
            return None
        d = os.path.dirname(d)


def stranded_leaves(manifests, root_txt):
    members = _root_list(root_txt, "members")
    exclude = _root_list(root_txt, "exclude")
    out = []
    for manifest in manifests:
        if not _has(manifest, PACKAGE):
            continue
        if _has(manifest, WORKSPACE) or _has(manifest, WORKSPACE_KEY):
            continue
        leaf = os.path.dirname(manifest)
        if owning_root(leaf) != "Cargo.toml":
            continue
        if leaf in members or leaf in exclude:
            continue
        out.append(leaf)
    return sorted(out)


def self_test():
    """The parser, against the shapes that have actually appeared here."""
    txt = """
[workspace]
members = [
    "packages/core/nros-core",
]
exclude = [
    # a comment naming "a/quoted/path" that is NOT an entry
    ".claude",
    "examples/workspaces/launch/src/talker_pkg",
]
"""
    members = _root_list(txt, "members")
    exclude = _root_list(txt, "exclude")
    assert members == {"packages/core/nros-core"}, members
    assert "examples/workspaces/launch/src/talker_pkg" in exclude, exclude
    assert ".claude" in exclude, exclude
    # A comment inside the array contributes its quoted text — that can only
    # make the gate more permissive for that one path, never stricter, and
    # rewriting the arrays as TOML to avoid it would pull in a parser this
    # script deliberately does without.
    assert "a/quoted/path" in exclude

    # Missing array is empty, not a crash: a manifest may have neither.
    assert _root_list("[workspace]\n", "members") == set()

    # owning_root walks UP and stops at the nearest `[workspace]`.
    tree = {
        "examples/ws/Cargo.toml": "[workspace]\n",
        "Cargo.toml": "[workspace]\n",
    }
    _has.__defaults__[0].update(tree)
    assert owning_root("examples/ws/src/leaf", exists=lambda p: p in tree) == (
        "examples/ws/Cargo.toml"
    )
    assert owning_root("examples/other/leaf", exists=lambda p: p in tree) == "Cargo.toml"
    print("check-example-leaf-workspace-roots: self-test OK")


def main():
    if "--self-test" in sys.argv:
        self_test()
        return

    manifests = subprocess.run(
        ["git", "ls-files", "examples/Cargo.toml", "examples/**/Cargo.toml"],
        capture_output=True,
        text=True,
        check=True,
    ).stdout.split()

    with open("Cargo.toml", encoding="utf-8") as fh:
        root_txt = fh.read()

    bad = stranded_leaves(manifests, root_txt)
    if bad:
        sys.stderr.write(
            "check-example-leaf-workspace-roots: leaf(es) resolve against the REPO\n"
            "root, which names them in neither `members` nor `exclude`:\n\n"
        )
        for leaf in bad:
            sys.stderr.write(f"  {leaf}\n")
        sys.stderr.write(
            "\n  Every cargo command in these leaves fails with \"current package\n"
            "  believes it's in a workspace when it's not\" — including `just format`,\n"
            "  which is repo-wide, so this breaks the tree for everyone.\n\n"
            "  Usual cause: a workspace root under `examples/workspaces/<ws>/` was\n"
            "  deleted (an `nros build` migration puts the generated root under\n"
            "  `build/<coord>/`), stranding leaves in a directory the migration was\n"
            "  not editing. Issues 0894 and 0948 are both this.\n\n"
            "  Fix: add each leaf to the repo root `Cargo.toml`'s `exclude`, with the\n"
            "  reason recorded there — that is where every sibling standalone leaf is\n"
            "  already handled. An empty `[workspace]` table in the leaf works too,\n"
            "  but splits the answer across two places.\n"
        )
        sys.exit(1)

    print(
        f"check-example-leaf-workspace-roots: OK "
        f"({len(manifests)} tracked example manifest(s), none stranded)"
    )


if __name__ == "__main__":
    main()

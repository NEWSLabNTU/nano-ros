#!/usr/bin/env python3
"""Every example root is BUILT by some lane, or says why not — issue 1650.

An example no lane compiles can break with every gate green, and an example is
exactly what a user copies. This is issue 1521's unreported-lane class one
level up: 1521 was tests no merge-gating lane RAN; this is examples no lane
BUILDS.

THE CENSUS (phase-477 W2; re-run, never re-derive):
  * walk `git ls-files examples`;
  * a ROOT is `examples/<plat>/<lang>/<name>`, `examples/workspaces/<ws>` or
    `examples/templates/<t>` — keyed on SHAPE, never on a list of paths
    (issue 0196's rule), so a new example is in the census the moment it is
    tracked;
  * load the fixture manifest through `fixtures-manifest.py`'s own loaders
    (`load` / `load_workspace_fixtures` / `load_compile_check_fixtures`) — the
    manifest has ONE reader;
  * a root is COVERED when any row's `dir` equals it or lies under it.

An uncovered root needs a line in `.config/example-build-coverage-baseline.txt`
— `<root>  # <reason>` — RATCHETED both ways: an unlisted uncovered root
fails, and a listed root that has gained a row fails until its line goes.

What this does NOT claim: a row builds; whether it builds is the fixture
lane's verdict. This asks only whether anything is asked at all.

  check-example-build-coverage.py [--self-test] [--list]
"""

import importlib.util
import os
import subprocess
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
MANIFEST = os.path.join(ROOT, "examples", "fixtures.toml")
BASELINE = os.path.join(ROOT, ".config", "example-build-coverage-baseline.txt")


def manifest_module():
    spec = importlib.util.spec_from_file_location(
        "fixtures_manifest", os.path.join(ROOT, "scripts", "build", "fixtures-manifest.py"))
    mod = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(mod)
    return mod


def roots_of(paths):
    """Example roots, by shape, from a list of tracked paths."""
    out = set()
    for p in paths:
        parts = p.split("/")
        if len(parts) < 3 or parts[0] != "examples":
            continue
        if parts[1] in ("workspaces", "templates"):
            if len(parts) >= 4:  # a file INSIDE the root, not a README beside it
                out.add("/".join(parts[:3]))
        elif len(parts) >= 5:  # examples/<plat>/<lang>/<name>/<file>
            out.add("/".join(parts[:4]))
    return out


def covered(root, dirs):
    return any(d == root or d.startswith(root + "/") for d in dirs)


def row_dirs(mod):
    dirs = []
    for loader in (mod.load, mod.load_workspace_fixtures, mod.load_compile_check_fixtures):
        dirs += [r["dir"].rstrip("/") for r in loader(MANIFEST) if r.get("dir")]
    return dirs


def load_baseline():
    out = {}
    if os.path.exists(BASELINE):
        for line in open(BASELINE):
            line = line.rstrip("\n")
            if line.strip() and not line.lstrip().startswith("#"):
                root, _, reason = line.partition("#")
                out[root.strip()] = reason.strip()
    return out


def self_test():
    ok = True

    def expect(cond, what):
        nonlocal ok
        if not cond:
            ok = False
            print(f"check-example-build-coverage self-test FAILED: {what}", file=sys.stderr)

    paths = [
        "examples/native/rust/talker/Cargo.toml",
        "examples/workspaces/launch/src/talker_pkg/Cargo.toml",
        "examples/templates/zephyr-byo/west.yml",
        "examples/workspaces/README.md",       # beside the roots, not a root
        "examples/fixtures.toml",
        "examples/README.md",
    ]
    expect(roots_of(paths) == {"examples/native/rust/talker",
                               "examples/workspaces/launch",
                               "examples/templates/zephyr-byo"},
           f"roots by shape, got {sorted(roots_of(paths))}")
    expect(covered("examples/templates/cpp-port", ["examples/templates/cpp-port/zephyr"]),
           "a row UNDER a root covers it")
    expect(not covered("examples/templates/multi-node-workspace",
                       ["examples/templates/multi-node-workspace-cpp"]),
           "a sibling with a common PREFIX does not cover a root")
    return ok


def main():
    if not self_test():
        return 1
    print("check-example-build-coverage self-test: OK (3 cases)")
    if "--self-test" in sys.argv:
        return 0
    files = subprocess.run(["git", "-C", ROOT, "ls-files", "examples"],
                           capture_output=True, text=True, check=True).stdout.split()
    roots = sorted(roots_of(files))
    dirs = row_dirs(manifest_module())
    uncovered = [r for r in roots if not covered(r, dirs)]
    if "--list" in sys.argv:
        for r in uncovered:
            print(r)
        return 0
    base = load_baseline()
    new = [r for r in uncovered if r not in base]
    stale = [r for r in base if r not in uncovered]
    if new or stale:
        print("check-example-build-coverage: FAILED (issue 1650)", file=sys.stderr)
        for r in new:
            print(f"  {r}: no fixture row builds it", file=sys.stderr)
        if new:
            print("  Give it a row in examples/fixtures.toml (a build-only\n"
                  "  [[compile_check_fixture]] is enough), or record why it is not a build\n"
                  "  target in .config/example-build-coverage-baseline.txt.", file=sys.stderr)
        for r in stale:
            print(f"  {r}: baselined but now has a row (or is gone) — delete its line",
                  file=sys.stderr)
        return 1
    print(f"check-example-build-coverage: OK — {len(roots) - len(uncovered)} of "
          f"{len(roots)} example root(s) built by a row; {len(uncovered)} with a recorded reason")
    return 0


if __name__ == "__main__":
    sys.exit(main())

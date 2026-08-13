#!/usr/bin/env python3
"""issue 0560 — a lock whose dep versions are decided by a SUBMODULE must resolve.

## The failure this catches

`packages/cli/nros-launch-resolve` path-deps into the `play_launch` submodule,
whose `ros-launch-resolve/Cargo.toml` git-deps `ros-launch-manifest` by TAG. So
that leaf's lock is pinned by a manifest living outside its own tree: advance the
submodule pointer and the lock is stale, with nothing relating the two halves.

That happened. The submodule moved to rlm v0.1.6 while the lock still pinned
v0.1.4, and `--locked` (injected project-wide by `scripts/bin/cargo`) made the
leaf unbuildable:

    error: cannot update the lock file … because --locked was passed

It survived on main because the only consumer, `just setup-launch-resolve`, is a
dependency of `build-test-fixtures` and nothing else — so the break waited for
whoever next ran the ~40-minute fixture lane, rather than failing its author.

## Why `cargo metadata`, not a build

Resolution is what broke, so resolution is what to check. `cargo metadata
--locked --offline` reproduces the failure in seconds without compiling
anything, and `--offline` keeps this gate off the network: a correct lock needs
no fetch, and an incorrect one fails on the lock rather than on connectivity.

Both directions were verified against the real pre-fix lock (`567101c43~1`)
before this gate was written: rc=101 on the broken lock, rc=0 on the fixed one,
offline in both cases.

## The leaf set is DERIVED, not listed

A hardcoded path would go stale the first time another leaf grew a submodule
dep — the exact class of drift this repo keeps paying for. A leaf qualifies when
it has a tracked `Cargo.lock` AND its manifest carries a `path = …` dependency
resolving inside a path registered in `.gitmodules`. Today that is one leaf; the
rule is what matters.
"""
import configparser
import re
import subprocess
import sys
from pathlib import Path

REPO = Path(__file__).resolve().parents[1]
PATH_DEP = re.compile(r'path\s*=\s*"([^"]+)"')


def submodule_paths():
    """Every registered submodule path, from `.gitmodules`."""
    gm = REPO / ".gitmodules"
    if not gm.is_file():
        return []
    cp = configparser.ConfigParser()
    # .gitmodules section headers are `[submodule "name"]`, which configparser
    # handles; values are indented, which it also handles.
    cp.read_string(gm.read_text())
    out = []
    for section in cp.sections():
        p = cp[section].get("path")
        if p:
            out.append((REPO / p).resolve())
    return out


def exposed_leaves():
    """Leaves whose lock is pinned by a manifest outside their own tree."""
    subs = submodule_paths()
    if not subs:
        return []
    locks = subprocess.run(
        ["git", "ls-files", "Cargo.lock", "*/Cargo.lock"],
        cwd=REPO, capture_output=True, text=True, check=True,
    ).stdout.split()

    found = []
    for lock in locks:
        leaf = (REPO / lock).parent
        manifest = leaf / "Cargo.toml"
        if not manifest.is_file():
            continue
        for rel in PATH_DEP.findall(manifest.read_text()):
            target = (leaf / rel).resolve()
            if any(target == s or s in target.parents for s in subs):
                found.append((leaf, target))
                break
    return found


def main():
    leaves = exposed_leaves()
    if not leaves:
        print("submodule-pinned locks: none (no leaf path-deps into a submodule)")
        return 0

    failures = []
    checked = 0
    for leaf, target in leaves:
        rel = leaf.relative_to(REPO)
        # A leaf whose submodule is not initialised cannot be checked, and must
        # not fail: `just setup-launch-resolve` self-gates on the same condition
        # and says so. Silence here would be wrong too, hence the note.
        if not target.exists():
            print(f"  SKIP {rel} — submodule not initialised at {target.relative_to(REPO)}")
            continue
        checked += 1
        proc = subprocess.run(
            ["cargo", "metadata", "--locked", "--offline", "--format-version", "1"],
            cwd=leaf, capture_output=True, text=True,
        )
        if proc.returncode != 0:
            failures.append((rel, proc.stderr.strip().splitlines()))
        else:
            print(f"  ok   {rel} resolves under --locked")

    if failures:
        print("", file=sys.stderr)
        print(
            f"[FAIL] {len(failures)} lock(s) pinned by a submodule manifest no longer "
            f"resolve:", file=sys.stderr,
        )
        for rel, err in failures:
            print(f"\n  {rel}", file=sys.stderr)
            for line in err[-4:]:
                print(f"      {line}", file=sys.stderr)
        print(
            "\n  The submodule pointer moved and the lock did not follow (issue 0560).\n"
            "  Update it the sanctioned way — never a bare `cargo generate-lockfile`:\n"
            "      just lock-update \"\" \"\" <leaf-dir>\n"
            "  then REVIEW the diff: added/removed packages are a dependency change,\n"
            "  which is expected when a pinned tag moves, but should be seen.",
            file=sys.stderr,
        )
        return 1

    print(f"submodule-pinned locks: OK ({checked} leaf/leaves resolve under --locked)")
    return 0


if __name__ == "__main__":
    sys.exit(main())

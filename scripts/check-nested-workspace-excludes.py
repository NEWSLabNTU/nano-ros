#!/usr/bin/env python3
"""Every package under a root-less workspace is excluded by the repo root.

# What breaks without it

A package that is not a member of the repo-root workspace, has no `[workspace]`
table of its own and no nested workspace root above it is found by cargo's
walk-up at the REPO ROOT — and unless the root `exclude`s it, any cargo
invocation inside it dies with

    error: current package believes it's in a workspace when it's not

Nothing near the leaf says the repo root has to know about it, and the root
itself resolves fine, so the break shows up only when someone runs cargo (or
west) from inside the leaf — in CI, the embedded lane, a day of latency away.
phase-331's renames left five stale root excludes and two unprotected leaves;
issues 0894 and 0948 hit it one deleted nested root at a time.

# What changed, and why this gate was rewritten (phase-472 W4)

The first version read the `exclude` arrays of TRACKED nested workspace roots
(`examples/workspaces/*/Cargo.toml`) and asked the repo root to mirror them.
RFC-0098 D9 (phase-445 W5) deleted those roots — a workspace has no root build
file now; the ones that exist on disk are generated and untracked — so it
examined ZERO leaves and printed "every excluded package is excluded at the
repo root too" over a tree where deleting the root's `examples/workspaces`
exclude stranded 24 packages.

It also matched an exclude by exact string, where cargo matches by path
PREFIX: fed the real tree, the old test would have called all 24 packages
unprotected while the root's one-line prefix covered them.

# The rule now

Population: every tracked package manifest under `examples/workspaces/` and
`examples/templates/` (the two root-less-workspace trees D9 names). For each:

* its own `[workspace]` table → its own root, skip (counted as examined);
* a TRACKED `[workspace]` root between it and the repo root → that root owns
  it, skip;
* otherwise the repo root must `exclude` it — exact path or a path-component
  PREFIX, which is what cargo implements.

The other direction is kept: a repo-root exclude under `examples/workspaces/`
or `examples/templates/` naming a path that does not exist is how this broke
the first time.

The count examined is printed and zero FAILS (`scripts/lib/population.py`).

Buildless, pure text; fast tier.
"""

from __future__ import annotations

import io
import sys
from pathlib import Path

try:
    import tomllib
except ModuleNotFoundError:  # Python < 3.11
    import tomli as tomllib  # type: ignore

ROOT = Path(__file__).resolve().parent.parent
sys.path.insert(0, str(ROOT / "scripts" / "lib"))
from population import require_population  # noqa: E402
from tracked import tracked  # noqa: E402  issue 0721: the index, not a walk

GATE = "check-nested-workspace-excludes"
SCOPES = ("examples/workspaces", "examples/templates")


def _load(path: Path) -> dict:
    with path.open("rb") as fh:
        return tomllib.load(fh)


def excluded(rel: str, excludes: list[str]) -> bool:
    """cargo's rule: an exclude covers a path equal to it or BELOW it."""
    parts = Path(rel).parts
    for e in excludes:
        ep = Path(e.rstrip("/")).parts
        if parts[: len(ep)] == ep:
            return True
    return False


def check(repo: Path, manifests: list[str], root_doc: dict, workspace_roots: set[str]):
    """(failures, examined). `manifests` are repo-relative package Cargo.toml paths."""
    ws = root_doc.get("workspace", {})
    excludes = [e for e in ws.get("exclude", []) if isinstance(e, str)]
    members = [m for m in ws.get("members", []) if isinstance(m, str)]
    failures = []
    examined = 0
    for m in manifests:
        doc = _load(repo / m)
        if "package" not in doc:
            continue
        examined += 1
        leaf = str(Path(m).parent)
        if "workspace" in doc:
            continue
        # A tracked nested root between the leaf and the repo root owns it.
        owner = next(
            (str(p) for p in Path(leaf).parents
             if str(p) not in (".", "") and str(p) in workspace_roots),
            None,
        )
        if owner:
            continue
        if leaf in members or excluded(leaf, excludes):
            continue
        failures.append(
            f"[FAIL] {leaf}\n"
            f"       no [workspace] of its own, no tracked workspace root above it,\n"
            f"       and the repo-root Cargo.toml neither lists nor excludes it.\n"
            f"       cargo run from inside it will fail: \"current package believes\n"
            f"       it's in a workspace when it's not\"."
        )
    for e in sorted(set(excludes)):
        if not any(e == s or e.startswith(s + "/") for s in SCOPES):
            continue
        if not (repo / e).exists():
            failures.append(
                f"[FAIL] repo-root Cargo.toml excludes '{e}', which does not exist.\n"
                f"       A rename left it behind; the leaf it used to name is either gone\n"
                f"       or now unprotected under its new path."
            )
    return failures, examined


def self_test() -> int:
    import tempfile

    bad = 0
    with tempfile.TemporaryDirectory() as d:
        repo = Path(d)

        def put(rel, text):
            p = repo / rel
            p.parent.mkdir(parents=True, exist_ok=True)
            p.write_text(text)
            return rel

        leaf = put("examples/workspaces/w/src/a_pkg/Cargo.toml", '[package]\nname = "a"\n')
        own = put("examples/workspaces/w/src/own/Cargo.toml", '[package]\nname = "o"\n[workspace]\n')
        put("examples/workspaces/w/src/a/README", "a real sibling dir, so its exclude is not stale\n")
        cases = [
            ("covered by a PREFIX exclude (cargo's rule)", {"exclude": ["examples/workspaces"]}, [leaf], 0),
            ("covered by an exact exclude", {"exclude": ["examples/workspaces/w/src/a_pkg"]}, [leaf], 0),
            ("the P2: the prefix exclude deleted", {"exclude": []}, [leaf], 1),
            ("a sibling-name prefix is not a path prefix",
             {"exclude": ["examples/workspaces/w/src/a"]}, [leaf], 1),
            ("its own [workspace] is its own root", {"exclude": []}, [own], 0),
            ("a stale exclude under the scope", {"exclude": ["examples/workspaces", "examples/workspaces/gone"]}, [leaf], 1),
        ]
        for name, ws, manifests, want in cases:
            fails, _ = check(repo, manifests, {"workspace": ws}, set())
            if len(fails) != want:
                print(f"  SELF-TEST FAIL: {name}: expected {want}, got {len(fails)}")
                bad += 1
        # A tracked nested root above the leaf owns it.
        fails, _ = check(repo, [leaf], {"workspace": {"exclude": []}}, {"examples/workspaces/w"})
        if fails:
            print("  SELF-TEST FAIL: a tracked nested root above the leaf should own it")
            bad += 1
        # The population half: no packages examined must not read as OK.
        _, n = check(repo, [], {"workspace": {"exclude": ["examples/workspaces"]}}, set())
        if require_population(n, "package(s)", gate="probe", out=io.StringIO(), err=io.StringIO()):
            print("  SELF-TEST FAIL: zero packages examined read as a pass")
            bad += 1
    if bad:
        print(f"{GATE}: {bad} self-test(s) failed — its verdict cannot be trusted")
    return 1 if bad else 0


def main() -> int:
    if self_test():
        return 1
    if "--self-test" in sys.argv:
        print(f"{GATE}: self-test OK")
        return 0
    manifests = [
        str(p.relative_to(ROOT))
        for p in tracked(*SCOPES, name="Cargo.toml")
    ]
    workspace_roots = set()
    for p in tracked("examples", name="Cargo.toml"):
        if "workspace" in _load(p):
            workspace_roots.add(str(p.parent.relative_to(ROOT)))
    failures, examined = check(ROOT, manifests, _load(ROOT / "Cargo.toml"), workspace_roots)
    if not require_population(examined, f"package(s) under {' / '.join(SCOPES)}", gate=GATE):
        return 1
    if failures:
        print("\n".join(failures), file=sys.stderr)
        return 1
    print(f"{GATE}: OK — all {examined} package(s) are excluded (or owned) at the repo root.")
    return 0


if __name__ == "__main__":
    sys.exit(main())

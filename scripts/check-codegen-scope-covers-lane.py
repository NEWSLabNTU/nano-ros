#!/usr/bin/env python3
"""A lane's narrowed codegen covers everything that lane's fixture build reads.

phase-535 W10 (issue 2475 defect 4). `_codegen <lane>` generates only the
codegen units `scripts/lib/codegen_scope.py` selects for the lane. This checks
that selection against an INDEPENDENT enumeration of what the build reaches,
so the two cannot agree by sharing a bug:

  1. Every row the build enumerates — through the commands the build itself
     runs (`list`, `list-workspaces`, `west-leaves` with `--coords-from`, and
     `list-compile-checks` unscoped, since `build-compile-check-fixtures` runs
     for every lane) — has every codegen unit nested with its dir in scope.
  2. No `path =` dep from a manifest inside an in-scope unit lands in the
     OUTPUT of a skipped unit (`<unit>/generated`, the only tree this codegen
     writes). This is the cross-unit case nesting cannot see: a leaf that
     path-deps another unit's crates. A `generated/` that is no unit's output
     (the committed `packages/interfaces/generated`, a test bin's build-time
     `generated/nros-derived`) has another producer and is not this check's.
     `generated/px4_msgs` is exempt for the reason `leaf-config-includes.py`
     gives: its producer is `just px4 build-fixtures`, not this codegen.

MEMBERSHIP, never counts. `--self-test` drops one in-scope unit from a real
selection and requires a failure (the mutation this exists to catch).

Usage:
    check-codegen-scope-covers-lane.py <coords file>
    check-codegen-scope-covers-lane.py --self-test <coords file>
"""
from __future__ import annotations

import re
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
sys.path.insert(0, str(ROOT / "scripts" / "lib"))
import codegen_scope  # noqa: E402

SEP = "\x1f"
MANIFEST = "scripts/build/fixtures-manifest.py"
GENERATED_PATH_RE = re.compile(r'path\s*=\s*"([^"]*\bgenerated/[^"]*)"')
PX4_PRODUCED_RE = re.compile(r"\bgenerated/px4_msgs\b")


def _fm(*args: str) -> list[list[str]]:
    out = subprocess.run(
        [sys.executable, MANIFEST, *args], cwd=ROOT, capture_output=True,
        text=True, stdin=subprocess.DEVNULL,
    )
    if out.returncode != 0:
        raise SystemExit(f"fixtures-manifest.py {' '.join(args)} failed:\n{out.stderr}")
    return [line.split(SEP) for line in out.stdout.splitlines() if line.strip()]


def built_dirs(coords: str) -> set[str]:
    """Row dirs the lane's fixture build reaches, by the build's own commands."""
    dirs = set()
    dirs |= {r[0] for r in _fm("list", "--coords-from", coords)}
    dirs |= {r[2] for r in _fm("list-workspaces", "--coords-from", coords)}
    dirs |= {r[5] for r in _fm("west-leaves", "--coords-from", coords) if len(r) > 5}
    dirs |= {r[2] for r in _fm("list-compile-checks") if len(r) > 2}
    return {d.rstrip("/") for d in dirs if d}


def manifests() -> list[str]:
    out = subprocess.run(
        ["git", "ls-files", "*Cargo.toml"], cwd=ROOT, capture_output=True,
        text=True, check=True,
    ).stdout
    return out.split()


def _inside(path: str, units: list[str]) -> bool:
    return any(path == u or path.startswith(u + "/") for u in units)


def gaps(coords: str, scoped: list[str]) -> list[str]:
    everything = [u for _, u in codegen_scope.all_units()]
    out = []
    for d in sorted(built_dirs(coords)):
        for u in everything:
            if codegen_scope._nested(u, d) and u not in scoped:
                out.append(f"row dir {d} needs codegen unit {u}, which the lane skips")
    for rel in manifests():
        if not _inside(rel, scoped):
            continue
        man = ROOT / rel
        for dep in GENERATED_PATH_RE.findall(man.read_text()):
            if PX4_PRODUCED_RE.search(dep):
                continue
            target = (man.parent / dep).resolve()
            try:
                tgt = target.relative_to(ROOT).as_posix()
            except ValueError:
                continue  # outside the checkout: not this codegen's output
            owner = next((u for u in everything
                          if tgt == f"{u}/generated" or tgt.startswith(f"{u}/generated/")),
                         None)
            if owner is not None and owner not in scoped:
                out.append(f"{rel} path-deps {dep}, the output of codegen unit "
                           f"{owner}, which the lane skips")
    return out


def main(argv: list[str]) -> int:
    self_test = argv[1:2] == ["--self-test"]
    rest = argv[2:] if self_test else argv[1:]
    if len(rest) != 1:
        print("usage: check-codegen-scope-covers-lane.py [--self-test] <coords file>",
              file=sys.stderr)
        return 2
    coords = rest[0]
    units, narrowed = codegen_scope.scoped_units(coords)
    scoped = [u for _, u in units]
    if self_test:
        # Drop each in-scope unit that some built row needs; every such
        # mutation must be reported. A gate that cannot fail checks nothing.
        needed = sorted({u for u in scoped for d in built_dirs(coords)
                         if codegen_scope._nested(u, d)})
        if not needed:
            print(f"self-test: {coords} selects no unit a row needs", file=sys.stderr)
            return 1
        victim = needed[len(needed) // 2]
        if not gaps(coords, [u for u in scoped if u != victim]):
            print(f"self-test FAILED: dropping {victim} went unreported", file=sys.stderr)
            return 1
        print(f"codegen-scope coverage self-test OK (dropping {victim} is reported)")
        return 0
    bad = gaps(coords, scoped)
    for line in bad:
        print(f"FAIL {line}", file=sys.stderr)
    if bad:
        print("  The lane's codegen scope is narrower than its fixture build. "
              "The selection lives in scripts/lib/codegen_scope.py.", file=sys.stderr)
        return 1
    print(f"codegen scope covers the lane: {len(scoped)} unit(s)"
          + ("" if narrowed else " (not narrowed)"))
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))

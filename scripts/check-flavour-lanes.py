#!/usr/bin/env python3
"""phase-359 W9 — every matrix platform resolves to exactly ONE std flavour.

## Why this exists

`std` and `no_std` are not two spellings of one target set here, and the split
does not follow "host vs embedded" in either direction:

  * `ThreadxLinux` is a `no_std` board running as an ordinary Linux process.
  * `NuttX` is `std` — on an RTOS — via `nros-board-nuttx`, which compiles the
    standard library from source with `build-std`.

So a lane cannot infer flavour from the platform's name or its host-ness, and a
test group that spans both flavours is testing two different products in one
run. That is what phase-359 W9 separates.

## Why DERIVED, not listed

A hand-kept flavour table is the failure mode this repo has already paid for
twice: `lane-filter.sh`'s own header records that a hand-written exclusion list
"rots the moment a platform is added, and the lane then silently skips it"
(issue 0341), and issue 0577 was seven tests that no lane ran at all.

So nothing here is asserted. The flavour of a platform is READ from the board
crates that serve it: a board is `std` iff its manifest enables the `"std"`
feature on its `nros` / `nros-platform` dependencies, which is exactly what
makes the standard library reachable in the image it links. The registry
(`packages/boards/board-support.toml`) supplies the board -> platform relation
it already maintains for `check-board-tiers`.

## What is enforced

1. **No platform is served by boards of BOTH flavours.** That is the "one group,
   one flavour" rule: if it ever became true, a lane keyed on the platform would
   silently mix std and no_std images.
2. **Every board row that names a `matrix_platform` resolves.** A row pointing at
   a crate whose manifest cannot be read is a gap, not a pass.

`--print` emits the derived table for `lane-filter.sh` to consume, so the lane
split and this gate cannot disagree — there is one derivation, used twice.
"""
import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
REGISTRY = ROOT / "packages/boards/board-support.toml"
BOARDS_DIR = ROOT / "packages/boards"

# A dependency line enabling the `std` feature. The board's own `[features]`
# table is deliberately NOT consulted: several boards declare a `std` feature
# they never enable, and what decides the image is which features the board
# turns on in its DEPENDENCIES.
STD_DEP_RE = re.compile(r'^\s*(nros|nros-platform|nros-log|nros-core)\s*=.*features\s*=\s*\[[^\]]*"std"')
# Boards depend on other boards — `nros-board-nuttx-qemu` on `nros-board-nuttx`,
# and the threadx family similarly — so a board's flavour is NOT decided by its
# own manifest alone. NuttX is the case that proves it: the qemu board enables
# no `std` itself, but the base board it links does, and NuttX genuinely
# compiles the standard library from source via `build-std`. Reading only the
# own-manifest reported NuttX as `no_std`, which is wrong.
BOARD_DEP_RE = re.compile(r'^\s*(nros-board-[a-z0-9-]+)\s*=')


def parse_registry(text):
    """Minimal `[[board]]` reader — mirrors scripts/check-board-tiers.py."""
    entries, cur = [], None
    for raw in text.splitlines():
        line = raw.strip()
        if line.startswith("#") or not line:
            continue
        if line == "[[board]]":
            cur = {}
            entries.append(cur)
            continue
        if cur is None:
            continue
        m = re.match(r'^([a-z_]+)\s*=\s*(.+)$', line)
        if not m:
            continue
        key, val = m.group(1), m.group(2).strip()
        if val.startswith("["):
            cur[key] = re.findall(r'"([^"]*)"', val)
        elif val.startswith('"'):
            cur[key] = val.strip('"')
        else:
            cur[key] = val.rstrip(",")
    return entries


def _read_manifest(crate):
    m = BOARDS_DIR / crate / "Cargo.toml"
    return m.read_text(errors="replace") if m.is_file() else None


def board_flavour(crate, _seen=None):
    """`std` / `no_std` / None (manifest unreadable), following board deps.

    Cycle-safe: `nros-board-nuttx` and `nros-board-nuttx-qemu` depend on each
    other (feature-gated in both directions), as do the threadx pair, so a naive
    walk loops.

    Over-approximates toward `std`: a board that MIGHT link a std board counts
    as std. That is the safe direction — it keeps a doubtful board OUT of the
    no_std lane rather than silently admitting a std image into it.
    """
    seen = _seen if _seen is not None else set()
    if crate in seen:
        return "no_std"  # cycle: contributes nothing on its own
    seen.add(crate)
    text = _read_manifest(crate)
    if text is None:
        return None
    deps = []
    for line in text.splitlines():
        if line.strip().startswith("#"):
            continue
        if STD_DEP_RE.match(line):
            return "std"
        m = BOARD_DEP_RE.match(line)
        if m and m.group(1) != crate:
            deps.append(m.group(1))
    for d in deps:
        if board_flavour(d, seen) == "std":
            return "std"
    return "no_std"


def derive():
    """platform -> {flavour: [crates]}, plus rows whose crate is unreadable."""
    rows = parse_registry(REGISTRY.read_text())
    by_platform, unreadable = {}, []
    for r in rows:
        platform = r.get("matrix_platform")
        crate = r.get("crate")
        if not platform or not crate:
            continue  # `infra` rows carry no platform, by design
        fl = board_flavour(crate)
        if fl is None:
            unreadable.append((crate, platform))
            continue
        by_platform.setdefault(platform, {}).setdefault(fl, []).append(crate)
    return by_platform, unreadable


def main():
    by_platform, unreadable = derive()

    if "--print" in sys.argv:
        for platform in sorted(by_platform):
            flavours = by_platform[platform]
            # An ambiguous platform prints nothing: the gate below fails, and a
            # consumer must not silently pick one side.
            if len(flavours) == 1:
                print(f"{platform}\t{next(iter(flavours))}")
        return 0

    width = max([len(p) for p in by_platform] + [len("platform")])
    print(f"{'platform':<{width}}  flavour   boards")
    for platform in sorted(by_platform):
        flavours = by_platform[platform]
        fl = "/".join(sorted(flavours)) if len(flavours) > 1 else next(iter(flavours))
        crates = ", ".join(sorted(c for cs in flavours.values() for c in cs))
        print(f"{platform:<{width}}  {fl:<8}  {crates}")

    rc = 0
    mixed = {p: f for p, f in by_platform.items() if len(f) > 1}
    if mixed:
        print("\n[FAIL] platform(s) served by boards of BOTH flavours:", file=sys.stderr)
        for p, f in sorted(mixed.items()):
            for fl, crates in sorted(f.items()):
                print(f"    {p}: {fl} <- {', '.join(sorted(crates))}", file=sys.stderr)
        print(
            "\n  A lane keyed on the platform would mix std and no_std images in one\n"
            "  run. Split the platform, or move the odd board to its own.",
            file=sys.stderr,
        )
        rc = 1
    if unreadable:
        print(f"\n[FAIL] {len(unreadable)} registry row(s) name an unreadable crate:", file=sys.stderr)
        for crate, platform in unreadable:
            print(f"    {crate} (platform {platform}) — no packages/boards/{crate}/Cargo.toml", file=sys.stderr)
        rc = 1
    if rc == 0:
        print(f"\nflavour lanes: OK ({len(by_platform)} platform(s), each one flavour)")
    return rc


if __name__ == "__main__":
    sys.exit(main())

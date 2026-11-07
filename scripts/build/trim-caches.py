#!/usr/bin/env python3
"""Keep a worktree's multi-target build caches to a budget: least recently used first.

phase-543 W4. `build/corrosion-cargo` and `build/sizes-probe` each hold many
independent cargo target directories (one per image key or probe key), and
nothing pruned them in a developer's worktree. Over five `just ci gate` runs in
one worktree they grew 51 -> 160 G and 38 -> 93 G, and filled a 1.8 T disk
twice in one day. The self-hosted runner has its own evictor
(`scripts/ci/runner-sweep.sh`, `_evict`), and this does not run there: two
evictors with two budgets over the same directories would fight.

## What is evicted, and what is kept

The UNIT is one cargo target directory, found by the `CACHEDIR.TAG` cargo
writes at its root. Where cargo writes the tag inside a target-triple directory
(`sizes-probe/<rustc>/<key>/x86_64-unknown-linux-gnu/`), the unit is the parent:
the triple names come from `rustc --print target-list`, never from a pattern,
because a key directory such as `nros-c-image_9d89b` looks like a triple to
any regex. A whole unit goes or stays. Half a cargo target dir is a build that
cannot trust its own fingerprints.

"Recently used" is the newest access or modification time among the unit's
`.fingerprint` entries. On a `relatime` mount access times move at most once a
day, so this orders units by day, and a unit used in the last `--keep-hours`
(default 12) is never evicted. A gate run in progress keeps its cache whatever
the budget says.

Not reachability (`gc-shared-cargo.py`): `template-copy-out` deletes its copies
after each run, so the stores those copies reuse read as unreachable between
runs, and pruning them would make every run start cold. Least recently used
keeps what runs actually read.

Budgets are `.config/cache-budgets.toml`. `NROS_KEEP_CACHES=1` makes this a
report only. Report is also the default without `--apply`.
"""

from __future__ import annotations

import argparse
import os
import shutil
import subprocess
import sys
import tempfile
import time
from pathlib import Path

try:
    import tomllib
except ModuleNotFoundError:
    import tomli as tomllib

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / "scripts" / "lib"))
import build_root  # noqa: E402

BUDGETS = ROOT / ".config" / "cache-budgets.toml"
GIB = 1 << 30


def target_triples() -> set[str]:
    try:
        out = subprocess.run(["rustc", "--print", "target-list"], capture_output=True,
                             text=True, check=True, cwd=ROOT).stdout
    except (OSError, subprocess.CalledProcessError):
        return set()
    return {t.strip() for t in out.splitlines() if t.strip()}


def units_in(area: Path, triples: set[str]) -> list[Path]:
    """The cargo target dirs under `area`, outermost only."""
    found: set[Path] = set()
    for dirpath, dirnames, filenames in os.walk(area):
        if "CACHEDIR.TAG" in filenames:
            d = Path(dirpath)
            found.add(d.parent if d.name in triples else d)
        # Never descend into a cargo profile dir (one holding `.fingerprint`):
        # nothing below is a unit, and listing those directories would refresh
        # their access times, which is the evidence `last_used` reads.
        if ".fingerprint" in dirnames:
            dirnames[:] = []
    # Outermost: drop a unit nested inside another.
    return sorted(u for u in found if not any(o != u and o in u.parents for o in found))


def last_used(unit: Path) -> float:
    """The newest access/modification time of a FILE in the unit's fingerprints.

    cargo reads those files on every build, fresh or not, so a FILE's atime is
    the use record. A directory's is not: listing one (this scan included)
    refreshes it. Files are only stat'ed here, which changes nothing."""
    best = 0.0
    for dirpath, dirnames, _ in os.walk(unit):
        if ".fingerprint" not in dirnames:
            continue
        fp = Path(dirpath) / ".fingerprint"
        try:
            for entry in os.scandir(fp):
                if not entry.is_dir(follow_symlinks=False):
                    continue
                for f in os.scandir(entry.path):
                    st = f.stat(follow_symlinks=False)
                    best = max(best, st.st_atime, st.st_mtime)
        except OSError:
            pass
        dirnames[:] = [d for d in dirnames if d != ".fingerprint"]
    if best == 0.0:
        st = unit.stat()
        best = max(st.st_atime, st.st_mtime)
    return best


def size_of(path: Path) -> int:
    total = 0
    for dirpath, _, filenames in os.walk(path):
        for f in filenames:
            try:
                total += os.lstat(os.path.join(dirpath, f)).st_blocks * 512
            except OSError:
                pass
    return total


def plan(units: list[tuple[Path, int, float]], budget: int, keep_after: float) -> list[Path]:
    """Oldest first, until the area fits; never a unit used after `keep_after`."""
    total = sum(s for _, s, _ in units)
    out = []
    for path, size, used in sorted(units, key=lambda u: u[2]):
        if total <= budget:
            break
        if used >= keep_after:
            continue
        out.append(path)
        total -= size
    return out


def trim(apply: bool, keep_hours: float, budgets_file: Path = BUDGETS) -> int:
    cfg = tomllib.loads(budgets_file.read_text())
    triples = target_triples()
    if not triples:
        print("trim-caches: `rustc --print target-list` failed; refusing to guess unit boundaries",
              file=sys.stderr)
        return 1
    keep_after = time.time() - keep_hours * 3600
    freed = 0
    for kind, spec in cfg.get("area", {}).items():
        area = build_root.build_dir(ROOT, kind)
        if not area.is_dir():
            continue
        budget = int(float(spec["budget_gib"]) * GIB)
        units = [(u, size_of(u), last_used(u)) for u in units_in(area, triples)]
        total = sum(s for _, s, _ in units)
        drop = plan(units, budget, keep_after)
        gone = sum(s for p, s, _ in units if p in drop)
        after = total - gone
        if drop:
            verdict = (f"{'evicting' if apply else 'would evict'} {len(drop)} "
                       f"({gone / GIB:.1f} G), least recently used first")
        elif total <= budget:
            verdict = "within budget"
        else:
            verdict = "nothing evictable"
        if after > budget:
            # Everything left was used inside the keep window: a run in
            # progress keeps its cache. Say so rather than call it in budget.
            verdict += (f"; still {after / GIB:.1f} G, over budget by what was used in "
                        f"the last {keep_hours:g} h (kept)")
        print(f"trim-caches: {kind}: {len(units)} target dir(s), {total / GIB:.1f} G "
              f"/ {budget / GIB:.0f} G budget -> {verdict}")
        if apply:
            for p in drop:
                shutil.rmtree(p, ignore_errors=True)
            freed += gone
    if apply and freed:
        print(f"trim-caches: freed {freed / GIB:.1f} G")
    return 0


def self_test() -> None:
    with tempfile.TemporaryDirectory(prefix="nros-trim-test-") as t:
        t = Path(t)
        now = time.time()
        # Three target dirs: one tagged inside a triple dir, one nested inside a
        # unit (must not count as its own), one plain.
        for name, age_h in (("old", 48), ("mid", 24), ("new", 1)):
            u = t / "area" / name
            (u / "x86_64-unknown-linux-gnu").mkdir(parents=True)
            (u / "x86_64-unknown-linux-gnu" / "CACHEDIR.TAG").write_text("")
            fp = u / "debug" / ".fingerprint" / "c-0123456789abcdef"
            fp.mkdir(parents=True)
            (u / "debug" / "blob").write_bytes(b"x" * 8192)
            ts = now - age_h * 3600
            (fp / "lib-c").write_text("0")
            os.utime(fp / "lib-c", (ts, ts))
        (t / "area" / "old" / "nested").mkdir()
        (t / "area" / "old" / "nested" / "CACHEDIR.TAG").write_text("")
        triples = {"x86_64-unknown-linux-gnu"}
        units = units_in(t / "area", triples)
        assert [u.name for u in units] == ["mid", "new", "old"], units
        # Discovery must not refresh the evidence: walking the area twice
        # leaves the oldest unit as old as it was.
        units_in(t / "area", triples)
        assert now - last_used(t / "area" / "old") > 47 * 3600, "the scan refreshed atimes"
        rows = [(u, 10, last_used(u)) for u in units]
        # Budget 15 of 30: oldest goes first, then the next; `new` is protected.
        drop = plan(rows, 15, now - 12 * 3600)
        assert [p.name for p in drop] == ["old", "mid"], drop
        # Within budget: nothing goes.
        assert plan(rows, 30, now - 12 * 3600) == []
        # Everything recent: nothing goes, whatever the budget.
        assert plan(rows, 0, now - 72 * 3600) == []
        # A key dir named like a triple to a regex is still a unit.
        k = t / "keys" / "nros-c-image_9d89b"
        k.mkdir(parents=True)
        (k / "CACHEDIR.TAG").write_text("")
        assert [u.name for u in units_in(t / "keys", triples)] == ["nros-c-image_9d89b"]


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__.split("\n")[0])
    ap.add_argument("--apply", action="store_true", help="delete (default: report)")
    ap.add_argument("--keep-hours", type=float, default=12.0)
    args = ap.parse_args()
    self_test()
    apply = args.apply and os.environ.get("NROS_KEEP_CACHES") != "1"
    if args.apply and not apply:
        print("trim-caches: NROS_KEEP_CACHES=1 — reporting only")
    return trim(apply, args.keep_hours)


if __name__ == "__main__":
    sys.exit(main())

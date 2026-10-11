#!/usr/bin/env python3
"""check-cargo-registry-pins — one Rust registry pin set, and the roots obey it.

RFC-0104 D2 / phase-485 W1. `nros-sdk-index.toml` names the pin set
(`[rust] registry_pins`) and the closed-graph roots that keep a tracked
`Cargo.lock` (`[rust] locked_roots`). This gate holds three things:

  * the pin set holds ONE version per semver-compatible range (two cannot
    coexist in one resolve, so a second one could only ever be dead weight
    or a disagreement);
  * every locked root's registry entries are IN the pin set at that version,
    so `heapless` cannot be 0.8.1 in the repo root and 0.8.0 in the examples;
  * the pin set is what `scripts/gen-cargo-registry-pins.py` writes from the
    tracked locks today, so a moved root lock is a stale pin set, not a
    silent divergence;
  * a `Cargo.lock` is tracked IFF its directory is a locked root (phase-485
    W2) -- every other cargo root is a leaf, seeded from the pins on its first
    build by `nros build` / `nros ws seed-lock`, never committed.

Remedy for every failure: move the root's lock with `just lock-update`, then
`python3 scripts/gen-cargo-registry-pins.py`, and review both diffs together.

Buildless, ~0.2 s. Self-tests its two set checks against planted locks on
every run, so a check that can no longer fail fails here.
"""

import os
import subprocess
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
sys.path.insert(0, os.path.join(ROOT, "scripts", "lib"))
import cargo_pins as cp  # noqa: E402

REG = "registry+https://github.com/rust-lang/crates.io-index"


def _pkg(name, version):
    return {"name": name, "version": version, "source": REG}


def self_test():
    pins = {"package": [_pkg("heapless", "0.8.0"), _pkg("serde", "1.0.200")]}
    good = {"package": [_pkg("heapless", "0.8.0"), {"name": "local", "version": "0.0.0"}]}
    planted = {"package": [_pkg("heapless", "0.8.1")]}
    unpinned = {"package": [_pkg("libc", "0.2.1")]}
    other_major = {"package": [_pkg("heapless", "0.9.0")]}
    assert cp.subset_violations(pins, good) == [], "a subset lock was reported"
    assert cp.subset_violations(pins, planted) == [("heapless", "0.8.1", "0.8.0")], "planted mismatch missed"
    assert cp.subset_violations(pins, unpinned) == [("libc", "0.2.1", None)], "unpinned crate missed"
    assert cp.subset_violations(pins, other_major) == [("heapless", "0.9.0", None)], "other range missed"
    dup = {"package": [_pkg("heapless", "0.8.0"), _pkg("heapless", "0.8.1"), _pkg("heapless", "0.9.0")]}
    assert cp.one_per_range_violations(dup) == [("heapless", "0.8", ["0.8.0", "0.8.1"])], "duplicate missed"
    assert cp.compat_range("0.0.5") == "0.0.5" and cp.compat_range("2.1.0+x") == "2"


def main():
    self_test()
    pins_rel, roots = cp.pin_config(ROOT)
    pins_path = os.path.join(ROOT, pins_rel)
    if not os.path.exists(pins_path):
        print(f"check-cargo-registry-pins: {pins_rel} is missing -- run "
              "`python3 scripts/gen-cargo-registry-pins.py`", file=sys.stderr)
        return 1
    pins = cp.load_lock(pins_path)
    failed = False

    for name, rng, versions in cp.one_per_range_violations(pins):
        print(f"check-cargo-registry-pins: {pins_rel} pins {name} {rng}.x more than once: "
              f"{', '.join(versions)}", file=sys.stderr)
        failed = True

    # The tracked-lock invariant (RFC-0104 D3, phase-485 W2): a `Cargo.lock` is
    # tracked IFF its directory is a locked root. Every other cargo root is a
    # LEAF, whose lock is seeded from the pins on its first build and never
    # committed — so a leaf lock in git is a second, unreviewed pin source.
    tracked = set(cp.tracked_locks(ROOT))
    declared = {cp.lock_path(r) for r in roots}
    for rel in sorted(tracked - declared):
        print(f"check-cargo-registry-pins: {rel} is tracked, but its directory is not in "
              "`[rust] locked_roots`. A leaf lock is seeded, not committed: `git rm --cached "
              f"{rel}` (or, for a closed-graph tool root, add the directory to locked_roots).",
              file=sys.stderr)
        failed = True
    for rel in sorted(declared - tracked):
        print(f"check-cargo-registry-pins: `[rust] locked_roots` names {os.path.dirname(rel) or '.'}, "
              f"but {rel} is not tracked.", file=sys.stderr)
        failed = True

    for r in roots:
        rel = cp.lock_path(r)
        if rel not in tracked:
            continue
        for name, have, want in cp.subset_violations(pins, cp.load_lock(os.path.join(ROOT, rel))):
            where = f"pinned at {want}" if want else "not pinned in that range"
            print(f"check-cargo-registry-pins: {rel} locks {name} {have}, {where} in {pins_rel}",
                  file=sys.stderr)
            failed = True

    gen = subprocess.run([sys.executable, os.path.join(ROOT, "scripts", "gen-cargo-registry-pins.py"),
                          "--check"], capture_output=True, text=True)
    if gen.returncode != 0:
        sys.stderr.write(gen.stderr)
        failed = True

    if failed:
        print("Remedy: `just lock-update <crate> <version> <root>`, then "
              "`python3 scripts/gen-cargo-registry-pins.py`; commit both.", file=sys.stderr)
        return 1
    print(f"check-cargo-registry-pins: {len(cp.registry_packages(pins))} pins, one per range; "
          f"{len(roots)} locked root(s) are subsets")
    return 0


if __name__ == "__main__":
    sys.exit(main())

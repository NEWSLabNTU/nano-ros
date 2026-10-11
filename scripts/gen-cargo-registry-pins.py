#!/usr/bin/env python3
"""Regenerate the Rust registry pin set (RFC-0104 D2, phase-485 W1).

    python3 scripts/gen-cargo-registry-pins.py           # write it
    python3 scripts/gen-cargo-registry-pins.py --check   # exit 1 if it would change

One version per semver-compatible range, chosen in this order:

  1. the locked roots (`[rust] locked_roots`): what the repo root and the CLI
     resolve IS the pin, and the two must agree (a disagreement is an error
     naming both, because the fix is a `just lock-update` in one of them);
  2. the pin file already committed, so a range only some leaf uses keeps
     its pin once the leaf locks that first supplied it are gone;
  3. otherwise the highest version any other tracked `Cargo.lock` holds.

Nothing is dropped: a range no lock names any more stays pinned, because a
leaf that is not built on this host may still resolve it.
"""

import argparse
import os
import sys

sys.path.insert(0, os.path.join(os.path.dirname(os.path.abspath(__file__)), "lib"))
import cargo_pins as cp  # noqa: E402

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))


def main():
    ap = argparse.ArgumentParser(description=__doc__.split("\n")[0])
    ap.add_argument("--check", action="store_true", help="report drift, write nothing")
    args = ap.parse_args()

    pins_rel, roots = cp.pin_config(ROOT)
    pins_path = os.path.join(ROOT, pins_rel)
    chosen = {}  # (name, range) -> package dict
    rank = {}    # (name, range) -> 1 | 2 | 3

    errors = []
    for r in roots:
        rel = cp.lock_path(r)
        for p in cp.registry_packages(cp.load_lock(os.path.join(ROOT, rel))):
            k = (p["name"], cp.compat_range(p["version"]))
            if k in chosen and rank[k] == 1 and chosen[k]["version"] != p["version"]:
                errors.append(f"  {p['name']}: {chosen[k]['version']} ({chosen[k]['_from']}) vs "
                              f"{p['version']} ({rel})")
                continue
            chosen[k], rank[k] = dict(p, _from=rel), 1
    if errors:
        print("gen-cargo-registry-pins: the locked roots disagree within a semver range:", file=sys.stderr)
        print("\n".join(errors), file=sys.stderr)
        print("Move the lower one with `just lock-update <crate> <version> <root>`, then rerun.",
              file=sys.stderr)
        return 1

    if os.path.exists(pins_path):
        for p in cp.registry_packages(cp.load_lock(pins_path)):
            k = (p["name"], cp.compat_range(p["version"]))
            if k not in chosen:
                chosen[k], rank[k] = dict(p, _from=pins_rel), 2

    root_locks = {cp.lock_path(r) for r in roots}
    for rel in cp.tracked_locks(ROOT):
        if rel in root_locks:
            continue
        for p in cp.registry_packages(cp.load_lock(os.path.join(ROOT, rel))):
            k = (p["name"], cp.compat_range(p["version"]))
            if k in chosen and rank[k] < 3:
                continue
            if k not in chosen or cp.version_key(p["version"]) > cp.version_key(chosen[k]["version"]):
                chosen[k], rank[k] = dict(p, _from=rel), 3

    text = cp.render(chosen.values())
    current = open(pins_path).read() if os.path.exists(pins_path) else None
    if args.check:
        if current != text:
            print(f"{pins_rel} is stale: run `python3 scripts/gen-cargo-registry-pins.py`", file=sys.stderr)
            return 1
        print(f"{pins_rel} is current ({len(chosen)} pins)")
        return 0
    if current != text:
        os.makedirs(os.path.dirname(pins_path), exist_ok=True)
        with open(pins_path, "w") as fh:
            fh.write(text)
    print(f"{pins_rel}: {len(chosen)} pins "
          f"({sum(1 for v in rank.values() if v == 1)} from the locked roots)")
    return 0


if __name__ == "__main__":
    sys.exit(main())

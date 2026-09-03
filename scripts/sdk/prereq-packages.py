#!/usr/bin/env python3
"""Print the OS packages `[prereq.*]` declares for a manager — phase-413 W3.

WHY A SCRIPT AND NOT `nros setup --system`

`nros setup --system` is the user-facing verb and stays so. This exists for the
jobs that need two packages and nothing else from the toolchain: making a docs
deploy build the `nros` CLI to learn that `doxygen` is spelled `doxygen` costs
minutes to answer a question the index answers in milliseconds.

It is not a second source of truth. It reads `nros-sdk-index.toml`, the same
file the CLI reads, exactly as `check-dist-runtime-deps.py` already does — the
SSoT is the index, not any one consumer of it.

WHY IT REFUSES UNKNOWN KEYS

Silently printing nothing for a typo would install nothing and fail later at the
compiler, which is the shape RFC-0062 exists to delete. An unknown key is an
error naming the key, the same rung the `<depend>` ladder ends on.

Usage:
    prereq-packages.py --manager apt doxygen graphviz
    prereq-packages.py --self-test
"""

import argparse
import os
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
INDEX = os.path.join(ROOT, "nros-sdk-index.toml")
MANAGERS = ("apt", "dnf", "pacman", "brew")


def load(path=INDEX):
    try:
        import tomllib as toml
    except ModuleNotFoundError:  # py<3.11
        import tomli as toml
    with open(path, "rb") as fh:
        return toml.load(fh)


def packages_for(index, manager, keys):
    """The manager's packages for `keys`, in the order given, deduped.

    Raises on an unknown key, or on a key the index declares for no package
    under this manager — "declared but not for your OS" is a real answer and a
    silent empty string is not.
    """
    prereq = index.get("prereq", {})
    out, seen, missing, unmapped = [], set(), [], []
    for k in keys:
        entry = prereq.get(k)
        if entry is None:
            missing.append(k)
            continue
        pkgs = entry.get(manager) or []
        if not pkgs:
            unmapped.append(k)
            continue
        for p in pkgs:
            if p not in seen:
                seen.add(p)
                out.append(p)
    if missing:
        raise SystemExit(
            f"prereq-packages: no [prereq.{missing[0]}] in nros-sdk-index.toml"
            + (f" (and {len(missing) - 1} more)" if len(missing) > 1 else "")
            + "\n  Declare it there — the index is the SSoT (RFC-0062)."
        )
    if unmapped:
        raise SystemExit(
            f"prereq-packages: [prereq.{unmapped[0]}] declares no `{manager}` package"
            + (f" (and {len(unmapped) - 1} more)" if len(unmapped) > 1 else "")
            + f"\n  Add the `{manager} = [..]` line to that entry."
        )
    return out


def self_test():
    index = {
        "prereq": {
            "doxygen": {"apt": ["doxygen"], "dnf": ["doxygen"]},
            "graphviz": {"apt": ["graphviz"]},
            "dup": {"apt": ["doxygen"]},
        }
    }
    failures = 0

    got = packages_for(index, "apt", ["doxygen", "graphviz"])
    if got != ["doxygen", "graphviz"]:
        print(f"  FAIL: order/content {got}")
        failures += 1

    # A package named by two keys is emitted once — the caller pastes this into
    # one `apt-get install` line.
    if packages_for(index, "apt", ["doxygen", "dup"]) != ["doxygen"]:
        print("  FAIL: duplicate package not deduped")
        failures += 1

    for keys, why in ((["nope"], "unknown key"), (["graphviz"], "unmapped manager")):
        manager = "apt" if why == "unknown key" else "dnf"
        try:
            packages_for(index, manager, keys)
        except SystemExit:
            pass
        else:
            print(f"  FAIL: {why} did not raise")
            failures += 1

    if failures:
        print(f"prereq-packages self-test: {failures} case(s) FAILED")
        return 1
    print("prereq-packages self-test: OK")
    return 0


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--manager", default="apt", choices=MANAGERS)
    ap.add_argument("--self-test", action="store_true")
    ap.add_argument("keys", nargs="*")
    args = ap.parse_args()
    if args.self_test:
        return self_test()
    if not args.keys:
        ap.error("name at least one [prereq.*] key")
    print(" ".join(packages_for(load(), args.manager, args.keys)))
    return 0


if __name__ == "__main__":
    sys.exit(main())

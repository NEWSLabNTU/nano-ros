#!/usr/bin/env python3
"""A `system.toml` lives only in a bringup package — RFC-0121 D2/D7, phase-531 W2.

THE RULE. `system.toml` is the project layer (RFC-0103 D1): it states the
board, the RMW, the images and the components. A bringup package holds it and
nothing else is required of a bringup — `package.xml` plus `system.toml`. A
NODE package holds code. A tracked `system.toml` in a directory that also holds
a tracked `Cargo.toml` or `CMakeLists.txt` is the standalone ("leaf") shape
RFC-0121 retires: a project-layer file in a package directory, built by its own
road.

THE RATCHET. Phase-531 W1 measured 223 such files on 2026-11-05 (examples/ 162,
templates 11, packages/testing/ 50). They are listed in
`.config/system-toml-in-bringup-baseline.txt`, generated from that measurement,
and the list may only SHRINK:

  * a beside-code `system.toml` NOT in the list fails — a new one is the shape
    being retired, so write a bringup instead (`<ws>/src/<pkg>_bringup/` with
    `package.xml` and `system.toml`), or move an existing one with
    `nros dev migrate-bringup <dir>` once phase-531 W6 lands it;
  * a listed path that no longer violates (migrated, deleted, or its build file
    gone) fails until its line is deleted, so the list cannot go stale and a
    migration cannot leave its row behind;
  * `--write-baseline` rewrites the list from the tree and REFUSES to add a
    path, so regenerating it can only shrink it.

The predicate is asked of the TRACKED set (`git ls-files`), never the working
tree: an untracked scratch file is not the repository's shape, and a gate
reading the disk would report a contributor's leftovers.

Phase-531 W6 (and phase-535 for the paired examples, RFC-0122) empties the
list; the phase's acceptance is this gate with no allowlist at all.

  check-system-toml-in-bringup.py [--self-test | --list | --write-baseline]
"""

import os
import subprocess
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
BASELINE = os.path.join(ROOT, ".config", "system-toml-in-bringup-baseline.txt")
BUILD_FILES = ("Cargo.toml", "CMakeLists.txt")
HEADER = """\
# check-system-toml-in-bringup (RFC-0121 D7, phase-531 W2) — tracked
# `system.toml` files that still sit beside a `Cargo.toml` or `CMakeLists.txt`
# (the standalone package shape RFC-0121 retires). Generated from phase-531
# W1's measurement (223 on 2026-11-05).
#
# A RATCHET that may only SHRINK: a new beside-code `system.toml` fails, and a
# listed path that no longer violates fails until its line is deleted. Phase-531
# W6 and phase-535 empty it. Regenerate (shrink only):
#   python3 scripts/check-system-toml-in-bringup.py --write-baseline
"""


def tracked_files():
    out = subprocess.run(["git", "-C", ROOT, "ls-files", "-z"],
                         capture_output=True, check=True).stdout
    return {p.decode() for p in out.split(b"\0") if p}


def beside_code(files):
    """Every tracked `system.toml` whose directory holds a tracked build file."""
    found = []
    for f in files:
        if f != "system.toml" and not f.endswith("/system.toml"):
            continue
        d = f[: -len("system.toml")]
        if any(d + b in files for b in BUILD_FILES):
            found.append(f)
    return sorted(found)


def load_baseline(path=BASELINE):
    if not os.path.exists(path):
        return []
    rows = []
    for line in open(path, encoding="utf-8"):
        line = line.split("#", 1)[0].strip()
        if line:
            rows.append(line)
    return rows


def judge(files, baseline):
    """(new, stale): violations not listed, and listed paths that no longer violate."""
    found = set(beside_code(files))
    listed = set(baseline)
    return sorted(found - listed), sorted(listed - found)


def self_test():
    ok = True

    def expect(cond, what):
        nonlocal ok
        if not cond:
            ok = False
            print(f"check-system-toml-in-bringup self-test FAILED: {what}", file=sys.stderr)

    tree = {
        "ws/src/demo_bringup/package.xml",
        "ws/src/demo_bringup/system.toml",
        "ws/src/talker/package.xml",
        "ws/src/talker/Cargo.toml",
        "leaf/Cargo.toml",
        "leaf/system.toml",
    }
    base = ["leaf/system.toml"]
    expect(judge(tree, base) == ([], []), "the measured tree passes against its own list")
    expect(beside_code(tree) == ["leaf/system.toml"], "a bringup's system.toml is not code-adjacent")
    # Mutation 1 — a NEW system.toml beside a Cargo.toml fails.
    t = tree | {"ws/src/talker/system.toml"}
    expect(judge(t, base)[0] == ["ws/src/talker/system.toml"],
           "a new system.toml beside a Cargo.toml fails")
    # ... and beside a CMakeLists.txt.
    t = tree | {"c/CMakeLists.txt", "c/system.toml"}
    expect(judge(t, base)[0] == ["c/system.toml"], "a new system.toml beside a CMakeLists.txt fails")
    # ... including at the repository root.
    t = tree | {"Cargo.toml", "system.toml"}
    expect(judge(t, base)[0] == ["system.toml"], "a root system.toml beside a Cargo.toml fails")
    # Mutation 2 — removing an allowlisted site without deleting its row fails.
    t = tree - {"leaf/system.toml"}
    expect(judge(t, base) == ([], ["leaf/system.toml"]),
           "a migrated site whose row stayed fails as stale")
    # ... and so does a site whose BUILD FILE went (it is a bringup now).
    t = tree - {"leaf/Cargo.toml"}
    expect(judge(t, base) == ([], ["leaf/system.toml"]),
           "a site that became a bringup fails until its row goes")
    # Only the exact file name counts.
    t = tree | {"x/Cargo.toml", "x/system.toml.in", "x/my-system.toml"}
    expect(judge(t, base) == ([], []), "a file merely named like system.toml is not one")
    return ok


def write_baseline(files):
    current = beside_code(files)
    old = set(load_baseline())
    grown = [p for p in current if p not in old]
    if os.path.exists(BASELINE) and grown:
        print("check-system-toml-in-bringup: --write-baseline REFUSED — it may only shrink "
              "the list, and these are new:", file=sys.stderr)
        for p in grown:
            print(f"  {p}", file=sys.stderr)
        return 1
    with open(BASELINE, "w", encoding="utf-8") as fh:
        fh.write(HEADER)
        for p in current:
            fh.write(p + "\n")
    print(f"check-system-toml-in-bringup: wrote {len(current)} row(s) to "
          f"{os.path.relpath(BASELINE, ROOT)}")
    return 0


def main():
    if not self_test():
        return 1
    if "--self-test" in sys.argv:
        print("check-system-toml-in-bringup self-test: OK (8 cases)")
        return 0
    files = tracked_files()
    if "--list" in sys.argv:
        print("\n".join(beside_code(files)))
        return 0
    if "--write-baseline" in sys.argv:
        return write_baseline(files)
    new, stale = judge(files, load_baseline())
    if new or stale:
        print("check-system-toml-in-bringup: FAILED (RFC-0121 D7, phase-531 W2)", file=sys.stderr)
        for p in new:
            print(f"  {p}: a system.toml beside a Cargo.toml/CMakeLists.txt", file=sys.stderr)
        if new:
            print("  system.toml lives only in a bringup package (package.xml + system.toml,\n"
                  "  no build file). Put it in `<ws>/src/<pkg>_bringup/`, and keep the code in\n"
                  "  its own node package (RFC-0121 D1/D2; `nros dev migrate-bringup <dir>`).\n"
                  "  The allowlist only shrinks: do not add the path to it.", file=sys.stderr)
        for p in stale:
            print(f"  {p}: listed in {os.path.relpath(BASELINE, ROOT)} but no longer beside "
                  "code — delete its line", file=sys.stderr)
        return 1
    n = len(load_baseline())
    print(f"check-system-toml-in-bringup: OK — no new system.toml beside code; "
          f"{n} allowlisted site(s) left to migrate")
    return 0


if __name__ == "__main__":
    sys.exit(main())

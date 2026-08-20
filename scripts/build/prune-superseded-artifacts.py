#!/usr/bin/env python3
"""Delete cargo artifacts that a later build superseded — keep the newest per slot.

# Why this exists

Cargo names an artifact after its `-C metadata` hash, so when a crate's identity
changes it writes a NEW file beside the old one and never collects the old:

    cargo/<root>/nros-relwithdebinfo/deps/
        libnros_core-2b8d1fd2adda0290.rlib   (an earlier build era)
        libnros_core-61324ec517cf714a.rlib   (another)
        libnros_core-<today>.rlib            (the one cargo will link)

`check-artifact-identity-budget` counts IDENTITIES, so a long-lived incremental
tree can bust the budget on history alone while the current build sits exactly
on it. Measured in the ROS distrobox on 2026-08-20: `nros_core` held 12 rlibs —
three build eras (Aug 5 / Aug 7 / Aug 20) x the 4 slots the budget decomposes
into (2 workspace roots x 2 R3 halves). Today's build had produced exactly 4,
the budgeted number. The gate was right and the tree was old.

That script's own documented remedy is "delete the tree and rebuild", which is
correct and costs a full mixed rebuild. This is the cheap equivalent: it removes
only what is already unreachable, so nothing has to be rebuilt.

# The rule, and why it is this one

Group by `(directory, crate stem, extension)`, keep the NEWEST, delete the rest.

The newest file in a slot is the one cargo links; every older one is an
unreferenced identity — no fingerprint names it, no manifest points at it.
Deleting those cannot cost a rebuild.

A date cutoff was the obvious alternative and is WRONG: a crate that simply was
not rebuilt recently still has its current artifact dated old, so a cutoff
deletes live output and forces work. "Newest per slot" is the property that
makes this free; "older than X" is not.

# Deliberately NOT a cargo gc

Only DUPLICATE identities go. A slot holding a single stale file is left alone,
and `.fingerprint/`, build-script output and incremental dirs are untouched.
This answers the identity-budget question and nothing else — which is why it is
safe to run without thinking about it.

Usage:

    scripts/build/prune-superseded-artifacts.py <build-tree> [--apply]
    scripts/build/prune-superseded-artifacts.py --self-test

Dry run by default: it prints what it would remove and exits 0.
"""

import collections
import os
import re
import sys
import tempfile

# `lib<crate>-<hash>.<ext>` and `<crate>-<hash>.<ext>`. The hash is hex and at
# least 8 chars; the stem may itself contain dashes, so the LAST dash-hash pair
# before the extension is the identity.
PAT = re.compile(r"^(?P<stem>.+)-(?P<hash>[0-9a-f]{8,})\.(?P<ext>rlib|rmeta|d|so|a)$")


def plan(root):
    """[(kept, [superseded…])] for every slot holding more than one identity."""
    groups = collections.defaultdict(list)
    for dirpath, _dirs, files in os.walk(root):
        for f in files:
            m = PAT.match(f)
            if not m:
                continue
            p = os.path.join(dirpath, f)
            try:
                groups[(dirpath, m.group("stem"), m.group("ext"))].append(
                    (os.lstat(p).st_mtime, p)
                )
            except OSError:
                pass

    out = []
    for _key, v in groups.items():
        if len(v) < 2:
            continue
        v.sort(reverse=True)  # newest first
        out.append((v[0][1], [p for _mt, p in v[1:]]))
    return out


def self_test():
    """Both directions: keep the newest, and never touch a lone artifact."""
    bad = []
    with tempfile.TemporaryDirectory() as tmp:
        deps = os.path.join(tmp, "deps")
        os.makedirs(deps)

        def touch(name, mtime):
            p = os.path.join(deps, name)
            open(p, "w").close()
            os.utime(p, (mtime, mtime))
            return p

        old = touch("libnros_core-aaaaaaaaaaaaaaaa.rlib", 1000)
        mid = touch("libnros_core-bbbbbbbbbbbbbbbb.rlib", 2000)
        new = touch("libnros_core-cccccccccccccccc.rlib", 3000)
        # A lone artifact, and one whose extension differs: neither is a duplicate.
        lone = touch("libnros_rmw-dddddddddddddddd.rlib", 1000)
        meta = touch("libnros_core-cccccccccccccccc.rmeta", 1000)

        doomed = {p for _kept, ps in plan(tmp) for p in ps}
        if doomed != {old, mid}:
            bad.append(f"expected exactly the two superseded rlibs, got {sorted(doomed)}")
        for keep in (new, lone, meta):
            if keep in doomed:
                bad.append(f"must not delete {os.path.basename(keep)}")

        # A tree with no duplicates must plan nothing at all.
        with tempfile.TemporaryDirectory() as clean:
            os.makedirs(os.path.join(clean, "deps"))
            open(os.path.join(clean, "deps", "libx-1111111111111111.rlib"), "w").close()
            if plan(clean):
                bad.append("a duplicate-free tree must plan no deletions")

    if bad:
        for b in bad:
            sys.stderr.write(f"prune-superseded-artifacts --self-test: {b}\n")
        return 2
    print("prune-superseded-artifacts --self-test: OK (2 case(s))")
    return 0


def main(argv):
    if "--self-test" in argv:
        return self_test()
    args = [a for a in argv if not a.startswith("--")]
    if len(args) != 1:
        sys.stderr.write(__doc__.split("Usage:")[1])
        return 2
    root = args[0]
    if not os.path.isdir(root):
        sys.stderr.write(f"prune-superseded-artifacts: not a directory: {root}\n")
        return 2

    slots = plan(root)
    doomed = [p for _kept, ps in slots for p in ps]
    freed = 0
    for p in doomed:
        try:
            freed += os.lstat(p).st_size
        except OSError:
            pass

    print(f"prune-superseded-artifacts: {root}")
    print(f"  slots with more than one identity: {len(slots)}")
    print(f"  superseded files: {len(doomed)}  ({freed / 1e9:.2f} GB)")
    if not doomed:
        print("  nothing to do — every slot holds one identity")
        return 0
    if "--apply" not in argv:
        print("  (dry run — pass --apply to delete)")
        return 0

    n = 0
    for p in doomed:
        try:
            os.remove(p)
            n += 1
        except OSError as e:
            sys.stderr.write(f"  could not remove {p}: {e}\n")
    print(f"  deleted {n} file(s); nothing needs rebuilding (the kept copy is the linked one)")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))

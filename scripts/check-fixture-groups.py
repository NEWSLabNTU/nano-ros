#!/usr/bin/env python3
"""phase-340 W2 — the preconditions a fixture GROUP has to meet before a
platform may share one cargo target dir.

Phase 226.D gave compatible Rust fixture rows ONE `--target-dir` per group so
the shared nano-ros crates compile once for the group instead of once per
example dir.  A group's members therefore write into one flat
`<group>/[<triple>/]<profile>/` namespace — and cargo does NOT hash the final
artifact name the way it hashes `deps/`.  Two rows in one group that produce a
binary of the SAME name overwrite each other, last writer wins, and the test
resolver for one of them silently gets the other's binary.  A green test on the
wrong artifact is the worst outcome in this repository's failure taxonomy, so
the invariant is checked rather than hoped for.

Nothing enforced this before.  `qemu-arm-baremetal` — the one migrated platform
— happens to be collision-free, so the eligibility gate
(`NROS_FIXTURE_SHARED_PLATFORMS`) has been protecting the property by accident.
`linux`, the platform phase-340 W2.a wants to add next, is NOT collision-free.

Three arms, all fatal:

  A1  For every platform ALREADY in `NROS_FIXTURE_SHARED_PLATFORMS`, no two
      distinct packages inside one group claim the same artifact name.

  A2  For every such platform, every group it produces is the DEFAULT group
      (slug == platform).  This is not a style rule: the Rust resolver
      (`fixture_shared_target_dir` in nros-tests) can express no other shape —
      it returns `build_dir("fixtures-cargo", &[platform])` and has no mirror of
      the shell's hashed variant slug.  Adding a platform whose rows carry
      features or env would make the BUILD write `fixtures-cargo/<plat>-<hash>`
      while the TEST looked in `fixtures-cargo/<plat>` — issue #393 verbatim.
      When the Rust side learns variant groups, replace this arm with the
      agreement check; do not simply delete it.

  A3  The collisions that BLOCK each not-yet-shared platform are frozen in
      `BLOCKED` below.  A budget, in the same spirit as
      `check-artifact-identity-budget`: it fails when a new collision appears
      (a regression) AND when a recorded one is fixed (so the migration list
      and this file move together instead of drifting apart).

The group key itself is NOT reimplemented here.  `nros_fixture_group` in
`scripts/build/fixtures-target-dir.sh` is the one derivation (RFC-0070 R3) and
this gate shells into it, so a change to the key cannot pass a gate that
mirrors the old one.

Buildless and source-free: it reads `examples/fixtures.toml` and the leaf
`Cargo.toml`s, both tracked, so it runs on a pristine per-push checkout.
"""

import collections
import os
import subprocess
import sys

try:
    import tomllib  # Python 3.11+
except ModuleNotFoundError:  # 3.10 and earlier
    import tomli as tomllib

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
SEP = "\x1f"

# A3 — the frozen blocker list.  Measured 2026-08-08 over the whole manifest.
#
# `linux` is the platform phase-340 W2.a step 2 wants to add to
# NROS_FIXTURE_SHARED_PLATFORMS.  These two artifact names are what stops it:
# the custom-transport examples name their binaries `talker` / `listener`, the
# same names the plain talker / listener examples use, and all four rows are
# default-feature `linux` rows — i.e. the SAME group.
#
# Format: platform -> sorted list of "group::binary <- pkg, pkg" strings.
#
# The OWNERS are part of the key on purpose. The first version keyed on
# `group::binary` alone and could not fail when a THIRD package started
# claiming an already-recorded name: renaming `native-rs-xrce-serial-talker`'s
# binary to `talker` left the budget reading exactly the same two entries and
# the gate green. Tripwired again with the owners in the key, which fails.
BLOCKED = {
    "linux": [
        "linux::listener <- native-rs-custom-transport-listener, native-rs-listener",
        "linux::talker <- native-rs-custom-transport-talker, native-rs-talker",
    ],
}


def rows():
    """Every rust fixture row as (platform, dir, env, cargo_args).

    Straight from `fixtures-manifest.py`, which is the manifest's only reader —
    this gate does not parse `examples/fixtures.toml` itself.
    """
    out = subprocess.run(
        [
            sys.executable,
            os.path.join(ROOT, "scripts/build/fixtures-manifest.py"),
            "list",
            "--lang",
            "rust",
            "--with-platform",
        ],
        cwd=ROOT,
        capture_output=True,
        text=True,
        check=True,
    ).stdout
    for line in out.splitlines():
        if not line.strip():
            continue
        platform, directory, env, args = line.split(SEP)
        yield platform, directory, env, args


def groups_for(records):
    """Group slug per record, computed by the SHELL derivation.

    One `bash` invocation for the whole manifest rather than one per row: the
    point is to call `nros_fixture_group` itself, not to be fast about it, but
    240 shell spawns would make this gate the slowest thing in `check-fast`.
    """
    program = (
        "set -u\n"
        ". scripts/build/fixtures-target-dir.sh\n"
        "while IFS=$'\\x1f' read -r platform args envstr; do\n"
        '    printf "%s\\n" "$(nros_fixture_group_slug "$platform" "$args" "$envstr")"\n'
        "done\n"
    )
    stdin = "".join(f"{p}{SEP}{a}{SEP}{e}\n" for p, _d, e, a in records)
    res = subprocess.run(
        ["bash", "-c", program],
        cwd=ROOT,
        input=stdin,
        capture_output=True,
        text=True,
    )
    if res.returncode != 0:
        sys.exit(
            "check-fixture-groups: the shell group derivation failed:\n" + res.stderr
        )
    out = res.stdout.splitlines()
    if len(out) != len(records):
        sys.exit(
            f"check-fixture-groups: derivation emitted {len(out)} slugs for "
            f"{len(records)} rows — the batch program and the manifest disagree"
        )
    return out


def shared_platforms():
    """`NROS_FIXTURE_SHARED_PLATFORMS`, read from the shell file that owns it."""
    res = subprocess.run(
        ["bash", "-c", '. scripts/build/fixtures-target-dir.sh; printf "%s" '
         '"$NROS_FIXTURE_SHARED_PLATFORMS"'],
        cwd=ROOT,
        capture_output=True,
        text=True,
        check=True,
    )
    return res.stdout.split()


def artifacts(directory):
    """Artifact names a leaf manifest produces, as (name, package) pairs.

    Only `[[bin]]` targets and the implicit `src/main.rs` binary: those are what
    land in the flat `<profile>/` namespace under a shared target dir.  Libs are
    hashed into `deps/` and cannot collide.
    """
    manifest = os.path.join(ROOT, directory, "Cargo.toml")
    if not os.path.isfile(manifest):
        # A manifest-declared row whose dir has no Cargo.toml is a different
        # bug; `check-fixtures-manifest` owns it.  Say so rather than guessing
        # an artifact name.
        return [("<no-manifest>", directory)]
    with open(manifest, "rb") as fh:
        data = tomllib.load(fh)
    package = data.get("package", {}).get("name", directory)
    names = [(b["name"], package) for b in data.get("bin", []) if b.get("name")]
    if not names and os.path.isfile(os.path.join(ROOT, directory, "src/main.rs")):
        names = [(package, package)]
    return names


def collisions(by_group):
    """group -> {binary -> {(package, dir), ...}} for names claimed twice."""
    found = {}
    for group, members in sorted(by_group.items()):
        owners = collections.defaultdict(set)
        for directory in sorted(members):
            for name, package in artifacts(directory):
                owners[name].add((package, directory))
        clash = {n: o for n, o in owners.items() if len(o) > 1}
        if clash:
            found[group] = clash
    return found


def main():
    records = list(rows())
    if not records:
        sys.exit("check-fixture-groups: no rust fixture rows — refusing to pass on nothing")
    slugs = groups_for(records)

    per_platform = collections.defaultdict(lambda: collections.defaultdict(set))
    for (platform, directory, _env, _args), slug in zip(records, slugs):
        per_platform[platform][slug].add(directory)

    shared = shared_platforms()
    failures = []

    # --- A1 + A2: the platforms that ALREADY share a dir -------------------
    for platform in shared:
        by_group = per_platform.get(platform)
        if not by_group:
            failures.append(
                f"A1/A2: {platform!r} is in NROS_FIXTURE_SHARED_PLATFORMS but the "
                f"manifest has no rust rows for it — the eligibility list names a "
                f"platform this gate cannot check"
            )
            continue
        for group, clash in collisions(by_group).items():
            for name, owners in sorted(clash.items()):
                who = ", ".join(f"{pkg} ({d})" for pkg, d in sorted(owners))
                failures.append(
                    f"A1: group {group!r} has two packages claiming artifact "
                    f"{name!r}: {who}"
                )
        for group in sorted(by_group):
            if group != platform:
                failures.append(
                    f"A2: {platform!r} produces the variant group {group!r}, which "
                    f"the Rust resolver cannot express (fixture_shared_target_dir "
                    f"returns fixtures-cargo/<platform> only). Teach it the variant "
                    f"slug — and update this arm — before sharing this platform."
                )

    # --- A3: the frozen blocker budget for everything else -----------------
    observed = {}
    for platform, by_group in per_platform.items():
        if platform in shared:
            continue
        keys = sorted(
            "{}::{} <- {}".format(
                group, name, ", ".join(sorted(pkg for pkg, _d in owners))
            )
            for group, clash in collisions(by_group).items()
            for name, owners in clash.items()
        )
        if keys:
            observed[platform] = keys

    for platform in sorted(set(observed) | set(BLOCKED)):
        want = BLOCKED.get(platform, [])
        got = observed.get(platform, [])
        if want != got:
            failures.append(
                f"A3: the recorded blocker list for {platform!r} is stale.\n"
                f"      recorded: {want}\n"
                f"      observed: {got}\n"
                f"      If you FIXED one, delete it from BLOCKED in this file (and "
                f"say so in the phase-340 W2 notes). If you ADDED one, you have "
                f"just made {platform!r} harder to migrate — rename the binary."
            )

    if failures:
        sys.stderr.write("check-fixture-groups: FAILED\n")
        for f in failures:
            sys.stderr.write(f"  {f}\n")
        return 1

    n_groups = sum(len(per_platform[p]) for p in shared)
    n_rows = sum(len(m) for p in shared for m in per_platform[p].values())
    print(
        f"fixture groups: {len(shared)} shared platform(s), {n_groups} group(s), "
        f"{n_rows} row(s) — no artifact-name collisions; "
        f"{len(BLOCKED)} platform(s) recorded as blocked."
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())

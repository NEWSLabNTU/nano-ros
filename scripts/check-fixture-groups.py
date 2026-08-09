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

**The gate is also what refuted phase-340 W1's proposed platform-grained group
key** (2026-08-08).  Under that key `linux`'s 65 rows become one group, and
`examples/native/rust/talker`'s four rows — default, `rmw-zenoh`, `rmw-xrce`,
`link-tls` — all write `fixtures-cargo/linux/<profile>/talker`.  The shipped
version of this gate reported "no artifact-name collisions" for exactly that
configuration, because A1 keyed its owner set on the leaf DIRECTORY and the four
rows share one.  Widening the key to the ROW turns the same experiment into 11
failures on `linux` and 6 on `threadx-linux`.  To reproduce: replace the body of
`nros_fixture_group_slug` with `printf '%s' "$platform"` and run this gate with
`NROS_FIXTURE_SHARED_PLATFORMS="qemu-arm-baremetal linux"`.  Note also that the
coarse key disarms A2 by construction — every group becomes the default group —
so A2 could not have caught it either.  A change to the KEY must be evaluated
against this gate with the key actually applied, not reasoned about.

The tell was in the PASSING message, not the exit code: it read "2 shared
platform(s), 2 group(s), 61 row(s)" for two platforms carrying 85 rust rows.
61 is 20 + 41 — the dir count.  Both numbers a gate prints are assertions.

Three arms, all fatal:

  A1  For every platform ALREADY in `NROS_FIXTURE_SHARED_PLATFORMS`, no two
      manifest ROWS inside one group claim the same artifact name.  **Rows, not
      leaf directories** — one example built twice with different cargo args is
      two rows and one artifact path.  See `collisions()` for why that
      distinction is the whole arm.

  A2  The AGREEMENT check — phase-340 B2 replaced the old arm here, as that
      arm's own instructions required.

      It used to read "every group a shared platform produces is the DEFAULT
      group (slug == platform)", because the Rust resolver
      (`fixture_shared_target_dir`) returned `build_dir("fixtures-cargo",
      &[platform])` and had no mirror of the shell's hashed variant slug.  A
      platform whose rows carry features or env would have made the BUILD write
      `fixtures-cargo/<plat>-<hash>` while the TEST looked in
      `fixtures-cargo/<plat>` — issue #393 verbatim — so the arm blocked every
      migration except the one accidentally collision-free platform.

      The Rust resolver now reads `fixtures-manifest.py fixture-groups`, which
      pairs each cargo row's `row_artifact_root()` with the slug this same shell
      derivation produces, and rewrites a leaf artifact path onto the group dir
      (`nros_tests::fixtures::groups`).  It can express ANY slug.  What it still
      cannot do is tell apart two rows that share an artifact root — that is the
      key it inverts.  So the arm becomes the two things that are now actually
      load-bearing:

        A2a  the export and this gate's own derivation must agree, row for row.
             Both shell into `nros_fixture_group_batch`, but by different paths
             (`list --lang rust --with-platform` here, `is_cargo_row` +
             `cargo_args()` there), and a resolver reading a table this gate
             never checked is the two-derivations defect one level up.
        A2b  for every SHARED platform, `artifact_root -> slug` must be a
             FUNCTION, with non-empty, pairwise-unnested roots.  Two rows at one
             root resolve to whichever group the inversion happens to pick — a
             green test on the wrong artifact, which is what A1 exists to
             prevent inside a group and this prevents between groups.

  A3  Every artifact-name collision the manifest contains is frozen in
      `KNOWN_COLLISIONS` below.  A budget, in the same spirit as
      `check-artifact-identity-budget`: it fails when a new collision appears
      (a regression) AND when a recorded one is fixed (so the record and the
      migration move together instead of drifting apart).

**A3 observes EVERY platform, shared or not, and that independence is the
point.**  The first version skipped platforms already in the shared list, on
the theory that A1 owns those.  It made A3's remediation actively wrong the
moment anyone ran the obvious experiment: with
`NROS_FIXTURE_SHARED_PLATFORMS="qemu-arm-baremetal linux"`, A1 correctly
reported the two `linux` collisions while A3 observed `[]` — because it had
stopped looking, not because they were gone — and told the reader to delete two
live blockers from the record.  Following that advice would have erased the only
written trace of the collisions and let a later migration hit cargo's silent
`output filename collision` warning.  The gate still exited 1, so the exit code
hid the defect; only reading the message showed it.  A gate whose remedy is
wrong is worse than one that only says "blocked" — issue 0196's rule (a probe
must watch what the gate enforces) applied to a message instead of to coverage.

The group key itself is NOT reimplemented here.  `nros_fixture_group_slug` in
`scripts/build/fixtures-target-dir.sh` is the one derivation (RFC-0070 R3) and
this gate shells into it, so a change to the key cannot pass a gate that
mirrors the old one.  (That function exists BECAUSE of this gate: the key and
the eligibility rule shared one spelling, and a precondition check for
migrating a platform must ask "which group would this row land in?" about a
platform that is by definition not migrated.)

Buildless and source-free: it reads `examples/fixtures.toml` and the leaf
`Cargo.toml`s, both tracked, so it runs on a pristine per-push checkout.
"""

import collections
import os
import subprocess
import sys
from typing import NamedTuple

# The repo's existing spelling — identical to `scripts/build/fixtures-manifest.py`,
# which this gate already shells into. `tomllib` is 3.11+, `tomli` is the 3.10
# backport; a type checker resolves exactly one of the two and flags the other,
# on both files. Keep the two spellings in step rather than inventing a third.
try:
    import tomllib  # Python 3.11+
except ModuleNotFoundError:  # 3.10 and earlier
    import tomli as tomllib


class Row(NamedTuple):
    """One rust fixture row, as `fixtures-manifest.py list --with-platform` emits it."""

    platform: str
    directory: str
    env: str
    cargo_args: str

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
SEP = "\x1f"

# A3 — the frozen collision inventory.  Measured 2026-08-08 over the whole
# manifest, EVERY platform, independent of the shared list (see the module
# docstring for why that independence is load-bearing).
#
# **EMPTY, and that is the end state, not an omission** (phase-340 W2,
# 2026-08-08).  It used to hold `linux`'s two entries — the custom-transport
# examples named their binaries `talker` / `listener`, the same names the plain
# talker / listener examples use, and all four are default-feature `linux` rows,
# i.e. the SAME group.  Both were FIXED rather than recorded further: the two
# binaries are now `custom-transport-talker` / `custom-transport-listener`, so
# the manifest contains no artifact-name collision on ANY platform.  A3 was the
# arm that noticed the fix (it fails in both directions), and it is the arm that
# will notice a regression.
#
# Format: platform -> sorted list of "group::binary <- pkg[variant], …" strings.
# The VARIANT (cargo args + env, `--target-dir` stripped) joined the key in
# phase-340 W1 together with the owner widening in `collisions()`: without it a
# same-package clash records as "pkg, pkg" and cannot say which two rows.
#
# The OWNERS are part of the key on purpose. The first version keyed on
# `group::binary` alone and could not fail when a THIRD package started
# claiming an already-recorded name: renaming `native-rs-xrce-serial-talker`'s
# binary to `talker` left the budget reading exactly the same two entries and
# the gate green. Tripwired again with the owners in the key, which fails.
#
# Adding an entry here is the LAST resort — it says "this collision is known and
# unfixed", which blocks the platform from ever being shared.  Rename the binary
# instead; that is what unblocked `linux`.
KNOWN_COLLISIONS: dict[str, list[str]] = {}


def rows():
    """Every rust fixture row, as `Row`s.

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
        yield Row(*line.split(SEP))


def groups_for(records):
    """Group slug per record, computed by the SHELL derivation.

    One `bash` invocation for the whole manifest rather than one per row: the
    point is to call `nros_fixture_group_slug` itself, not to be fast about it,
    but 240 shell spawns would make this gate the slowest thing in `check-fast`.

    phase-340 B2 — the batch LOOP moved into `fixtures-target-dir.sh` as
    `nros_fixture_group_batch` when a second consumer appeared
    (`fixtures-manifest.py fixture-groups`, which the Rust test resolver reads).
    Two hand-written copies of "source the resolver and loop" is the drift this
    phase deletes; the loop now lives in the file that owns the key.
    """
    program = ". scripts/build/fixtures-target-dir.sh\nnros_fixture_group_batch\n"
    stdin = "".join(
        f"{r.platform}{SEP}{r.cargo_args}{SEP}{r.env}\n" for r in records
    )
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
    # `<slug>\x1f<eligible>`; this gate wants the key, and asks
    # `shared_platforms()` about eligibility itself.
    return [line.split(SEP)[0] for line in out]


class ExportRow(NamedTuple):
    """One line of `fixtures-manifest.py fixture-groups` — the table the Rust
    fixture resolver reads."""

    artifact_root: str
    platform: str
    slug: str
    shared: bool


def export_rows():
    """The group export, as the Rust resolver sees it (phase-340 B2)."""
    out = subprocess.run(
        [
            sys.executable,
            os.path.join(ROOT, "scripts/build/fixtures-manifest.py"),
            "fixture-groups",
        ],
        cwd=ROOT,
        capture_output=True,
        text=True,
        check=True,
    ).stdout
    for line in out.splitlines():
        if not line.strip():
            continue
        root, platform, slug, shared = line.split(SEP)
        yield ExportRow(root, platform, slug, shared == "1")


def path_under(path, directory):
    """Is `path` inside `directory`, comparing whole path COMPONENTS?

    Mirrors `nros_tests::fixtures::lane::path_under`, which is the rule the Rust
    resolver actually inverts with: a textual prefix would say
    `examples/x/target` contains `examples/x/target-zenoh`, a different row in a
    different group.
    """
    p = path.strip("/").split("/")
    d = directory.strip("/").split("/")
    return len(p) >= len(d) and p[: len(d)] == d


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

    Everything that lands UNHASHED in the flat `<profile>/` namespace under a
    shared target dir, which is where a collision silently overwrites:

      * `[[bin]]` targets and the implicit `src/main.rs` binary
      * `[lib]` targets whose crate-type includes `staticlib` / `cdylib` /
        `dylib`

    phase-340 B1 — the second bullet used to be excluded, on the stated ground
    that "libs are hashed into `deps/` and cannot collide".  That is true of
    RLIBS and false of the others, and the tree shows it: `libnros_c.a` exists
    at 438 copies across ~30 distinct sizes.  Measured on a probe crate with
    `crate-type = ["staticlib", "rlib"]` — ONE crate emits BOTH
    `debug/libslibprobe.a` (flat, unhashed, collides) and
    `debug/deps/libslibprobe-6f20517143bd9146.rlib` (hashed, cannot).  So the
    old rule was not wrong about rlibs; it was applied to a category it did not
    cover, which is why the gate could report "no collisions" over a namespace
    it was not looking at.
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

    # phase-340 B1 — the unhashed LIB artifacts.  A lib target's default name is
    # the package name with dashes turned into underscores; cargo prefixes the
    # file with `lib`, so two packages whose lib names agree land on one path.
    lib = data.get("lib", {})
    kinds = set(lib.get("crate-type", lib.get("crate_type", [])))
    if kinds & {"staticlib", "cdylib", "dylib"}:
        lib_name = lib.get("name") or package.replace("-", "_")
        # Report the FILE name, not the crate name: `libnros_c.a` is what the
        # reader will grep for, and it is what actually collides.
        ext = "a" if "staticlib" in kinds else "so"
        names.append((f"lib{lib_name}.{ext}", package))
    return names


def collisions(by_group):
    """group -> {binary -> {(package, dir, variant), ...}} for names claimed twice.

    **The owner is a manifest ROW, not a leaf directory** (phase-340 W1,
    2026-08-08).  The first version keyed on `(package, directory)`, which reads
    as "two different examples must not produce the same binary name" — and that
    is only half the invariant.  The other half is that ONE example built twice
    with different cargo args is also two rows, and a shared `--target-dir` gives
    both of them the same `<group>/<profile>/<bin>` path.

    That half was not academic.  W1 tested the platform-grained group key the
    phase doc proposed and this function reported ZERO collisions for `linux`,
    because `examples/native/rust/talker`'s four rows (default, `rmw-zenoh`,
    `rmw-xrce`, `link-tls`) dedup to one `(package, directory)` owner.  Measured
    on the provisioned tree, those four rows are four DIFFERENT binaries at one
    name — 8616504 / 8616504 / 6514392 / 9034536 bytes, four distinct sha256 —
    and cargo replaces the artifact silently on the second invocation (it warns
    only when one invocation builds both).  Widened, the same experiment reports
    11 collisions on `linux` and 6 on `threadx-linux`.

    Keying on the row also means `--target-dir` must NOT be part of the variant:
    the whole point of a group is that the row's authored dir is stripped
    (`nros_fixture_strip_authored_target_dir`), so two rows differing only there
    would land in one group AND one artifact path — which is a collision to
    report, not a distinction to hide behind.
    """
    found = {}
    for group, members in sorted(by_group.items()):
        owners = collections.defaultdict(set)
        for record in sorted(members):
            for name, package in artifacts(record.directory):
                owners[name].add((package, record.directory, variant_of(record)))
        clash = {n: o for n, o in owners.items() if len(o) > 1}
        if clash:
            found[group] = clash
    return found


def variant_of(record):
    """The row's cargo args + env with any `--target-dir <v>` pair removed.

    Mirrors what `_nros_fixture_variant_sig` drops, for the same reason: under a
    group the authored dir is stripped before cargo sees it, so it cannot
    distinguish two artifacts.  `--target` is deliberately KEPT here even though
    the shell sig drops it — a cross-compiled row writes under `<triple>/` and so
    does not share the host row's artifact path.
    """
    toks = record.cargo_args.split()
    out, i = [], 0
    while i < len(toks):
        if toks[i] == "--target-dir":
            i += 2
            continue
        out.append(toks[i])
        i += 1
    return " ".join(out) + "|" + " ".join(sorted(record.env.split()))


def main():
    records = list(rows())
    if not records:
        sys.exit("check-fixture-groups: no rust fixture rows — refusing to pass on nothing")
    slugs = groups_for(records)

    per_platform = collections.defaultdict(lambda: collections.defaultdict(set))
    for record, slug in zip(records, slugs):
        # The ROW is the member, not its directory — see `collisions`. Storing
        # the directory here is what made the same-package variant clash
        # invisible: 65 `linux` rows collapsed to 41 distinct dirs before any
        # collision logic ran.
        per_platform[record.platform][slug].add(record)

    shared = shared_platforms()
    failures = []
    exported = list(export_rows())

    # --- A2a: the export the Rust resolver reads agrees with this gate ------
    #
    # Multiset of (platform, slug), because the two derivations select rows by
    # different predicates (`--lang rust` here, `is_cargo_row` there) and
    # assemble the cargo args by different call paths. Same shell function
    # underneath — which is the point: if these ever differ, one of the two
    # CALLERS is feeding it something else, and the Rust resolver would be
    # keying on a table nothing else validates.
    here = collections.Counter((r.platform, s) for r, s in zip(records, slugs))
    there = collections.Counter((r.platform, r.slug) for r in exported)
    if here != there:
        only_here = sorted((here - there).elements())
        only_there = sorted((there - here).elements())
        failures.append(
            "A2a: `fixtures-manifest.py fixture-groups` (the table the Rust "
            "fixture resolver reads) disagrees with this gate's derivation.\n"
            f"      only in this gate: {only_here}\n"
            f"      only in the export: {only_there}\n"
            "      Both shell into `nros_fixture_group_batch`, so the key is not "
            "the suspect — the row SELECTION or the cargo-arg assembly on one "
            "side is."
        )

    # --- A2b: the export is invertible for every SHARED platform -----------
    #
    # `nros_tests::fixtures::groups` resolves a leaf artifact path to a group by
    # matching `artifact_root`. That is only well-defined when the root
    # DETERMINES the slug and no root sits inside another.
    shared_export = [r for r in exported if r.shared]
    by_root = collections.defaultdict(set)
    for r in shared_export:
        if not r.artifact_root:
            failures.append(
                f"A2b: a shared {r.platform!r} row in group {r.slug!r} has no "
                f"artifact root, so no resolver can find its binary"
            )
            continue
        by_root[r.artifact_root].add(r.slug)
    for root, group_slugs in sorted(by_root.items()):
        if len(group_slugs) > 1:
            failures.append(
                f"A2b: artifact root {root!r} is claimed by {len(group_slugs)} "
                f"groups ({', '.join(sorted(group_slugs))}). The resolver inverts "
                f"the root, so it would pick one of them arbitrarily and the test "
                f"would run the other group's binary. Give one row its own "
                f"`target_dir`."
            )
    for a in sorted(by_root):
        for b in sorted(by_root):
            if a != b and path_under(a, b):
                failures.append(
                    f"A2b: artifact root {a!r} is nested inside {b!r}; longest-match "
                    f"attribution cannot separate them"
                )

    # --- A1: the platforms that ALREADY share a dir ------------------------
    for platform in shared:
        by_group = per_platform.get(platform)
        if not by_group:
            failures.append(
                f"A1: {platform!r} is in NROS_FIXTURE_SHARED_PLATFORMS but the "
                f"manifest has no rust rows for it — the eligibility list names a "
                f"platform this gate cannot check"
            )
            continue
        for group, clash in collisions(by_group).items():
            for name, owners in sorted(clash.items()):
                who = ", ".join(
                    f"{pkg} ({d} [{var}])" for pkg, d, var in sorted(owners)
                )
                same_pkg = len({pkg for pkg, _d, _v in owners}) == 1
                how = (
                    "one package built with two different cargo arg sets — "
                    "the group key is too COARSE to separate them; make the key "
                    "discriminate, do not rename"
                    if same_pkg
                    else "two packages claiming one name — rename one binary"
                )
                failures.append(
                    f"A1: group {group!r} has {len(owners)} rows claiming artifact "
                    f"{name!r}: {who} ({how})"
                )
    # --- A3: the frozen collision inventory, over EVERY platform -----------
    #
    # Deliberately NOT filtered by `shared`. A1 and A3 answer different
    # questions about the same facts — "is this platform safe to share?" and
    # "has the set of collisions changed?" — and coupling them made A3's
    # remediation invert exactly when A1 fired. See the module docstring.
    observed = {}
    for platform, by_group in per_platform.items():
        keys = sorted(
            "{}::{} <- {}".format(
                group,
                name,
                ", ".join(sorted(f"{pkg}[{var}]" for pkg, _dir, var in owners)),
            )
            for group, clash in collisions(by_group).items()
            for name, owners in clash.items()
        )
        if keys:
            observed[platform] = keys

    for platform in sorted(set(observed) | set(KNOWN_COLLISIONS)):
        want = KNOWN_COLLISIONS.get(platform, [])
        got = observed.get(platform, [])
        if want != got:
            failures.append(
                f"A3: the recorded collision list for {platform!r} is stale.\n"
                f"      recorded: {want}\n"
                f"      observed: {got}\n"
                f"      Every entry below was observed on THIS tree, so a shorter "
                f"`observed` means one was genuinely fixed — delete it from "
                f"KNOWN_COLLISIONS (and say so in the phase-340 W2 notes). A longer "
                f"one means a new artifact-name clash just made {platform!r} harder "
                f"to share — rename the binary rather than recording it."
            )

    if failures:
        sys.stderr.write("check-fixture-groups: FAILED\n")
        for f in failures:
            sys.stderr.write(f"  {f}\n")
        return 1

    n_groups = sum(len(per_platform[p]) for p in shared)
    n_rows = sum(len(m) for p in shared for m in per_platform[p].values())
    n_recorded = sum(len(v) for v in KNOWN_COLLISIONS.values())
    # Every number here is an assertion — see the module docstring on how the
    # PASSING message, not the exit code, was what exposed the coarse key.
    # `{len(exported)} exported` vs `{n_rows} row(s)` is the one to read: the
    # export covers every cargo row on every platform, the row count only the
    # shared ones, so exported < rows would mean the resolver's table is
    # narrower than what this gate checked.
    print(
        f"fixture groups: {len(shared)} shared platform(s), {n_groups} group(s), "
        f"{n_rows} row(s) — no artifact-name collisions; "
        f"{n_recorded} collision(s) recorded across "
        f"{len(KNOWN_COLLISIONS)} unshared platform(s); "
        f"{len(exported)} cargo row(s) exported to the resolver, "
        f"{len(by_root)} shared artifact root(s), all invertible."
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())

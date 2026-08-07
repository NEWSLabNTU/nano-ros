# RFC-0070 — Build caches: one root, one vocabulary, one derivation

**Status:** Draft (2026-08-06)
**Implements:** [phase-334](../roadmap/phase-334-build-cache-layout.md) W2
**Relates to:** [RFC-0065](0065-colcon-like-builder.md) — that RFC decides who
*owns* a user workspace's `build/`; this one decides what every build cache in
this repository is *called* and *where it lives*.
**Informed by:** phase-334 W1 (the sharing verdict) and
[phase-340](../roadmap/phase-340-build-artifact-reuse.md) (the identity
measurements).

## Problem

Build caches grew as suffix-named siblings of their sources. Three separate
spellings encode "which RMW" alone:

```
examples/native/rust/talker/target-zenoh/         # cargo, RMW suffix
examples/workspaces/c/build-workspace-fixtures/   # cmake, stage suffix
examples/workspaces/c/build-workspace-fixtures-freertos/   # + platform suffix
build/fixtures-cargo/<group>/                     # phase-226 group key
```

Counted 2026-08-06: **117 `target*` and 249 `build*` directories inside source
trees**, and **236 hardcoded path literals across 17 files** — `justfile`, five
`just/*.just` modules, six `scripts/build/*`, `examples/fixtures.toml`,
`check-fixtures-stale.sh`, and three Rust fixture resolvers under
`packages/testing/nros-tests/src/`.

That literal count is the actual problem. A path convention with 236 spellings
cannot be changed, and it cannot be *verified* — which is issue 0196's class
(build-side probes must watch what test-side gates watch) expressed as
directory names.

## Rule

### R1 — One root

Every build cache lives under `$NROS_BUILD_ROOT` (default `<repo>/build/`,
overridable so the whole tree can move to a faster or larger volume — the
generalisation of the jobs audit's NVMe relocation, which today only `zephyr`
honours via `NROS_ZEPHYR_BUILD_ROOT`).

**Nothing writes build output inside a source directory.** Not
`examples/**/target-*`, not `examples/**/build-*`, not a workspace dir.

```
$NROS_BUILD_ROOT/
  cargo/<profile>/<variant-sig>/     cargo target dirs
  cmake/<kind>/<coordinate>/         kind = example | workspace | fixture
  west/<leaf>-<rmw>/                 zephyr (already rooted via env)
  models/<bringup>/                  phase-330 W3/W7 artifacts
  tools/…                            zenohd, install prefixes
```

### R2 — One vocabulary

A cache directory is named `<kind>/<coordinate>`, where the coordinate uses the
**fixture-manifest vocabulary already in use** — platform, lang, rmw,
feature-sig — and nothing else. `target-<rmw>`, `build-<rmw>`,
`build-workspace-fixtures[-<plat>]` all become derivations of that one scheme.

**A new ad-hoc suffix is a bug**, not a naming choice. The suffix zoo exists
because each new need invented a spelling instead of extending the coordinate.

### R3 — One derivation, consumed by all three sides

The path is computed by ONE function. The build, the staleness gate and the test
resolver call it — they never spell a literal. This is the #393 rule
(build/gate/run derive from one computation) applied to paths.

`scripts/build/fixtures-target-dir.sh` is the working precedent: it already
groups rows by platform, triple, profile, features, env and sync-mode, and both
`fixtures-build.sh` and `rust-fixture-stale.sh` call it *specifically* so the
probe inspects the tree the build wrote. Generalise that function; do not add a
second one.

### R4 — Sharing is a property of the root, not of a build

A shared cache directory is only ever driven by **one** cargo invocation at a
time. Cargo takes an exclusive lock per target dir, so N concurrent invocations
against one directory serialise — measured in phase-334 W1.a/W1.c as a net loss
against sccache, which already deduplicates the compilations.

Where sharing is wanted, it comes from **one invocation over many packages**
(cargo's internal jobserver parallelism), or from bounded worker concurrency —
never from pointing concurrent workers at a common directory.

## Consequences

**Deliberately enabled: corrosion's cargo trees become shareable.** phase-334
W1.c measured 32.6 GiB across 21 corrosion dirs with a 9:1 identity duplication
at the bottom of the stack, and found corrosion's own anti-collision hash is
*constant* for the shared nano-ros crates (`nano-ros_0b88c` in all nine
workspaces) — today's separation comes entirely from each workspace's
`CMAKE_BINARY_DIR`. Giving those trees one addressable root under R1 is what
makes the ~25 % disk saving reachable. R4 governs how.

**`.gitignore` collapses.** Per-directory ignore sprawl is replaced by
`build/`. The transition needs both sets until the last writer migrates.

**The mtime treadmill notes in CLAUDE.md name today's paths** and must move in
the same change as the family they describe.

**Out-of-tree consumers are unaffected.** This governs *this repository's*
caches. A user workspace's build root is RFC-0065's subject; the two agree that
`build/` is the root, and this RFC does not reach into a consumer's tree.

## Migration

Non-negotiable ordering, because the 236 literals are what makes this risky:

1. **Derivation first.** Extend the phase-226 resolver to cover every kind, with
   the current paths as its output. No directory moves. Behaviour identical.
2. **Callers second.** Replace literals with calls, one family at a time. Each
   family's build, staleness probe and test resolver move in ONE commit — a
   family split across commits is a red sweep.
3. **Paths last.** Only once a family reads its path from the derivation does
   the derivation's output change. Then the family rebuilds once.
4. **Gate.** A check that fails on a `target-*` or `build-*` directory inside a
   source tree, and on a literal cache path in a script. Without it the zoo
   regrows; it regrew once already after phase-226 introduced the shared group.

Do not overlap a family's move with an in-flight rename elsewhere in that tree.

## Open

* Which kinds beyond cargo/cmake/west need a coordinate — `compile-check`,
  `install`, and the `tools/` prefixes are currently ad-hoc but stable.
* Whether `$NROS_BUILD_ROOT` should be per-profile at the top level rather than
  under `cargo/`; the models and west trees are profile-independent today.
* The gate in step 4 needs an allowlist for vendored trees that build in place
  (`third-party/`), which cannot follow R1.

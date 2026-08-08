# Phase 334 — Build-cache layout: one place, one naming rule, measured sharing

**Informs:** RFC-0065 (the `nros build` builder owns the workspace build root)
**Informed by:** the 2026-08-03 jobs audit (fifo pools, NVMe relocation, the
sccache install) and phase-330 W3/W7 (models into the build dir).

**Status (2026-08-06). ALL of W1 is ANSWERED** — W1.a/W1.b/W1.d by phase-340,
which re-derived those questions before noticing this doc had already framed
them, and W1.c measured here. **W2 is open and is now the critical path**: it is
both the remaining value in this phase and the precondition for W1.c's win.
W3.a is decided.

W1 asked three things and phase-340 measured all three. The overlap is real and
was not deliberate: phase-340 W2's "findings" F1/F2/F3 restate W1.d, W1.a and
W1.b respectively. **This doc framed the questions first; phase-340 supplied the
numbers.** Neither should be read as independent confirmation of the other.

| W1 item | verdict | evidence |
| --- | --- | --- |
| **W1.a** cargo sharing vs per-example dirs | **sccache wins; do not share a dir across concurrent invocations** | phase-340 W1 lane A/B |
| **W1.b** feature-unification hazard / signature count | **bimodal — see below** | measured 2026-08-06 |
| **W1.d** sccache as the alternative | **prefers separate dirs + sccache**, comfortably inside W1.d's own ~15 % rule | 17846 hits / 222 misses warm |
| **W1.c** cmake / corrosion sharing | **sharing is safe and worth ~25 % disk, but W2 is its precondition** | measured 2026-08-06, below |

**W1.b answered, and the shape matters more than the count.** Over the 117
`linux` fixture rows there are **60 distinct variant signatures** — about half
the row count, which W1.b's own rule would read as "sharing buys little". The
distribution says otherwise:

```
 37 rows   (default features)
 10 rows   --no-default-features --features rmw-zenoh
  8 rows   --no-default-features --features rmw-xrce
  5 rows   --no-default-features --features rmw-cyclonedds
  2 rows   --features link-tls
 ---
 62 rows in 5 signatures   |   55 rows in 55 singleton signatures
```

**55 of the 60 signatures are singletons** and can never share with anything —
sccache is their only dedup. But **five signatures cover 53 % of the rows**, and
those are worth one shared build each. So the answer is neither "share
everything" nor "sccache only": share the head, cache the tail. W1.b's
either/or framing was too coarse, and phase-340 W2.b should target these five
groups specifically rather than "same-identity groups" in general.

**W3.a is therefore decided:** keep separate dirs under the new root and rely on
sccache for the tail; where sharing happens it must come from ONE cargo
invocation over many packages (inner parallelism), never N invocations against
one dir (lock contention). Rationale and the rejected design are recorded in
phase-340 W2 F3.

**W1.c answered 2026-08-06 — see below. What remains:** all of W2 (the layout
and naming rule — the part phase-340 does NOT cover) and W3.b/W3.c.

### W1.c result — corrosion's cargo trees

**The duplication is real and it is 9-fold.** The cmake workspace builds carry
**32.6 GiB across 21 corrosion cargo dirs**. Within the nine native
`build-workspace-fixtures/cargo/` trees the identity spread falls up the stack,
exactly as R1 predicts:

| crate | copies | distinct identities | ratio |
| --- | --- | --- | --- |
| `nros_core` | 36 | 4 | **9:1** |
| `nros_rmw_zenoh` | 18 | 4 | 4.5:1 |
| `nros_node` | 36 | 16 | 2.25:1 |

One identity (`b2744896132993cc`) has nine copies in nine DISTINCT workspaces —
`c`, `cpp`, `features`, `managed`, `mixed`, `realtime-c`, `realtime-cpp`,
`realtime-cpp-subnode-portable`, `safety`. Genuinely interchangeable.

**Sharing is mechanically trivial, because corrosion's separation is not doing
the work here.** Corrosion picks its dir as
`${CMAKE_BINARY_DIR}/<build>/cargo/<folder>_<sha1(workspace_manifest_path)[0:5]>`,
and the comment above that code says the hash exists so that "if you build
multiple workspaces … they won't collide if they use a common dependency. This
would confuse cargo and trigger unnecessary rebuilds". But for the shared
nano-ros crates that hash is **constant — `nano-ros_0b88c` in all nine** — since
they all import the same manifest path. Today's separation therefore comes
entirely from each workspace's own `CMAKE_BINARY_DIR`, not from corrosion's
anti-collision scheme. Redirect the root and the nine land in one place with no
renaming and no collision. (Per-workspace synthesized crates DO get distinct
hashes — `nros_ws_runtime_14eac`, `_ef4c9` — and are correctly separated.)

**Corrosion's warning is directionally right and quantitatively wrong.** Measured
directly on `nros-c` by alternating two feature sets in ONE target dir:

```
A (rmw-zenoh) cold        149 units built
B (rmw-xrce)  same dir     10 units built   <- 139 of 149 REUSED
A again                     9 units          <- steady-state churn
B again                     9 units
```

So alternating feature sets does churn — but **9 units against 139 reused, ~6 %
of the build**. Different feature sets do not fight over one slot: a different
`-C metadata` is a different filename, so the variants coexist in `deps/` and
only the genuinely feature-dependent units rebuild.

Disk, same two variants:

| | MiB |
| --- | --- |
| separate dirs (802 + 725) | 1527 |
| one shared dir | **1143** |
| saving | 384 (**25 %**) |

**Verdict.** Sharing corrosion's cargo root across workspaces is safe (no
pathological thrash), collapses a measured 9:1 duplication at the bottom of the
stack, and saves ~25 % disk on a two-variant pair. The cost is the same one W1.a
found: `workspace-fixtures-build.sh` schedules **one make target per workspace
dir in parallel**, so a shared dir serialises those group workers on cargo's
exclusive flock.

That makes W1.c's answer **conditional, and W2 is the precondition**: the win is
available only once the layout gives these trees a shared, addressable root.
Adopt it as part of W2's migration rather than as a standalone change, and pair
it with either a bounded group-worker concurrency or a single-invocation driver —
never N concurrent cmake workspaces against one cargo dir.

## Problem

Build caches grew organically as suffix-named siblings of their sources, and
the tree now carries a zoo with no single rule:

| Family | Where | Named by | Examples |
| --- | --- | --- | --- |
| Per-example cargo target dirs | inside each example dir | RMW / role suffix | `target-zenoh` (31 rows), `target-fixtures` (26), `target-xrce` (7), `target-cyclonedds`, `target-safety` |
| Per-workspace cmake dirs | inside each workspace | stage + platform suffix | `build-workspace-fixtures` (47), `build-workspace-codegen` (60), `-freertos`/`-nuttx`/`-nuttx-riscv` variants |
| Per-example cmake dirs | inside each example | RMW suffix | `build-zenoh`, `build-cyclonedds`, `build-xrce` |
| Zephyr west dirs | `zephyr-workspace/` (now `$NROS_ZEPHYR_BUILD_ROOT`) | leaf + rmw | `build-rs-service-server-zenoh`, … (~56) |
| Shared fixture cargo groups | `build/fixtures-cargo/<group>` | phase-226 group | qemu-arm-baremetal, stm32f4 |
| Repo-level | `build/` | tool/stage | `build/zenohd`, `build/compile-check`, `build/west-fixtures`, `build/install`, … |

The split exists FOR parallelism and variant isolation (disjoint dirs are what
made the phase-334-era fan-outs safe), but the cost is real: every separated
cargo dir rebuilds the same nros dependency stack from scratch. The zephyr
family alone compiles ~97k TUs on a cold tree (sccache measured), most of them
identical `nros-core`/`nros-node`/`nros-rmw-zenoh` builds repeated per leaf.
Meanwhile source trees are polluted with build output (the `legacy_files`
walker needed prefix-pruning just to survive it), and per-dir naming is
convention-by-accretion — three different spellings encode "which RMW" alone.

## Direction

One build root, structured; separation only where a MEASURED conflict requires
it; names derived from one vocabulary.

```
build/                                  # the ONE root (RFC-0065's domain)
  cargo/<profile>/<variant-sig>/        # shared cargo target dirs, keyed by
                                        #   (target triple, feature-set hash)
  cmake/<kind>/<coordinate>/            # kind = example|workspace|fixture
  west/<leaf>-<rmw>/                    # zephyr (already rooted via env)
  models/<bringup>/                     # phase-330 W3/W7 artifacts
  tools/…                               # zenohd, install prefixes (as today)
```

with `NROS_BUILD_ROOT` (default `<repo>/build/`, `.env`-overridable — the NVMe
relocation from the jobs audit generalizes to everything, not just zephyr).

## Work items

### W1 — Measure the sharing tradeoff before moving anything

- [x] **W1.a (cargo).** ANSWERED by phase-340 W1 — see Status. Cargo parallelizes WITHIN a build and locks the whole
      target dir per invocation. Measure, for the native example set and one
      QEMU family: (a) today's per-example `target-<rmw>` dirs, cold + warm;
      (b) one shared target dir per (triple, feature-sig) with the SAME
      concurrency delivered by the fifo pool. Record wall-clock, disk, and
      the serialization cost of cargo's target-dir lock under the pool (the
      phase-226 `fixtures-cargo/<group>` sharing is the existing prior —
      report its measured numbers first).
- [x] **W1.b (feature unification hazard).** ANSWERED 2026-08-06 — see Status. Shared cargo dirs are only
      correct per feature-set: quantify how many distinct `nros` feature
      signatures the fixture manifest actually produces (the `variant-sig`
      key). If the count approaches the example count, sharing buys little
      and W2 should default to sccache-only dedup.
- [x] **W1.c (cmake).** ANSWERED 2026-08-06 — see Status. CMake build dirs cannot share objects, but their
      corrosion-embedded cargo trees CAN share a `CARGO_TARGET_DIR` and all
      of them share sccache. Measure a workspace family with (a) today's
      layout, (b) corrosion cargo redirected to the shared cargo root.
- [x] **W1.d (sccache as the alternative).** ANSWERED by phase-340 W1 — see Status. With sccache now provisioned
      (`nros setup --tool sccache`, vendored-openssl recipe), re-measure the
      cold/warm zephyr + native families. If cache-hit builds get within ~15%
      of shared-dir builds, PREFER separate dirs + sccache (no lock
      contention, no unification hazard) and let W2 be layout-only.

### W2 — The layout + naming rule

- [x] **W2.a** LANDED 2026-08-06 as **[RFC-0070](../design/0070-build-cache-layout.md)**
      (a new RFC — 0065 stays builder-scoped, and this rule governs every cache in
      THIS repo, not a user workspace's build root).
      Write the rule into RFC-0065 (or a new RFC if 0065 stays
      builder-scoped): every build cache lives under `NROS_BUILD_ROOT`;
      NOTHING under `examples/**/src` or a workspace/source dir; names are
      `<kind>/<coordinate>` where coordinate reuses the fixture-manifest
      vocabulary (platform, lang, rmw, feature-sig) — never a new ad-hoc
      suffix. `target-<rmw>`, `build-<rmw>`, `build-workspace-fixtures[-<plat>]`
      all become derivations of the one scheme.
- [x] **W2.b** **STEP 2 COMPLETE 2026-08-08** (step 1 landed 2026-08-06):
      `scripts/build/build-root.sh` (`nros_build_root` / `nros_build_dir`) is the
      derivation, `fixtures-target-dir.sh` is its first caller, and
      `check-build-root` (in `check-fast`) asserts the emitted path is UNCHANGED.
      `NROS_BUILD_ROOT` now relocates that family.
      **Step 2 (CALLERS) — first three families landed 2026-08-07**, each with
      its build + staleness probe + test resolver in ONE commit:
      * **compile-check / cmake-fixtures** — `compile-check-fixtures.sh` (build),
        `scripts/test/compile-check-stale.sh` (probe),
        `require_compile_check{,_bin}` / `require_cmake_fixture` +
        `build_failure_marker` (resolver). The probe comment "the same roots the
        resolvers use" is now a derivation rather than a promise.
      * **idf-fixtures / west-fixtures** — `idf-fixtures.sh` / `west-fixtures.sh`
        (build) + `require_idf_fixture` / `require_west_fixture` (resolver).
      * **fixtures-cargo** — closes the split step 1 left: the shell half moved
        then, the Rust `fixture_shared_target_dir` was still a literal, so
        `NROS_BUILD_ROOT` relocated the build but not the lookup. **Also fixes a
        step-1 regression**: `fixtures-target-dir.sh` sourced `build-root.sh`
        from INSIDE `nros_fixture_target_dir_flag`, and `fixtures-build.sh`
        ships that function to its make leaves with `export -f`. In a leaf
        `${BASH_SOURCE[0]}` is not a file path, the source resolved to
        `./build-root.sh`, and every eligible qemu-arm-baremetal row got
        ` --target-dir ` with an EMPTY value. Sourcing moved to file scope and
        the two helpers joined the `export -f` list; the test now exercises the
        leaf, which an in-process-only assertion could never have caught.
      The Rust half cannot source bash, so `nros_tests::build_root` /
      `build_dir` is the ONE mirror; both halves are pinned to the same expected
      strings (`build_root_derivation.sh` + `nros-tests` unit tests), and the
      shell fallback moved off `$PWD` onto this file's own checkout.
      **Finding — the 236 count did NOT move, and cannot in step 2 as framed.**
      All 236 are the in-SOURCE suffix zoo (`target-<rmw>`,
      `build-workspace-fixtures[-<plat>]`), 140 of them in `examples/fixtures.toml`
      as manifest DATA (`target_dir = "target-zenoh"`,
      `build_subdir = "build-workspace-fixtures"`) and the rest as the matching
      default-argument strings in `binaries/mod.rs`
      (`build_workspace_cmake_entry{,_in}`). Those paths are *source-relative by
      construction* — there is no root in them to derive, which is exactly the R1
      violation. `nros_build_dir` cannot emit them without changing them, so a
      step-2 pass over that class needs either a source-relative sibling
      derivation (a second function — do not add one casually) or to be folded
      into step 3, where the path changes anyway and the manifest column stops
      being authored. **DECIDED 2026-08-07: the manifest column is DELETED, not derived — and that
      work belongs to phase-340 W2.a, not to a second step-2 pass.**

      Measured over the 137 rows in `examples/fixtures.toml` that author a path:

      ```
      128  reproducible from (kind, platform, rmw)
        9  not — target-tls, target-safety, target-zero-copy, target-large-buf,
           build-workspace-fixtures-threadx
      ```

      and the 9 are FEATURE-variant dirs, derivable from the feature signature
      `_nros_fixture_variant_sig` already computes. So the column is not data a
      derivation would need to reproduce; it is data the coordinate already
      contains. **A source-relative sibling derivation would be a second
      function whose only job is to re-emit a redundant column — do not add
      one.**

      This also means the class is NOT a separate work item. phase-340 W2.a is
      already "extend the phase-226 resolver so a manifest-authored
      `--target-dir` names a GROUP rather than opting the row out" — the same
      137 rows, approached from the identity side. Deleting the column and
      widening the group key are one change, and doing them separately would put
      two path conventions in flight at once (the #393 hazard the work order
      already names).

      **Consequence for the work order:** item 4's remaining class merges into
      item 5. What stays here as step-2 work is only the ROOTED side listed
      below, whose bug is R3 (one spelling), not R1 (wrong place). The literals this
      step DID move are the already-rooted `build/<kind>` writers, whose bug was
      narrower: R1-shaped but not R3-derived, so `NROS_BUILD_ROOT` moved the
      build and left the probe and the resolver behind.
      Still to migrate on the rooted side: `fixtures-build.sh` /
      `workspace-fixtures-build.sh`, cmake configure sites,
      `check-fixtures-stale.sh` / `legacy_files` pruning, and the `tools/` kinds
      (`build/zenohd`, `build/xrce-agent`, `build/qemu*`). Known NOT moved and
      why: `just/qemu-baremetal.just`'s `FIXTURE_TARGET` (a parse-time
      `absolute_path()`, which cannot call a bash function without a `shell()`
      on every justfile parse) and `check-weak-symbols-image.sh`'s COVERAGE
      table (relative find-bases mixing source dirs with cache dirs).
      One `lane-coords`-style derivation, not 300 edited literals.

      **Step 2 finished 2026-08-08.** The remaining rooted callers migrated as
      four more one-commit families — `zenohd`, `xrce-agent`, `qemu` /
      `qemu-zenoh-pico`, and the workspace builder's make-scratch root — plus a
      tail of four single call sites belonging to no family (`qemu.rs`
      libzenohpico, `ros2.rs` rmw_zenoh_ws overlay, `zephyr.rs`
      zephyr-workspace-builds, `cargo.sh` sizes-probe). That tail is worth
      naming: single sites in no family are how a "one derivation" rule ends up
      with five unplanned exceptions.

      Completion is CHECKABLE, not asserted:
      `git grep 'repo_root/build/|project_root().join("build/'` over `scripts/`
      and `nros-tests/src/` returns nothing but the doc comment naming the
      pattern it replaced. That mattered here because this work item's earlier
      finding was a count that did NOT move when it looked like it should.

      Verified by running the things, not reading them: `just zenohd/xrce/qemu
      doctor` all resolve through the derivation, `check-build-root` passes,
      `workspace-fixtures-build.sh linux` is rc=0, and the suites that exec
      these binaries are green — qos 6/6, xrce 6/6, emulator 16/16.

      Two deliberate non-migrations, recorded at their sites: `QEMU_PREFIX`
      (parse-time `absolute_path()`; a bash call there needs a `shell()` on
      every justfile parse) and `check-weak-symbols-image.sh`'s COVERAGE table.
      `qemu.rs`'s private `project_root()` walk-up was DELETED rather than
      `#[allow]`ed — two spellings of "where is the repo" is the R3 drift this
      step removes.

      **Steps 3-4 are NOT pending here.** Per the work-order consequence above,
      the source-relative class merges into phase-340 W2 (item 5), where the
      path changes anyway and the manifest column stops being authored. Doing a
      path move here would put two path conventions in flight at once — the #393
      hazard.
- [ ] **W2.c** Gitignore collapses to `build/` (plus the transition set);
      delete the per-dir ignore sprawl as dirs migrate.
- [x] **W2.d** LANDED 2026-08-06 — AGENTS.md "Build-Cache Root (RFC-0070)".
      `.env`/`NROS_BUILD_ROOT` documented as the ONE relocation
      knob (book + AGENTS.md); the jobs-audit NVMe note updates to it.
      *(Book page still to write; AGENTS.md is the normative home.)*

### W3 — MOVED to phase-340 (2026-08-07)

W3 asked "apply the W1 verdict", and the verdict's application turned out to be
phase-340's subject, not this one's. Keeping both spellings would have produced
two designs for one mechanism — the drift RFC-0070 R3 exists to prevent, applied
to work items instead of paths.

| was | now |
| --- | --- |
| W3.a shared dirs keyed by (triple, feature-sig) | **phase-340 W2** — the same grouping, with the umbrella-invocation shape F3 established |
| W3.b corrosion cargo target redirection | **phase-340 W3** — the corrosion split, measured there as total (∅ overlap with cargo leaves) |
| W3.c re-run the phase-331 measurement pair | **phase-340 W7** — one re-measure covering both axes |

**This phase's charter is now exactly one axis: WHERE a cache lives and what it
is called.** phase-340 owns WHAT gets compiled and how often. The two meet at
one point — a grouped build needs a derived path to write to — which is why the
work order below runs this phase's W2.b BEFORE phase-340's grouping work.

## Work order (both phases, 2026-08-07)

Authoritative copy in
[phase-340](phase-340-build-artifact-reuse.md#work-order-both-phases); repeated
here so a reader of either doc sees the same sequence.

1. **340 W4 follow-up** — the identity gate counts the size probe's nested
   dirs, so it reds on a tree that merely built more. It is in `check-fast`.
2. **340 W5.b/c** — make the `nros` build-dep edge optional. Near-deletion now
   that #464 removed the fallback it served; also removes the probe dirs that
   inflate (1).
3. **340 W6 step 1** — Zephyr remap + `OUT_DIR` + `codegen-units`, together.
   Largest measured population, additive, serialises nothing.
4. **334 W2.b steps 2–4** — the derivation for source-relative cache data. The
   precondition for placing anything new.
5. **340 W2** (was also 334 W3.a) — umbrella invocation per identity group.
   IN PROGRESS 2026-08-08: the group key widened (an authored `--target-dir`
   names a group instead of opting the row out) and `check-fixture-groups`
   gates the preconditions. The umbrella shape itself is blocked — every
   example leaf is its own workspace root, so cargo rejects the umbrella
   outright; findings and the shape that does work are in phase-340 W2.b. **The
   manifest column this item absorbed from W2.b is therefore still authored**;
   its deletion is phase-340 W2.d.
6. **340 W3** (was also 334 W3.b) — normalise the corrosion `--target` split.
7. **334 W2.c** — collapse `.gitignore` once (4) has moved the paths.
8. **340 W7** (was 334 W3.c) — re-measure both axes against phase-331's pair.

Rationale for the order: (1) is a live red in the fast tier; (2) shrinks (1)'s
population and is nearly free; (3) is the biggest win that needs no
restructuring; (4) unblocks every path move; (5) and (6) are the restructuring,
and must come after the derivation exists; (7) is cleanup; (8) proves it.

## Constraints

- Fixture identity: tests resolve artifacts by path; every path change goes
  through the fixtures-manifest/`lane-coords` derivation so the build, the
  staleness gate, and the test runner move together (the #393 rule).
- The mtime-treadmill practices in CLAUDE.md assume today's paths; update
  them in the same change that moves a family.
- Do not overlap phase-331's W2b/W3 renames mid-flight — sequence per-family
  moves after that phase's tree settles.

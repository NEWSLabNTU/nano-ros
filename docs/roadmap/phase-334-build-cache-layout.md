# Phase 334 — Build-cache layout: one place, one naming rule, measured sharing

**Informs:** RFC-0065 (the `nros build` builder owns the workspace build root)
**Informed by:** the 2026-08-03 jobs audit (fifo pools, NVMe relocation, the
sccache install) and phase-330 W3/W7 (models into the build dir).

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

- [ ] **W1.a (cargo).** Cargo parallelizes WITHIN a build and locks the whole
      target dir per invocation. Measure, for the native example set and one
      QEMU family: (a) today's per-example `target-<rmw>` dirs, cold + warm;
      (b) one shared target dir per (triple, feature-sig) with the SAME
      concurrency delivered by the fifo pool. Record wall-clock, disk, and
      the serialization cost of cargo's target-dir lock under the pool (the
      phase-226 `fixtures-cargo/<group>` sharing is the existing prior —
      report its measured numbers first).
- [ ] **W1.b (feature unification hazard).** Shared cargo dirs are only
      correct per feature-set: quantify how many distinct `nros` feature
      signatures the fixture manifest actually produces (the `variant-sig`
      key). If the count approaches the example count, sharing buys little
      and W2 should default to sccache-only dedup.
- [ ] **W1.c (cmake).** CMake build dirs cannot share objects, but their
      corrosion-embedded cargo trees CAN share a `CARGO_TARGET_DIR` and all
      of them share sccache. Measure a workspace family with (a) today's
      layout, (b) corrosion cargo redirected to the shared cargo root.
- [ ] **W1.d (sccache as the alternative).** With sccache now provisioned
      (`nros setup --tool sccache`, vendored-openssl recipe), re-measure the
      cold/warm zephyr + native families. If cache-hit builds get within ~15%
      of shared-dir builds, PREFER separate dirs + sccache (no lock
      contention, no unification hazard) and let W2 be layout-only.

### W2 — The layout + naming rule

- [ ] **W2.a** Write the rule into RFC-0065 (or a new RFC if 0065 stays
      builder-scoped): every build cache lives under `NROS_BUILD_ROOT`;
      NOTHING under `examples/**/src` or a workspace/source dir; names are
      `<kind>/<coordinate>` where coordinate reuses the fixture-manifest
      vocabulary (platform, lang, rmw, feature-sig) — never a new ad-hoc
      suffix. `target-<rmw>`, `build-<rmw>`, `build-workspace-fixtures[-<plat>]`
      all become derivations of the one scheme.
- [ ] **W2.b** Migrate the writers: `fixtures-build.sh` /
      `workspace-fixtures-build.sh` / `fixtures-target-dir.sh` (the
      phase-226 group logic generalizes), the per-example `--target-dir`
      rows in `fixtures.toml`, cmake configure sites, and the freshness
      probes that hardcode today's paths (`rust-fixture-stale.sh`,
      inputsig stamps, `legacy_files` pruning). One `lane-coords`-style
      derivation, not 300 edited literals.
- [ ] **W2.c** Gitignore collapses to `build/` (plus the transition set);
      delete the per-dir ignore sprawl as dirs migrate.
- [ ] **W2.d** `.env`/`NROS_BUILD_ROOT` documented as the ONE relocation
      knob (book + AGENTS.md); the jobs-audit NVMe note updates to it.

### W3 — Apply the W1 verdict

- [ ] **W3.a** If sharing wins for cargo: shared dirs keyed by
      (triple, feature-sig) with the fifo pool bounding concurrency; cargo's
      own lock replaces per-dir isolation. If sccache wins: keep separate
      dirs under the new root and rely on the cache for dedup.
- [ ] **W3.b** Corrosion cargo target redirection for cmake workspaces per
      W1.c's verdict.
- [ ] **W3.c** Re-run the phase-331 W1/W5 measurement pair so the
      consolidation numbers stay comparable across the layout change.

## Constraints

- Fixture identity: tests resolve artifacts by path; every path change goes
  through the fixtures-manifest/`lane-coords` derivation so the build, the
  staleness gate, and the test runner move together (the #393 rule).
- The mtime-treadmill practices in CLAUDE.md assume today's paths; update
  them in the same change that moves a family.
- Do not overlap phase-331's W2b/W3 renames mid-flight — sequence per-family
  moves after that phase's tree settles.

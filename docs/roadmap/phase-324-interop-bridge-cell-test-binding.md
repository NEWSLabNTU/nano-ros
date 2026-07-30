# Phase 324 — interop/bridge cells bind to the tests that run them

**Status (2026-07-31): DRAFT — not started.**
**Closes:** issue 0352. **Informed by:** issues 0341 (defect 3), 0327 (defect 2),
0196. **Extends:** [RFC-0051](../design/0051-test-matrix-architecture.md) (the test-matrix
SSoT) and the tier work in [phase-318](phase-318-fixture-freshness-and-tiers.md) /
[phase-319](phase-319-compile-check-lane-presence-to-truth.md) — those sharpen the
BUILD side (freshness, presence→truth, `ci_lane` tiers derived from
`matrix::CELLS`); this one closes the last hole on the TEST side: an interop/bridge
cell whose declared `(lang, rmw)` disagrees with the fixture its test actually
builds, with nothing to catch it.

## Goal

`matrix_fixture_coverage.rs` proves every baked `Tier::Runtime` cell has a
`fixtures.toml` row at its `(platform, lang, rmw)` coordinate. It cannot do the
same for `Kind::Interop | Kind::Bridge` cells and **exempts them**:

```rust
|| matches!(c.kind, Kind::Interop | Kind::Bridge)   // exempt: ros2 peers / bridge harness
```

The exemption is real: an interop/bridge peer is an ephemeral process (a `ros2`
node in a `DockerRosEnv`, a micro-XRCE Agent, an `rmw_zenohd` router, a
declarative bridge), and its nano side is built **outside** `fixtures.toml` — the
`(ZephyrNativeSim, Rust, Zenoh, Qos, Interop)` nano entry by the west leaves lane
(`build_zephyr_workspace_rust_qos_entry`), the native `ros_editions` nano nodes by
`just ros_editions build-e2e-fixtures` into `target-ros-edition-*`. No fixture row
→ no coordinate → no check. So a cell can declare `(ZephyrNativeSim, Cpp,
Cyclonedds, Qos)` while the only test of that shape boots the **Rust / Zenoh**
image, and nothing fails (issue 0341 defect 2 — corrected by hand, class open).

The design principle (settled in the issue-0352 dialogue):

> **Build and test are different concepts, and interop builds differently from
> baked fixtures on purpose. Do NOT unify the build methods. Instead: give
> interop/bridge their OWN cell list in the formulation their shape needs, and add
> ONE correspondence SSoT that binds each cell to the recipe that builds it and
> the recipe that runs it — gated, so drift is a gate failure, not a silent pass.**

So this phase does **not** introduce a `build_for(cell)` unifier and does **not**
move interop artifacts into `fixtures.toml`. It separates the two axes cleanly:

| concept | SSoT | this phase |
| --- | --- | --- |
| what is TESTED | `matrix::CELLS` (baked) + `interop::CELLS` (NEW) | split the two shapes |
| what is BUILT | `fixtures.toml` / west leaves / `build-e2e-fixtures` | unchanged methods |
| what RUNS | `just test-*` / `just ros_editions ci` / bridge tests | parametrized over the cells |
| CORRESPONDENCE | — (missing today) | a binding table + gates (NEW) |

## Target architecture

```
TEST INTENT ─ two lists, different formulations
  matrix::CELLS      baked/self-contained   Coord(platform,lang,rmw)+workload,kind,tier
  interop::CELLS     interop/bridge          nano Coord + peer + direction (+bridge endpoints)
        both impl `trait TestCell { id, tier, kind, binding() }`
                    │
BINDING SSoT ─ one row per runnable test
  { cell_id, build_recipe, artifact_ref, test_recipe }   (names the channel; does NOT build)
        │                                    │
BUILD ─ separate, unchanged                  TEST ─ separate, unchanged
  fixtures.toml (baked)                        just test / test-*
  west-leaves manifest (zephyr entries)        just ros_editions ci
  build-e2e-fixtures (native ex×edition×rmw)   declarative_bridge_*
  peers: ephemeral, DECLARED not built
```

`InteropCell` carries what a baked `Cell` cannot — a peer and a direction:

```rust
struct InteropCell {
    id:    &'static str,                                 // "zephyr-qos-rust-zenoh"
    nano:  Coord,                                        // (ZephyrNativeSim, Rust, Zenoh, Qos) — the BUILT side
    build: BuildChannel,                                 // ZephyrWestLeaves | RosEditionsE2E | NativeFixtures
    peer:  Peer,                                         // RosEdition{rmw} | XrceAgent | ZenohRouter | NanoBridge{ingress,egress}
    dir:   Dir,                                          // NanoToRos | RosToNano | BiDir
    kind:  Kind,                                         // Interop | Bridge
    tier:  Tier,
    test:  &'static str,                                 // the test recipe / binary that runs it
}
```

## W1 — extract the interop/bridge cells into their own list

`matrix::CELLS` keeps only baked/self-contained cells; interop/bridge move to a
new `interop::CELLS` with the `InteropCell` shape above. A shared
`trait TestCell` (id, tier, kind, `binding() -> Binding`) lets tooling iterate
both without caring which list a cell came from.

- [ ] **W1.a** New `packages/testing/nros-tests/src/interop.rs`: `InteropCell`,
      `BuildChannel`, `Peer`, `Dir`, and `interop::CELLS` seeded from the 8
      interop/bridge cells currently in `matrix.rs:700-719` (6 native interop, the
      2 zephyr QoS cells, the 2 bridge cells — read them off the current file so
      nothing is dropped).
- [ ] **W1.b** `trait TestCell` in a shared module; `impl TestCell for Cell` and
      `for InteropCell`. `matrix::CELLS` loses its `Kind::Interop | Kind::Bridge`
      rows; the matrix's own injectivity/uniqueness unit tests re-run green over
      the shrunk set.
- [ ] **W1.c** The corrected `(ZephyrNativeSim, Rust, Zenoh, Qos)` cell and its
      never-run `(Cpp, Cyclonedds)` CarveOut sibling (0341 defect 2) become
      `interop::CELLS` rows — the CarveOut expressed as a `tier: CarveOut(reason)`
      InteropCell, honest and expressible.

**Done when:** `matrix::CELLS` is baked-only, `interop::CELLS` enumerates every
interop/bridge shape, and the split is exhaustive (W3 gate proves it).

## W2 — the binding SSoT

`InteropCell::binding()` (and `Cell::binding()`) yield one `Binding { cell_id,
build_recipe, artifact_ref, test_recipe }`. The row NAMES its channel; it does not
build. For baked cells `artifact_ref` is the `fixtures.toml` id resolved by
coordinate (today's behavior, now explicit); for interop it is the west-leaves
entry or the `target-ros-edition-*` path, and the peer needs no build.

- [ ] **W2.a** `Binding` type + `build_recipe`/`test_recipe` as enums whose
      variants name real `just` recipes (checked by W3.b against the justfile, the
      way `PlatformId::just_module` already is).
- [ ] **W2.b** `artifact_ref` resolves per `BuildChannel`: `NativeFixtures` →
      `fixtures.toml` id; `ZephyrWestLeaves` → the leaves-lane artifact; `RosEditionsE2E`
      → `target-ros-edition-<edition>-<rmw>/…`. One resolver per channel, no
      cross-channel unification.

**Done when:** every `TestCell` yields a `Binding`, and the bindings render (a
`bindings` debug bin, sibling of `lane-coords`) for eyeballing.

## W3 — gates enforce the correspondence

Replace the blanket `Kind::Interop | Kind::Bridge` exemption in
`matrix_fixture_coverage.rs` with real checks over the two lists.

- [ ] **W3.a G1 test-coverage** — every `Tier::Runtime` cell across BOTH lists is
      bound in exactly one row. A cell nothing runs is caught; the interop
      exemption is gone because interop now has a declared binding of its own.
- [ ] **W3.b G2 build-coord match** — each binding's `build_recipe` exists in the
      justfile, and `artifact_ref`'s real `(lang, rmw)` equals the cell's `nano`
      coordinate. This is the check that kills defect 2: a `(Cpp, Cyclonedds)` cell
      pointing at a Rust/Zenoh artifact fails here.
- [ ] **G3 tier correspondence** — the tier/recipe that RUNS a binding's
      `test_recipe` also TRIGGERS its `build_recipe`, so a tier cannot test what it
      never built (the museum-binary / mtime-treadmill class; cross-ref
      [phase-319](phase-319-compile-check-lane-presence-to-truth.md)'s
      presence→truth). Reuse phase-319 mechanisms rather than inventing.
- [ ] **G4 peer-decl** — every interop `peer` names a real ephemeral provider
      (`RosEdition`/`XrceAgent`/`ZenohRouter`/`NanoBridge`); no undeclared peer.

**Done when:** the four gates pass over the seeded lists, and reverting the 0341
defect-2 fix (a cell lying about its lang/rmw) makes G2 RED — the regression the
whole phase exists to prevent.

## W4 — the interop/bridge tests consume `interop::CELLS`

The tests stop hand-picking their nano builder and peer; they parametrize over
`interop::CELLS`, so the cell's fields DRIVE the fixture built and the peer
spawned. This is the #327-defect-2-shaped move, now for the interop axis.

- [ ] **W4.a** `qos_zephyr_ros2_interop_e2e` iterates the zephyr QoS interop
      cells; the cell's `build` selects the west-leaves entry (no more literal
      `build_zephyr_workspace_rust_qos_entry`), the `peer` selects the router.
- [ ] **W4.b** `ros_editions_e2e` replaces its local `enum Workload`/`enum Dir`
      copies with the `interop::CELLS` native rows (the #327.2 collapse already
      made it one rstest — this rebinds its cases to the SSoT).
- [ ] **W4.c** `interop_e2e` (native pub/sub/service/lifecycle) and
      `declarative_bridge_*` (Bridge cells, two-endpoint peer) parametrize the same
      way.
- [ ] **W4.d** Each rewritten test asserts, at runtime, that the fixture it
      resolved matches the cell it was handed — belt to G2's braces, so a
      resolver bug surfaces as a test failure and not a wrong-image pass.

**Done when:** no interop/bridge test names a fixture builder or peer literally —
each derives both from its `interop::CELLS` row.

## W5 — the tier machinery sees both lists

`ci_lane::{cells,coords}` today pools `runtime_cells()` from `matrix::CELLS` only.
The interop/bridge cells that run in `just ci` (native `interop_e2e`,
`declarative_bridge`) must stay selected after W1 moves them out, or tier 1
silently drops them.

- [ ] **W5.a** `ci_lane` pools over `TestCell` (both lists), not `matrix::CELLS`
      alone. Verify the tier-1/2/2n cell counts and `coords` are unchanged for the
      baked cells and now also cover the interop cells they always should have —
      diff `just` `lane-coords` output before/after and record it in the commit.
- [ ] **W5.b** Interop cells that DON'T run in any CI tier (the ros_editions
      docker lanes, gated out of `just ci`) are marked so `ci_lane` does not select
      them — a `runs_in_ci: bool` (or a tier variant) on the InteropCell, checked
      by a gate so "in the list but never in a tier" is explicit, not accidental.

**Done when:** `just ci` (tier 1) selects exactly the interop cells it ran before
this phase, no more and no fewer, and the delta is recorded.

## W6 — docs + pointers

- [ ] **W6.a** ARCHITECTURE §2 + `docs/design/0051-*.md`: document the two-list
      split (baked vs interop/bridge) and the binding SSoT as the correspondence
      layer.
- [ ] **W6.b** `examples/README.md`: note that interop/bridge coverage is declared
      in `interop::CELLS`, not `fixtures.toml`.
- [ ] **W6.c** CLAUDE.md one-liner under the matrix pitfall: "interop/bridge cells
      live in `interop::CELLS` with their own (nano + peer + dir) shape; the
      binding table + G1–G4 gate the build↔test correspondence — do not add an
      interop test that hand-picks a fixture."

## Non-goals

- No `build_for(cell)` common builder; the three build channels stay separate.
- No move of interop artifacts into `fixtures.toml`.
- No change to how peers are spawned — only how a test SELECTS its peer.

## Acceptance

- `matrix::CELLS` baked-only; `interop::CELLS` enumerates every interop/bridge shape.
- G1–G4 green; reverting 0341 defect 2 turns G2 RED.
- No interop/bridge test names a fixture builder or peer literally.
- `just ci` tier-1 interop selection unchanged (W5), delta recorded.
- Issue 0352 closed; ARCHITECTURE §2 / RFC-0051 / CLAUDE.md updated.

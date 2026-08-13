# Phase 350 — West fixtures join the manifest: one SSoT, one vocabulary, one coordinate

**Status (2026-08-13). DRAFT — no work item started.**

**Implements:** [RFC-0051](../design/0051-test-matrix-architecture.md) (the fixture half),
[RFC-0070](../design/0070-build-cache-layout.md) (the naming rule it never
scoped).
**Informed by:** the 2026-08-13 fixture/test audit (this doc's Inventory),
[#509](../issues/0509-zephyr-lane-per-leaf-overhead.md)'s measurement,
and phase-340 W3's coordinate predicate.
**Files:** [#535](../issues/0535-west-fixtures-have-no-coordinate.md),
[#536](../issues/0536-configure-only-west-fixtures-pay-for-a-kernel-link.md),
[#537](../issues/0537-fvp-artifacts-built-with-no-runner.md),
[#538](../issues/0538-fixture-inventory-is-a-stale-second-answer.md),
[#539](../issues/0539-fixture-naming-vocabulary-drift.md),
[#540](../issues/0540-int32-observer-orphan-bin.md).

## Problem

`examples/fixtures.toml` is called the fixture SSoT and holds 379 rows. **74
west-built fixtures are not in it**, and their matrices live in two bash scripts
plus a third hardcoded copy in a Rust gate. A fixture with no row has no
`row_coord()`, and phase-340 W3 made that coordinate the ONE predicate both
halves of a lane read:

```
BUILD  skips row R  ⟺  row_coord(R) ∉ lane_coords
RUN    skips row R  ⟺  row_coord(R) ∉ lane_coords
```

So for 16 % of the fixture surface, neither half can select anything. That is
not a disagreement between two computations (issue 0482's shape) — it is an
absence, which nothing can report.

## Inventory (measured 2026-08-13)

**In the SSoT.** 250 `[[fixture]]` + 93 `[[workspace_fixture]]` +
36 `[[compile_check_fixture]]` = 379 rows over 126 fixture groups.

**Outside it, consumed by tests:**

| set | count | build declared in | consumers |
| --- | --- | --- | --- |
| zephyr west leaves | **70** | `fixture-matrix.sh` + `zephyr-fixture-leaves.sh` | `zephyr.rs`, `qos_zephyr_*`, workspace e2es |
| `build/west-fixtures/<id>` | 4 | `west-fixtures.sh` bash arrays | `board_import`, `cli_bringup_zephyr`, `zephyr_self_pkg` |
| FVP | 4 | `just/zephyr-setup.just` only | `fvp_smoke`, `fvp_runtime_ws`, **2 with no runner** |
| `target-zenoh-fixture-posix` | 1 | root recipe, literal `--target-dir` | `zenoh_archive_symbols`, `zenoh_header_parity` |
| esp32 flash `.bin` | 2 | espflash postprocess, literal both sides | `esp32_emulator.rs:74,119` |
| `tests/esp-idf-smoke` | 1 | `idf-fixtures.sh` | `cli_bringup_esp_idf` |
| ros-editions bins | 7 | `just ros_editions build-{fixture,e2e-fixtures}` | `ros_editions_*` |
| `bins/int32-observer` | 1 | **nothing** | **nothing** |

Zephyr leaf shape: 3 langs × 6 roles × 3 rmws = 54, + 12 `ws-*-entry`,
+ 3 mps2 talkers, + 1 logging-smoke = 70. Reproduce:

```sh
bash scripts/build/zephyr-fixture-leaves.sh --emit records \
  --include-logging-smoke --include-workspace-entry | wc -l
```

## What the cost actually is — correcting the obvious read

The zephyr workspace is **215 GB across 75 build dirs** (mean 2.8 GB, max
6.6 GB for `build-ws-mixed-entry-zenoh`). The natural inference — "70 leaves
means 70 kernel builds" — is **wrong**, and #509 measured why:

| signal | value | source |
| --- | --- | --- |
| lane wall-clock | 40 min for 68 leaves | #509 |
| ninja edges, ALL leaves | 1254 (mean **18**/leaf) | #509 |
| leaves that re-ran CMake | **8 of 69** | #509 |
| sccache | 96.8 % hit | #509 |
| workspace on disk | 215 GB / 75 dirs | this audit |

The leaves are not recompiling the kernel; sccache and CMake reuse are working.
~140 s per leaf buys 18 edges, and the cost is fixed per-leaf overhead — west +
cmake startup, `nros sync` prep, signature computation, and a cargo fingerprint
pass that runs to completion to learn there is nothing to do.

**This changes the fix.** Sharing one kernel build across leaves would reclaim
most of 215 GB and roughly none of the 40 minutes. The 40 minutes come down by
paying the per-leaf tax fewer times — fewer leaves, or leaves that skip on a
coordinate — and every one of those levers needs the coordinate this phase is
about. #509's last direction says exactly this and cannot be acted on today:

> Question whether all 68 leaves must be in `lane=all` at their current
> granularity, or whether the coordinate cover (phase-340 W3) can retire some.

## Non-goals

* **Folding test FILES.** phase-329 ran that campaign to completion and archived
  it; its `≤120` target was restated to a measured 151 because ~36 candidates
  proved genuine one-offs, W8's row-dedup was retracted as load-bearing, and
  phase-342 W1 measured a single-test fold costing 3.6× wall-clock plus the
  nextest filter vocabulary. Do not reopen it here.
* **Moving the build-cache root.** RFC-0070 / phase-334 W2.b settled that and is
  archived complete. This phase adds the leaf-NAME rule W2.b never scoped
  (#539), nothing about roots.
* **Deleting a Runtime cell to save a fixture.** A cell is a coverage claim;
  removing one is a separate decision with its own evidence (see W4).

---

## W0 — Zero-risk deletions

- [ ] Delete `packages/testing/nros-tests/bins/int32-observer/` (#540) — retired
      by issue 0128 T0, crate survived, no row / no builder / no consumer.
- [ ] Retire `scripts/build/fixture-inventory.py` (#538) or gate it. It claims
      to BE this phase's inventory, has no consumer, and 3 of its 5
      hand-authored rows have had manifest rows since phase-344 W2. Prefer
      deletion once W1 lands; gate it if W1 stalls.
- [ ] Fix the `TEST_DRIVEN_BUILDERS` entry in `examples_fixture_coverage.rs` to
      match whatever W3 decides for the two runnerless FVP artifacts.

*Acceptance:* `just ci` green; `grep -rn int32.observer` returns only archived
docs.

## W1 — The 74 get rows (#535)

The manifest already models non-cargo builders (`cmake`, `cmake-configure`,
`cross-build`, `cxx-syntax`), so this is a `builder = "west"` row, not a new
concept.

- [ ] `[[fixture]]` rows for the 70 zephyr leaves and the 4 west fixtures,
      carrying `(platform, lang, rmw)` so `row_coord()` answers.
- [ ] `zephyr-fixture-leaves.sh` and `west-fixtures.sh` CONSUME the manifest
      (`--coords-from`), as `fixtures-build.sh` and
      `workspace-fixtures-build.sh` already do. Delete
      `nros_fixture_langs`/`nros_fixture_roles` and the two bash arrays.
- [ ] `examples_fixture_coverage.rs` reads the rows instead of restating the
      role matrix in `ZEPHYR_LANGS` × `ZEPHYR_ROLES` — **three spellings of one
      matrix collapse to one**, or this is the sizes-header mirror again.
- [ ] `row_artifact_root()` answers for a west leaf, so the staleness probe can
      attribute it instead of exempting it wholesale.

*Acceptance:* `NROS_FIXTURE_COORDS` is read by the zephyr lane;
`build-test-fixtures lane=tier1` builds strictly fewer than 70 zephyr leaves and
the tier-1 run is still green; `just fixture-staleness` reports a coordinate for
every west leaf.

**Land W1 before W2/W4.** Both need a row to attach a decision to.

## W2 — Configure-only fixtures stop paying for a link (#536)

Three of four west fixtures assert a configure-time fact:

| fixture | asserts | needs ELF |
| --- | --- | --- |
| `west_board_import` | `CMakeCache.txt` ×4 | no |
| `zephyr_self_pkg_rust` | `system_config.h` | no |
| `zephyr_self_pkg_sibling` | `system_config.h` | no |
| `west_bringup_zephyr` | bake + boots `zephyr.exe` | **yes** |

The self-pkg pair runs a link `west-fixtures.sh:112` already calls "doomed", then
stamps on a file written before the link began.

- [ ] Give the three `builder = "west-configure"` and stop at configure.
- [ ] The stamp must DISTINGUISH configure-only from configure+link, so a
      build-only lane cannot read as covered — that failure mode is #537.

*Acceptance:* the three produce no ELF and their tests pass unchanged; per-leaf
wall-clock for them measured before/after and recorded here.

## W3 — FVP: close it or retire it (#537)

`build-fvp-aemv8r-cyclonedds` and `-rust` build `examples/zephyr/{cpp,rust}/talker-aemv8r`;
their runners were deleted by phase-298 W4 (`68a0a0b6f`). The `run-` recipes
survive, so the justfile still reads complete.

- [ ] Decide per artifact under [phase-217](phase-217-arm-fvp-local-runtime.md)
      (**Status OPEN**, Track A only): restore a runner, or retire recipe and
      example together.
- [ ] All four FVP artifacts get rows with the gated-SDK condition as a row
      property, so "gated SDK absent" and "nobody built it" stop sharing one
      skip message.

*Acceptance:* no build recipe produces an artifact with no consumer; a
license-gated skip is distinguishable from an unbuilt fixture in the test output.

## W4 — Leaf-count triage, on evidence

Three candidate groups, from the audit. **Each is a measurement, not a
foregone deletion** — phase-329 W8 retracted its dedup precisely because
presumed-redundant rows were load-bearing.

**(a) Feature entry leaves — 4.** `ws-rs-{params,qos,lifecycle,safety}-entry`.
Each of those four workloads already has Linux cells in all three languages
(`matrix.rs:763-778`) AND a `ZephyrNativeSim` Rust cell (`:783-786`). The
Zephyr witness is a real claim ("the feature works on an RTOS"), so the move is
**consolidation, not deletion**: one multi-feature Zephyr entry image in place
of four, 4 leaves → 1, coverage preserved.

**(b) Realtime entry leaves — 3.** `ws-{c,cpp,rs}-realtime-entry`. **Keep.**
phase-296 W5.5 made Zephyr honor sched dims natively (`k_thread_deadline_set`);
these are platform behavior, not feature duplication.

**(c) The 54-leaf role × lang × rmw block.** Once W1 lands, ask #509's question
against real coordinates: which of these does the tier-2 1-wise cover actually
select, and what does `lane=all` need that no lane reads?

*Acceptance:* a before/after leaf count and lane wall-clock, measured on a
cleanly rebuilt tree (museum binaries make this number a lie — CLAUDE.md
"fixture mtime treadmill"). Any leaf removed names the cell that still covers
its claim.

## W5 — One vocabulary (#539)

- [ ] Lang axis: `rust`, one spelling. Delete `nros_zephyr_lang_tag` or derive
      it from `matrix::Lang`.
- [ ] State the `build/<kind>` rule in RFC-0070 and rename the five outliers
      (`fixtures-cargo`, `compile-check`, `zephyr-fixture-build` vs
      `-make-driver`, `borrowed-e`, `px`).
- [ ] Zephyr build-dir names derive from the row coordinate (available after W1),
      so the name has a producer instead of a convention.
- [ ] Gate against phase/issue-coded fixture ids — `n9_form1`, `o4_pkg_index`,
      `l9_register_c`, `build-245-asan` — the rule test names already carry.

**Sequence inside W1's cutover, not after it.** The join W1 performs is
`(platform, lang, rmw)` against `<lang>-<role>-<rmw>` path segments, with
`rust`/`rs` disagreeing; renaming afterwards means touching every path twice.

## W6 — Close the class

Every gap above was invisible because the one coverage gate walks `examples/**`
for `package.xml` and nothing else.

- [ ] Extend it (or add its sibling) so **every** test-consumed artifact root is
      a manifest row or a tracked exception with a reason: the
      `packages/testing/nros-tests/bins/` crates (which is how #540 hid), the
      `build/<kind>` fixture trees, `target-zenoh-fixture-posix`, the esp32
      `.bin` postprocess, the ros-editions tree.
- [ ] Legitimate exceptions stay exceptions — the zenoh symbol fixture's literal
      path is deliberate (phase-336 allow-list) and ros-editions is a separate
      axis by RFC-0058 — but each is DECLARED, and a dead exception fails the
      gate the way `examples_fixture_coverage.rs`'s stale-exception arm does.

*Acceptance:* adding a fixture outside the manifest fails a gate, in CI, with a
message naming the manifest.

---

## Sequencing

```
W0 ──▶ W1 ──┬──▶ W2
            ├──▶ W3
            ├──▶ W4  (needs coordinates to triage against)
            └──▶ W5  (inside W1's cutover)
                  └──▶ W6
```

W0 is independent and can land immediately. Everything else is downstream of
W1, because a fixture with no row has nothing to attach a decision to.

## Acceptance (phase)

- Zero west-built fixtures outside `examples/fixtures.toml`; `row_coord()`
  answers for all 453.
- The zephyr lane honors `NROS_FIXTURE_COORDS`, and `lane=tier1` builds strictly
  fewer zephyr leaves than `lane=all` with tier 1 still green.
- One spelling of the lang axis, one `build/<kind>` rule, one zephyr build-dir
  scheme, no phase-coded fixture ids — each gated.
- No build recipe produces an artifact no test consumes.
- Lane wall-clock re-measured against #509's 40 min baseline on a cleanly
  rebuilt tree, with the leaf-count delta named.

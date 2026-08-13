# Phase 350 — West fixtures join the manifest: one SSoT, one vocabulary, one coordinate

**Status (2026-08-13). W0 and W1 COMPLETE — all 70 zephyr leaves are manifest rows, the emitter reads them (byte-identical), the lane narrows by coordinate (tier2: 7 leaves, was 70), the coverage gate reads rows, and W1.d is retracted with reasons. W2–W6 open; the 4 `west-fixtures.sh` fixtures still to row.**

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

*Correction (W1, 2026-08-13):* "70 with no manifest row" is 69 — one entry leaf,
`build-ws-rs-entry-zenoh`, has had a `[[workspace_fixture]]` row
(`workspace-rust-zephyr`) all along. The west lane ignores it and re-derives the
leaf anyway, so the defect class is unchanged; the count was not.

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

## W0 — Zero-risk deletions — **LANDED**

- [x] Delete `packages/testing/nros-tests/bins/int32-observer/` (#540) — retired
      by issue 0128 T0, crate survived, no row / no builder / no consumer.
- [x] **Gate** `scripts/build/fixture-inventory.py` (#538), not retire it, and
      delete its four stale rows.
- [x] Correct `packages/reference/README.md`, which said `fixture-inventory.py`
      *builds* `qemu-smoltcp-bridge`. It builds nothing — it is read-only.

### Why the inventory was gated rather than deleted

This item was drafted "prefer deletion once W1 lands", on the reading that W1
makes the file redundant by construction. That reading was incomplete: the file
is also where **`shared_mutation` hazards are declared**, and phase-339 W-item
treated that as a real obligation —

> `scripts/build/fixture-inventory.py`: the `nuttx-kernel-export-preflight` row
> declares `shared_mutation: …`. When the sharing is gone the declaration must
> go with it — **a stale `shared_mutation` is worse than none.**

Deleting the file would drop that model silently, which is the same failure the
phase-339 note is warning about. So: gate now (issue 0538's option 2), and let
W1 decide the home of `prerequisite_rows()` once the manifest can answer the
"outside the manifest" half on its own.

`--check` asserts each `hand-authored-*` row is genuinely absent from
`examples/fixtures.toml`, resolving `rmw` the way `row_coord()` does (absent
⇒ zenoh) so the two sides read a row identically — issue 0482 is what a second,
disagreeing resolution of that field costs. It is wired into
`check-fixtures-manifest`, so it runs in `just check`.

**Verified failing before trusted.** First run flagged 5; two were
`kind: postprocess` rows (espflash packing a `.bin` beside a manifest row's
cargo ELF), which SHARE their row's coordinate by construction and are exempt —
they assert "a step runs after this row", not "this build is outside the
manifest". Narrowed to `hand-authored-*`, it flagged the 4 real ones, which are
now deleted:

| row | why it was stale |
| --- | --- |
| `qemu-smoltcp-bridge` | row at `fixtures.toml:1669` |
| `native-rust-cyclonedds-talker` | row exists |
| `native-rust-cyclonedds-listener` | row exists |
| `threadx-riscv64-rust-talker-cyclonedds` | row added by phase-344 W2 **for this exact reason** |

*Correction to #538 as filed:* it says "3 of its 5 hand-authored rows". The list
holds **7** rows and **4** were stale (its own table already listed four). The
remaining true one is `esp-idf-smoke`; two are exempt postprocess rows.

**W0.c moved to W3.** The draft asked to fix the `TEST_DRIVEN_BUILDERS` entry in
`examples_fixture_coverage.rs` "to match whatever W3 decides" — which is
W3-blocked by construction, not zero-risk. The comment is accurate today (it
says nothing runs those two examples), so it stays until W3 decides.

*Acceptance, met:* `just check-fixtures-manifest` green with the new `--check`
leg; `grep -rn 'int32.observer\|int32_observer'` returns only archived docs and
this phase's own issue.

## W1 — The 74 get rows (#535) — **W1.a PARTIAL; W1.b/c/d open**

The manifest already models non-cargo builders (`cmake`, `cmake-configure`,
`cross-build`, `cxx-syntax`), so this is a `builder = "west"` row, not a new
concept.

- [x] **58 `[[fixture]]` rows** for the non-entry zephyr leaves, carrying
      `(platform, lang, rmw)` so `row_coord()` answers.
- [x] `builder = "west"` taught to `is_cargo_row()` / `row_artifact_root()` /
      `matches_filters()`, so the cargo/cmake lane and its probe cannot see a
      west row.
- [x] `check-zephyr-fixture-rows.py` — the rows and the emitter cannot drift
      while both exist.
- [x] **`zephyr-fixture-leaves.sh` CONSUMES the rows** (`fixtures-manifest.py
      west-leaves`). `nros_fixture_langs` / `nros_fixture_roles` /
      `nros_zephyr_lang_tag` deleted from `fixture-matrix.sh`; the mps2 and
      logging-smoke blocks are gone. **Proven byte-identical** in all four
      emitter modes.
- [x] **12 entry leaves** are `[[workspace_fixture]]` rows, emitted by the same
      loop. 573 more lines of copy-paste deleted; still byte-identical.
- [ ] **4 `west-fixtures.sh` fixtures** still bash arrays; that script still
      declares its own matrix.
- [x] **W1.c** — `examples_fixture_coverage.rs` reads the rows instead of
      restating the role matrix in `ZEPHYR_LANGS` × `ZEPHYR_ROLES`. **Three
      spellings of one matrix are now one.**
- [x] **W1.d — RETRACTED on inspection.** See below.

*Acceptance (unchanged, NOT yet met):* `NROS_FIXTURE_COORDS` is read by the
zephyr lane; `build-test-fixtures lane=tier1` builds strictly fewer than 70
zephyr leaves and the tier-1 run is still green; `just fixture-staleness`
reports a coordinate for every west leaf.

### Rows before consumer, and the gate that makes it safe

Landing rows nothing reads would ADD a spelling — the defect this phase is
about. So the rows ship with `scripts/check-zephyr-fixture-rows.py`, which
compares the emitter's leaf set against the west rows on
`(board, lang, role, rmw)` in both directions and fails on either a leaf with no
row or a row with no leaf. Verified red both ways before being wired in.

It normalises ONE axis: `zephyr-fixture-leaves.sh` gates cyclonedds on `idlc`
being present, so a host without it emits 36 role leaves where the manifest
always has 54. The gate restricts both sides to the RMWs this host emitted
rather than asserting raw equality (red for a reason the developer cannot act on
gets a gate disabled) — and rather than the host-safe-but-blind
`emitted ⊆ manifest`, which would let a deleted leaf keep its row forever. That
is how `fixture-inventory.py` rotted (#538).

### The rewire's oracle, and the two defects it caught

`zephyr-fixture-leaves.sh --emit records` runs no build tool, so the rewire has
a perfect oracle: capture the 70 records before, iterate the manifest instead of
the bash loops, and require the output to be **byte-identical**. It is, in all
four flag modes. Two real defects surfaced only because the comparison was
byte-exact rather than "looks right":

1. **`--include-logging-smoke` stopped gating its leaf.** Becoming an ordinary
   manifest row made it an ordinary member of the loop, emitted in every mode.
   A count-based or spot check would have passed.
2. **The mps2 witness leaves lost their locator.** The first cut keyed "does
   this row derive its isolation values?" on the ROLE, and those leaves are
   `talker`s — but their locator is `tcp/10.0.2.2:106xx`, a different board's
   allocator slot at the SLIRP host address, which the native_sim formula cannot
   produce. They would have been rebuilt against the wrong router. The predicate
   now keys on what the row AUTHORED.

**A third finding is declared, not fixed:** the logging-smoke leaf emits no
cmake defs and an EMPTY staleness signature into a real sig-file path — so its
signature is a constant and the leaf can never read stale, and it gets none of
the codegen-tool / toolchain-cache / sccache defs its siblings get. That is what
the hand-written block did. A byte-identical rewire must preserve it, so the row
carries `west_bare = true` with the anomaly written down. Fixing it changes what
rebuilds, which is its own change with its own evidence.

### What stayed in the script, deliberately

The ISOLATION FORMULA. A role leaf's zenoh/xrce port and cyclone domain is
allocator arithmetic over (lang, role) mirrored in `nros_tests::alloc`, so
exporting the computed value from the manifest would trade one duplication for
another — the manifest would become a second spelling of the allocator. Rows
carry identity; the script keeps the formula; a row carries a literal only where
the formula cannot produce one and the script already held that literal.

### Three things the rows exposed

1. **`zephyr-cortex-m` gets its first row.** The mps2 witness leaves spell the
   same `dir`, `lang` and `rmw` as their native_sim siblings, so one platform
   token for both would have given two rows one coordinate. phase-337 W2.c had
   already declared the token for exactly this board, noting "no `fixtures.toml`
   row spells it". These are that row — so **that comment is now stale**.
2. **The 12 entry leaves are Workspace cells, not `[[fixture]]` rows.**
   `fixture_rows_all_modeled_by_matrix` rejected them, naming the orphan
   coordinate `(ZephyrNativeSim, Mixed, Zenoh, is_ws=false)` — the mixed entry.
   A workspace row is a different SHAPE (`dir` is the workspace ROOT with
   `bringup`/`entry` beside it, not the entry app dir), so they are their own
   step. One already exists: `workspace-rust-zephyr`, which carries
   `skip_probe = true` and the precedent that a zephyr workspace row is built by
   the west lane.
3. **A west row's artifacts are genuinely unattributable today.**
   `row_artifact_root()` returns `""` for them rather than a repo-relative
   guess: the bytes land in the Zephyr WORKSPACE, whose root is a host fact no
   manifest can name. Failing closed beats a wrong path (phase-344 W2). Giving
   them a real root is W1.d.

**Land W1 before W2/W4.** Both need a row to attach a decision to.

## W1.b — The zephyr lane narrows by coordinate — **LANDED**

The point of the rows. `NROS_FIXTURE_COORDS` now reaches the zephyr lane, read
from the env and passed as `--coords-from` exactly like `fixtures-build.sh` and
`workspace-fixtures-build.sh` — one filter, through `row_coord`.

**Measured, same manifest, per lane:**

| lane | west leaves selected | of |
| --- | --- | --- |
| tier1 | 0 (zephyr not in the lane) | 70 |
| **tier2** | **7** | 70 |
| tier2-nightly | 38 | 70 |

tier2 needs exactly two zephyr coordinates (`zephyr,cpp,xrce` and
`zephyr-cortex-m,c,zenoh`) and now builds the 7 leaves on them instead of all
70. Against #509's measurement (~140 s of fixed per-leaf overhead, 40 min for
68 leaves, serial-added because zephyr is an order-only prerequisite of every
other family) that is the single largest cut available to the lane.

An empty or absent coords file is FATAL, not a silent fallthrough — falling
through would build everything while the log says "lane".

### The half that would have been a regression

Narrowing the BUILD without teaching the RUN is precisely the asymmetry
`lane_run_narrowing` exists to catch: the lane omits leaves, then the run
resolves one and fails on a fixture it deliberately did not build. W1.a had
excluded west rows from `lane::manifest_rows()` — the very gate that would have
caught it — so it would have shipped silently.

West leaves cannot be attributed by PATH (their artifacts live in the Zephyr
workspace, not under the row's `dir`), so they take the **coordinate route**:
the same one issue 0517's multi-row leaves take, keyed on the build-dir name
both halves already agree on. `require_west_leaf_in_lane` sits ahead of the
freshness check in both zephyr resolvers, and an unknown build dir does NOT skip
(fail-closed means run it).

Verified in both directions, not just gated: with tier2 coords a `zephyr,rust,zenoh`
leaf reports `[SKIPPED] out of lane`, and an in-lane `zephyr,cpp,xrce` leaf does
not. (Bare `cargo nextest` renders `skip!` as a failure; only `just test-all`'s
junit rewrite shows it as a skip — CLAUDE.md's note.)

`every_west_leaf_is_placeable_by_coordinate` is the family's half of the
attributability invariant, mirroring the multi-row arm: every west leaf must
carry a complete coordinate in the export the resolver queries, and build-dir
names must be unique because the lookup keys on them.

## W1.c — one matrix, one spelling — **LANDED**

`examples_fixture_coverage.rs` held `ZEPHYR_LANGS` × `ZEPHYR_ROLES`, the THIRD
copy of the zephyr matrix. It reads `lane::west_leaves()` now, so a leaf added
to or removed from the manifest changes what the gate considers covered without
anyone remembering to edit a constant.

That is this file's own failure mode applied to itself: a coverage gate whose
"covered" set is hand-maintained reports green for a dir nothing builds, the
moment the two drift.

**Verified non-vacuous.** Re-pointing the three `examples/zephyr/cpp/action-client`
rows at another dir makes the gate report `zephyr/cpp/action-client` as a silent
coverage gap; restoring them makes it pass. (The first attempt at this proof
failed for the WRONG reason — `west_role` rejected the renamed basename before
the coverage arm ran — so the rename was redone keeping a valid role basename.
A proof that fires on the wrong assertion proves nothing.)

## W1.d — RETRACTED: a west leaf should NOT be attributable by path

The item read: "`row_artifact_root()` answers for a west leaf, so the staleness
probe can attribute it instead of exempting it wholesale." Both halves of that
turned out to be wrong once W1.b landed.

**The lane half is already solved, by a better route.** W1.b showed west leaves
reach the lane by COORDINATE — the same route issue 0517's multi-row leaves
take — keyed on the build-dir name. Path attribution is not needed for lane
skipping and never was; it was the mechanism the other families happened to use.

**The staleness half would ADD a second answer.** West leaves already have a
purpose-built staleness pair: `.nros-zephyr-fixture.sig`, a content signature
the build writes into each build dir, and `is_binary_stale` on the test side.
Giving them a `row_artifact_root` so the GENERIC probe could also watch them
would mean two mechanisms answering "is this leaf stale?" for one family — the
exact duplication this phase exists to remove, and the shape of issue 0482.

So `row_artifact_root()` keeps returning `""` for a west row. That is not a gap:
it is the true statement "not attributable by path", which `fixtures::lane`
fails closed on and the coordinate route answers instead.

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

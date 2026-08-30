# Phase 350 — West fixtures join the manifest: one SSoT, one vocabulary, one coordinate

**Status: COMPLETE (archived 2026-08-13).** All work items landed; W1.d retracted
with reasons and W4 answered NO rather than acted on. Every acceptance item met,
including the wall-clock. All seven issues it filed are resolved: #535–#540 and
#549.

**Headline:** the zephyr lane costs 592 s warm; tier 2 costs 76 s — **7.8×**,
~8.6 min per tier-2 sweep. NOT the 31× a naive read of #509's published 40 min
suggests: that baseline was taken mid-sweep and does not reproduce standalone,
which is why the measurement was kept open instead of being closed on a leaf
count.

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

*Acceptance, met:* `just check fixtures-manifest` green with the new `--check`
leg; `grep -rn 'int32.observer\|int32_observer'` returns only archived docs and
this phase's own issue.

## W1 — The 74 get rows (#535) — **LANDED** (W1.d retracted, see below)

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
- [x] **4 `west-fixtures.sh` fixtures** — done in W2: they are
      `[[compile_check_fixture]]` rows and that script reads them. Its two bash
      arrays are gone, which completed the 74.
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

## W2 — Configure-only fixtures declare it, and stop paying for it in disk (#536) — **LANDED**

Three of four west fixtures assert a configure-time fact:

| fixture | asserts | needs ELF |
| --- | --- | --- |
| `west_board_import` | `CMakeCache.txt` ×4 | no |
| `zephyr_self_pkg_rust` | `system_config.h` | no |
| `zephyr_self_pkg_sibling` | `system_config.h` | no |
| `west_bringup_zephyr` | bake + boots `zephyr.exe` | **yes** |

The self-pkg pair runs a link `west-fixtures.sh:112` already calls "doomed", then
stamps on a file written before the link began.

- [x] The four fixtures are `[[compile_check_fixture]]` rows; `west-fixtures.sh`
      reads them and its two bash arrays are gone. **That was the last of the 74
      outside the manifest.**
- [x] The three configure-only ones build with `west build --cmake-only`.
- [x] `output` is the stamp gate for both shapes, and the BUILDER rides in the
      stamp, so a consumer can tell them apart (#537's failure mode).

### The measurement contradicted the item's premise

*Acceptance said: "per-leaf wall-clock measured before/after and recorded here."
Measured, and it does not say what the item assumed.*

| fixture | full `west build` | `--cmake-only` | |
| --- | --- | --- | --- |
| `west_board_import` | 3 s, **93 MB** | 3 s, **7.3 MB** | 12.7× less disk, same time |
| `zephyr_self_pkg_rust` | 3 s, 3.0 MB | 2 s, 3.0 MB | no saving |

"Stop paying for a kernel link" is true of **disk**, and only for
`west_board_import`. The self-pkg pair costs nothing to stop because there was
never a link: it fails at the cmake GENERATE step ("No SOURCES given to target:
app"), before any compilation. #536 read the script's own "attempts the doomed
link" comment as evidence of expense — a comment about intent, not a
measurement.

**So the value here is the DECLARATION, not the seconds:** four fixtures leave
their bash arrays for the manifest, `output` makes "configure only" checkable
instead of asserted in prose, and the stamp says which shape produced it.

*Acceptance, met:* the lane produced 3/4 before and 3/4 after on the same tree,
same fixture failing — `west_bringup_zephyr`, whose SystemModel is absent
(a pre-existing `nros sync` precondition, verified by running the pre-change
script in place: identical result, 16 s vs 18 s).

**A caveat on that verification:** the first attempt ran the old script from
`/tmp`, which broke its `repo_root` derivation and reported a meaningless 0/4.
A comparison whose control is misconfigured is worse than none — it was redone
in place via `git stash`.

## W3 — FVP: retired, not restored (#537) — **LANDED**

Maintainer decision (2026-08-13): *"We'll support FVP in the future, but we
don't have effort to work on it for now. I don't want to keep non-used code
there."*

So the half with no consumer is gone and the half with consumers is kept — which
is what a future revival needs anyway.

**Retired:** `build-fvp-aemv8r-cyclonedds` and `-rust`, their
`run-fvp-aemv8r-cyclonedds{,-rust}` siblings, and
`examples/zephyr/{rust,cpp}/talker-aemv8r` (~1 MB), plus the rust one's
root-workspace membership and the two `TEST_DRIVEN_BUILDERS` entries that had
been excusing them. An allowlist can only excuse a gap; deleting the code closes
it.

**Kept, each with a live consumer:** the `fvp-aemv8r-smp` board crate and
`nano_ros_use_board()`; the `west_board_import` fixture, whose test runs in CI
because it reads `CMakeCache.txt` and needs no FVP binary; `build-fvp-board-import`
+ `fvp_smoke.rs`; `build-fvp-ws-entry` + `fvp_runtime_ws.rs` +
`verify-fvp-runtime`; the `[gated.arm-fvp]` installer and SDK-index entry.
`build-fvp-all` now aggregates only `build-fvp-ws-entry`.

**Docs corrected, not left dangling.** The ARM FVP book chapter documented the
deleted recipes as its entire Build/Run path and linked a README that no longer
exists. It now documents the ws-entry and board-import lanes; `supported-boards.md`,
`environment-variables.md`, `examples/zephyr/README.md`, `check-example-matrix.sh`
and the board crate's own README (which used the deleted example as its usage
sample) follow.

**#537's second half is NOT closed and the issue says so:** none of the surviving
FVP artifacts is reachable from `build-test-fixtures`, so both gated tests still
skip with a message that cannot distinguish "license-gated SDK absent" from
"nobody built it". That is phase-217's when FVP work resumes.

## W4 — Leaf-count triage, on evidence — **ANSWERED: delete nothing**

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

### ANSWERED: no leaf should be deleted

With coordinates in place, #509's question is finally askable. Measured:

| lane | west leaves selected |
| --- | --- |
| tier1 | 0 |
| tier2 | 7 |
| tier2-nightly | 38 |
| **union of all lanes** | **44 of 70** |

**26 leaves (37 %, 63 GB) are selected by NO lane** — only `lane=all` builds
them. That looks like the retirement candidate #509 was reaching for, and it is
not: every one of their coordinates carries Runtime cells in `matrix::CELLS`
(`ZephyrNativeSim` alone has 3–10 cells per (lang, rmw) pair). They are outside
the pairwise **sample**, not outside the matrix. Deleting them deletes coverage
`lane=all` exercises — the same answer phase-329 W8 got when it assumed rows
were redundant, and the reason that item was retracted.

So **#509's "can the coordinate cover retire some?" resolves to NO.** What the
coordinate bought is W1.b's narrowing (tier2: 7 instead of 70), which is banked.
The remaining 63 GB is the price of `lane=all` covering the matrix, and the
lever on it is disk, not leaf count.

**(a) The four feature entries: 2.2 GB each, 8.8 GB total.** Consolidating them
into one multi-feature image would save ~6.6 GB — 3 % of the workspace — and is
NOT free: each is an isolated system with its own zenohd port, and the four
workloads (param services, QoS matching, lifecycle transitions, safety CRC)
would then share one `system.toml` and one image. That changes what is under
test, and the isolation exists precisely to stop them interfering. Given
phase-342 W1 (a fold cost 3.6× wall-clock and erased the scheduler vocabulary)
and phase-329 W8, **not attempted without a build to measure the runtime
effect** — which is a 40-minute lane this session did not run.

**(b) The three realtime entries: KEEP**, as drafted. phase-296 W5.5 made Zephyr
honour sched dims natively (`k_thread_deadline_set`); these are platform
behaviour, not feature duplication.

## W5 — One vocabulary (#539) — **LANDED**

- [x] **Work-item ids renamed and BANNED.** Nine fixture ids carried a work-item
      letter from a phase nobody has open. They now name behaviour, and
      `_reject_work_item_id` refuses the next one, on all three row tables:

      | was | is |
      | --- | --- |
      | `n9_form1`…`n9_form4` | `main_macro_form1`…`4` |
      | `o3_board_agnostic` | `board_agnostic_run_plan` |
      | `o4_pkg_index` | `pkg_index_workspace` |
      | `o5_nav2_compat` | `nav2_compat_smoke` |
      | `l9_register_c` / `_cpp` | `node_register_c` / `_cpp` |

      CLAUDE.md already forbids this for TEST names ("Phases go stale"); the
      argument is stronger for a fixture id, which outlives the phase by longer
      and is typed by hand into `--id`. The gate rejects a leading
      letter+digits+underscore and any `phase<N>` / `issue<N>` token — `mps2`
      and `qemu-arm-baremetal` keep their digits, because those are part of a
      name rather than an index into a plan.
- [x] **Lang axis: the `rs` short form is RETIRED** (2026-08-13). Zephyr build
      dirs are `build-rust-talker-zenoh` now, not `build-rs-…`.

      **A correction: this item said "`west_lang_tag` has ONE producer now, so
      the drift is contained." That was wrong.** There were TWO —
      `fixtures-manifest.py::west_lang_tag` on the build side and
      `nros_tests::zephyr::build_dir_for_example` on the test side, each
      carrying its own `rust` -> `rs` mapping. They had to be retired together
      or the build and the resolver would name different directories. The drift
      was not contained, which made this MORE worth doing than the deferral
      claimed, not less.

      Verified as a pair, not by inspection: the lane built
      `build-rust-talker-zenoh/zephyr/zephyr.exe` (77 s) and
      `case_01_zenoh_rust_talker_boots` then resolved it. 18 orphaned
      `build-rs-*` dirs removed — **48 GB**, workspace 208 GB -> 159 GB.
- [x] **`fixtures-cargo` → `cargo-fixtures` — DONE** (2026-08-13), and it
      measured what a kind rename costs. `mv` does NOT preserve the cache: the
      14 GB tree held `CMakeCache.txt` files with the old absolute path baked
      in, so the moved copy failed with "The current CMakeCache.txt directory
      ... is different than the directory ... where CMakeCache.txt was created".
      Wiped and rebuilt; the linux/rust slice alone took 453 s, other platforms
      rebuild when their lanes run.
- [x] **The named-constant extraction is DONE** (2026-08-13), which was the
      prerequisite `compile-check` was actually blocked on. Every kind is a
      constant now — `nros_tests::kind::*` and `NROS_KIND_*` in
      `build-root.sh` — so renaming one is TWO edits, and the three scripts that
      share the `compile-check` prefix are untouched. Demonstrated by making the
      change and reverting it.

      `build_root_derivation.sh` gates it both ways: a shell call site passing a
      bare word, or a Rust one passing a literal, fails. Both arms verified red
      first, and the Rust arm immediately found a kind the manual census missed
      — `rmw_zenoh_ws`, which the census regex had excluded because it uses
      underscores. A gate that finds something on its first run is the argument
      for writing it.
- [x] **`compile-check` → `compile-check-fixtures` — DONE** (2026-08-13). Two
      edits, as the extraction promised. 9.4 GB wiped, lane rebuilt in 105 s.

      **It also exposed that the gate was not pinning what I said it pinned.**
      `build_root_derivation.sh` compared `nros_build_dir compile-check` against
      the literal `build/compile-check` — a bare word on both sides, so it
      tested `nros_build_dir`'s joining and nothing about the vocabulary. It
      passed the rename unchanged, which is how I noticed. The family checks put
      the CONSTANT on the actual side now, so drifting a constant without moving
      its expected literal fails; verified by drifting one. The Rust unit test
      was a real pin already and caught the same change immediately.

**What the renames actually cost, now measured rather than estimated.** A kind
or tag rename FORFEITS the cache under it — `mv` does not work, because
`CMakeCache.txt` bakes its own absolute path and the moved copy fails with "The
current CMakeCache.txt directory ... is different than the directory ... where
CMakeCache.txt was created". So: 14 GB wiped for `cargo-fixtures` (linux/rust
slice rebuilt in 453 s), 48 GB wiped for the lang axis (one leaf rebuilt in
77 s). Total reclaimed 62 GB; the rest rebuilds when each lane next runs.

The one rename NOT done is the one that turned out not to be a cache problem at
all — see `compile-check` below.

## W6 — Close the class — **LANDED**

Every gap this phase found was invisible because the one coverage gate walks
`examples/**` for `package.xml` and nothing else.

- [x] `fixture_source_coverage.rs`, the sibling that covers the rest:
      every crate under `packages/testing/nros-tests/bins/` is a manifest row or
      a tracked exception with a reason — the hole #540 fell through — and the
      declared non-manifest producers must still exist.
- [x] Legitimate exceptions stay exceptions, but DECLARED: `ros-edition-pose-pub`
      (RFC-0058 per-run edition axis), `idf-fixtures.sh`, `ros-editions.just`,
      and the license-gated FVP recipes. A stale exception FAILS, the way
      `examples_fixture_coverage.rs`'s stale arm does.

**Verified failing in all three directions before being trusted:** an uncovered
bin fails; an allowlist entry that GAINS a row fails; a declared producer that
disappears fails. Restoring each makes it pass.

*Acceptance, met:* adding a fixture bin outside the manifest fails a gate whose
message names the manifest and the issue.

**Completed 2026-08-13:** `target-zenoh-fixture-posix` and the esp32 `.bin`
postprocess were the last two, and they did NOT need the row shape this section
predicted. Neither is a fixture in the manifest's sense — each is a postprocess
of another row's artifact — so what they shared with their consumers was a PATH,
and the KIND is the right unit. The zenoh one also moved out of the repo root
into the one build root (R1). Details in #535.

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

Scored 2026-08-13. Three met, one partial, one **not measured**.

- [x] **Zero west-built fixtures outside `examples/fixtures.toml`;
      `row_coord()` answers for all of them.** 70 zephyr leaves (W1) + the 4
      `west-fixtures.sh` ones (W2).
- [x] **The zephyr lane honors `NROS_FIXTURE_COORDS`**, and a narrowed lane
      builds strictly fewer leaves: tier2 selects 7 of 70, nightly 38, tier1 0.
- [x] **No build recipe produces an artifact no test consumes** — W3 retired the
      FVP pair, #549 retired the duplicate logging-smoke builder.
- [~] **One vocabulary, each gated.** Work-item ids: done and gated. The
      `build/<kind>` rule: RFC-0070 R5, and `fixtures-cargo` renamed to match.
      The lang axis: the `rs` short form is retired, both producers. Zephyr
      build-dir names: have a producer. Remaining: `compile-check`, which is not
      mechanizable until the kind is a named constant (see W5), and no gate —
      a gate on the kind rule would fail on that one known exception.
- [x] **MEASURED 2026-08-13.** Same 32-core host, same lane settings #509
      recorded (`concurrency: 4; ninja-jobs: 8; sccache on`), same
      `just zephyr build-fixtures` entry point, all three runs back to back:

      | run | leaves | state | elapsed |
      | --- | --- | --- | --- |
      | tier2 (coords-narrowed) | 7 | warm | **76 s** |
      | full lane | 70 | 18 cold | 1104 s |
      | full lane | 70 | all warm | **592 s** |

      **The zephyr lane costs 592 s warm; tier 2 costs 76 s. That is 7.8×, and
      ~8.6 minutes saved per tier-2 sweep.**

      **Two things this contradicts, both of which I had been repeating.**

      *#509's 40 min does not reproduce here.* The full warm lane is 9 m 52 s on
      the same hardware and settings, not 40 minutes. So the tempting
      "2400 s → 76 s = 31×" is a comparison across machine states, not a result
      of this phase. The honest ratio is against a baseline measured the same
      day: 7.8×.

      *The saving is SUBLINEAR in leaf count, and per-leaf tier 2 is WORSE.*
      Cutting 70 leaves to 7 is 10×, but the time only falls 7.8×, because
      per-leaf cost is 8.5 s across the full lane and **10.9 s** across tier 2's
      seven. Lane-level fixed cost — driver startup, the `nros sync` prep, the
      west-fixtures pass — does not shrink with the leaf set, so it lands on
      fewer leaves. Anyone predicting the saving from the leaf count alone will
      over-promise.

      Incidentally measured: run A − run B = 512 s for 18 cold leaves ≈ **28 s
      per cold leaf**, which is the real cost of the rename-induced wipes in W5.

## Verdict

Every acceptance item is met. The headline is 7.8× on the zephyr lane for tier 2,
not the 31× the published baseline would have suggested — and the difference
between those two numbers is the whole reason this item was kept open instead of
being closed on a leaf count.

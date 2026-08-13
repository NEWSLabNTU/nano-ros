---
id: 535
title: "74 west-built fixtures have no manifest row, so they have no coordinate — the lane predicate cannot see them in either direction"
status: resolved
type: tech-debt
area: build, testing
related: [issue-0482, issue-0509, issue-0517, issue-0536, issue-0539, phase-340, phase-344]
---

## Problem

`examples/fixtures.toml` is the fixture SSoT: 250 `[[fixture]]` + 93
`[[workspace_fixture]]` + 36 `[[compile_check_fixture]]` = **379 rows**, and
`row_coord()` in `fixtures-manifest.py` is the ONE computation that gives a row
its `(platform, lang, rmw)` coordinate. Phase-340 W3 made that coordinate the
single predicate for both halves of a lane:

```
BUILD  skips row R  ⟺  row_coord(R) ∉ lane_coords     (--coords-from)
RUN    skips row R  ⟺  row_coord(R) ∉ lane_coords     (fixtures::lane)
```

**74 west-built fixtures are outside that manifest entirely**, in two scripts
that each carry their own matrix:

| source | count | matrix lives in |
| --- | --- | --- |
| `scripts/build/zephyr-fixture-leaves.sh` | **70** | `scripts/build/fixture-matrix.sh` (`nros_fixture_langs`, `nros_fixture_roles`) + inline `fixture_rmws=(zenoh xrce cyclonedds)` |
| `scripts/build/west-fixtures.sh` | **4** | `WEST_FIXTURES=()` / `SELF_PKG_FIXTURES=()` bash arrays |

Reproduce the 70:

```sh
bash scripts/build/zephyr-fixture-leaves.sh --emit records \
  --include-logging-smoke --include-workspace-entry | wc -l   # 70
```

Shape: 3 langs × 6 roles × 3 rmws = 54, plus 12 `ws-*-entry` leaves, 3 mps2
talkers, 1 logging-smoke.

## Consequences

1. **The build half cannot narrow.** `NROS_FIXTURE_COORDS` has **zero** hits in
   `just/zephyr-ci.just`, `zephyr-fixture-leaves.sh` and
   `zephyr-fixture-make-driver.sh`. A lane can only include or exclude the
   zephyr MODULE wholesale (`nros_lane_modules`), never a coordinate inside it.
   So `lane=tier2` — which is 1-wise over platform and therefore contains
   zephyr — pulls all 70.
2. **The run half cannot attribute.** `fixtures/lane.rs:67` already names these
   as handled "module-level rather than by coordinate", and `:381` records that
   the leaves are unattributable by path. Per CLAUDE.md the staleness probe
   therefore never skips them — correct given the missing fact, but it means no
   coordinate-scoped run can ever include or exclude them on merit.
3. **The SSoT claim is false for ~16 % of the surface.** 74 of 453 fixture
   builds (379 + 74) answer to a bash array instead of `row_coord()`. That is
   the same shape as issue 0482 (two computations of one coordinate), except
   here the second computation is not a disagreeing copy — it is an ABSENCE, so
   nothing can even report the divergence.
4. **`just fixture-staleness` and the coverage gates are blind to them.**
   `examples_fixture_coverage.rs` compensates by hardcoding the role matrix a
   THIRD time (`ZEPHYR_LANGS` × `ZEPHYR_ROLES`, `examples_fixture_coverage.rs:52`)
   so the `examples/zephyr/**` dirs read as covered. Three spellings of one
   matrix, none of them the SSoT.

## Why it matters now

Issue 0509 measured the lane at **40 min for 68 leaves**, serial-added to every
full sweep because zephyr is an order-only prerequisite of every other platform.
Its last direction reads: "Question whether all 68 leaves must be in `lane=all`
at their current granularity, or whether the coordinate cover (phase-340 W3) can
retire some." **That question cannot be asked of a fixture with no coordinate.**
This issue is the structural prerequisite for 0509's cheapest lever.

Phase-329 W8.d is blocked on the same fact from the other side: a
coordinate-scoped tier-2 RUN needs every test cell-bound, and a cell cannot bind
to a fixture the coordinate system cannot name.

## Direction

Give all 74 rows in `examples/fixtures.toml` and make the two scripts CONSUME
the manifest (`--coords-from`, as `fixtures-build.sh` and
`workspace-fixtures-build.sh` already do) rather than declare their own matrix.
The manifest already models a non-cargo builder (`builder = "cmake"`,
`"cross-build"`, `"cmake-configure"`), so a `builder = "west"` row is the
existing seam, not a new concept.

Fix the CLASS: the three matrix spellings (`fixture-matrix.sh`,
`west-fixtures.sh` arrays, `examples_fixture_coverage.rs` constants) must
collapse to one, and `examples_fixture_coverage.rs` must then read the rows
instead of restating them — otherwise this is the sizes-header mirror again.

## Resolved 2026-08-13 (phase-350)

**The 74 west fixtures are manifest rows** — 70 zephyr leaves (W1) and the four
`west-fixtures.sh` ones (W2) — the emitter reads them, and the lane narrows by
coordinate: tier 2 builds 7 leaves instead of 70 (measured 592 s -> 76 s).

**The two literal-path fixtures this issue also named are fixed, by a different
mechanism than a row.** Neither is a fixture in the manifest's sense — each is a
POSTPROCESS of another row's artifact, and the manifest has no shape for that.
What they actually shared with their consumers was a PATH spelled literally on
both sides, so the fix is the KIND, not a row:

| was | now | producer / consumer |
| --- | --- | --- |
| `target-zenoh-fixture-posix/` at the REPO ROOT | `build/zenoh-fixture-posix` via `kind::ZENOH_FIXTURE_POSIX` | `just build-zenoh-posix-fixture` / `zenoh_archive_symbols`, `zenoh_header_parity`, `zpico_build_matrix` |
| `build/esp32-qemu/*.bin` literal in 7 places | `kind::ESP32_QEMU` / `$NROS_KIND_ESP32_QEMU` | `just esp32 build-qemu` / `esp32_emulator` |

The zenoh one also moved OUT of the repo root and under the one build root,
which is RFC-0070 R1 — a `target-*` dir beside the workspace is exactly what R1
exists to prevent, and its `.gitignore` entry is gone because `/build/` already
covers it.

**This filing's list was incomplete, and the sweep found more.** It named two
tests reading the zenoh fixture; there are three (`zpico_build_matrix` too). It
implied one or two esp32 sites; there were seven, including
`esp32-ws-entry.bin`. Fixed all of them; verified by rebuilding the zenoh
fixture at its new path (21 s) and running all four tests that read it.

**One bug caught before landing:** routing `just esp32 clean` through the
constants, I mapped `build/esp32-zenoh-pico` onto `NROS_KIND_QEMU_ZENOH_PICO` —
a DIFFERENT directory (the ARM archive, not the RISC-V one). `clean` would have
deleted the wrong tree. `esp32-zenoh-pico` has its own constant now.

Related work this issue produced: RFC-0070 R5 (the kind naming rule), the
named-constant extraction that made a kind rename two edits, and the gate that
refuses a bare-word or literal kind in either language.

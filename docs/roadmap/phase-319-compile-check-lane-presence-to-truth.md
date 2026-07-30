# Phase 319 — the compile-check fixture lane answers truth, not presence

**Status (2026-07-30): W1–W3 landed. Phase complete.**
**Closes:** issue 0351. **Informed by:** issues 0350, 0196, 0030, 0309.
**Extends:** [RFC-0061](../design/0061-fixture-freshness-and-test-tiers.md) /
[phase-318](phase-318-fixture-freshness-and-tiers.md) — that phase sharpens the
WORKSPACE lane's freshness signal; this one gives the COMPILE-CHECK lane the
mechanisms it never had.

## Goal

Every gate over `scripts/build/compile-check-fixtures.sh` currently answers *"is
the artifact present?"*. The question that matters is *"is it TRUE?"* — produced
by a build that succeeded, from the current sources, on a machine that could have
built it at all. Issue 0350 (that script red on `main` for three days, unnoticed)
is what the gap costs.

The workspace lane already answers all three, and this phase borrows its
mechanisms rather than inventing any:

| question | workspace lane | compile-check lane (today) |
| --- | --- | --- |
| did the build SUCCEED? | no signature written on failure | `.compile-ok` survives from an earlier run |
| are the sources CURRENT? | `.inputsig` recomputed and compared | nothing |
| absent, or BROKEN? | `nros_toolchain_present` probes the toolchain | inferred from a missing file → skipped |

## W1 — clear the suite stamp before the attempt (issue 0351 defect 1)

`target/nextest/.fixtures-built` is written on success and removed by nothing, so
it answers *"did this build stage EVER succeed?"*. A regression aborts the recipe
under `set -e` and the old stamp survives, so `_require-fixtures` waves `test-all`
through.

- [x] **W1.a** `rm -f target/nextest/.fixtures-built` at the TOP of
      `build-test-fixtures` and `build-all`, before any work.
- [x] **W1.b** Comment naming the invariant, pointing at
      `compile-check-fixtures.sh`'s per-fixture `rm -f .compile-ok` — the same
      discipline, one level down, already correct.

**Done when:** a `build-test-fixtures` that fails leaves NO stamp, so the next
`test-all` fails with the existing build hint instead of running on a stale one.

## W2 — the lane's inventory moves into `examples/fixtures.toml`

`AGENTS.md:79` already prescribes this ("add a row to `examples/fixtures.toml`"),
and the lane is drift from it: 26 entries live in **six** hardcoded arrays inside
the shell script, each with its own colon-delimited positional format. That is
also *why* `check-fixtures-stale.sh` cannot see them — it enumerates the manifest.

The rows are not a separate species. Ten are compile-intent checks with no runtime
artifact; the other sixteen produce binaries and JSON that tests read or execute.
What differs from an ordinary `[[fixture]]` row is the **builder** and the
**output path**, not the kind of thing they are.

- [x] **W2.a** Manifest schema gains `builder` and `output` (the path a test
      resolves, relative to the row's build root). Builders: `cargo-check`,
      `cargo-build`, `cmake-configure`, `cross-build`, `cxx-syntax`.
      *Landed as its own `[[compile_check_fixture]]` table, not as fields on
      `[[fixture]]` — see the note below.*
- [~] **W2.b** ~~Relax `platform`/`lang` to optional when `builder` is not the
      default.~~ **SUPERSEDED, not done:** unnecessary once the rows got their own
      table. No existing validation was relaxed.
- [x] **W2.c** Port all 26 entries as manifest rows; delete the six arrays.
- [x] **W2.d** `compile-check-fixtures.sh` reads the manifest
      (`fixtures-manifest.py list --builder …`) instead of its arrays. Its
      per-builder functions stay; only the inventory moves.
- [x] **W2.e** `NROS_FIXTURE_ID=<id>` narrowing, matching
      `workspace-fixtures-build.sh` (added while fixing #342) — the lane is
      currently all-or-nothing, which is why iterating on one fixture means
      rebuilding twenty-five.

**Done when:** `compile-check-fixtures.sh` contains no fixture inventory, the
manifest lists all 26, and the script's output is unchanged for a clean build.

**Landed.** 26 rows validate; per-builder counts 7/5/9/3/2 match the six deleted
arrays exactly; a full lane run exits 0.

W2.b turned out unnecessary: the rows are their own `[[compile_check_fixture]]`
table rather than `[[fixture]]` rows with a `builder` field, so no existing
validation was relaxed and the 251 existing rows were not touched. Reading the
consumer is what changed the design — `list`'s record format is per-language and
consumed positionally by `fixtures-build.sh`, so overloading it would have
changed that contract. The risk noted below therefore did not materialise.

Two bugs surfaced while porting, both by running the thing:

- **Mine:** `o5_nav2_compat`'s third colon field is `manifest_dir`, not `pkg`.
  Mis-mapped, the build failed with "package ID specification `demo_entry` did
  not match any packages".
- **Pre-existing** (confirmed against clean `main`): `orch_tiers_single` stripped
  `[tiers.*]` from `system.toml` to force the legacy single-tier path, but the
  entry is `nros::main!(model = …)` and phase-296 made the MODEL authoritative.
  The strip had stopped doing anything;
  `single_tier_system_takes_the_legacy_boardentry_run_path` was RED on `main`.
  Now strips `execution.tiers` from the model too (bindings preserved). This
  issue's own theme, one layer over: an overlay certifying something it no longer
  affects.

## W3 — signature + toolchain predicate (issue 0351 defects 2 and 3)

- [x] **W3.a** A signature per compile-check row, on the
      `workspace-fixture-signature.sh` model: hash the manifest record plus the
      row's source tree, write it **after** a successful build, and have the
      staleness probe recompute and compare. A failed build writes none; a source
      edit invalidates one — defects 1-and-2 closed together, per row.
- [x] **W3.b** Teach `check-fixtures-stale.sh` the new rows (it already fans out
      over the manifest, so this is a second record kind, not a second probe).
- [~] **W3.c** ~~Declare each row's toolchain requirement in the manifest and gate
      on the shared `nros_toolchain_present`.~~ **SUPERSEDED, not done:** met by
      the `.build-failed` marker instead — see the note below. No per-row
      `toolchain` field was added.
- [x] **W3.d** The resolver hard-fails in **every** tier, including
      `NROS_FIXTURES_OPTIONAL`, when the build stage recorded a FAILURE for that
      fixture. A genuinely absent toolchain (no marker) still skips.

**Done when:** breaking a compile-check fixture turns the suite red on a machine
that has the toolchain, in the light tier — the lane people actually run. That is
the acceptance test for the whole phase, and it is exactly what #350 failed.

**Landed, and the acceptance test passes:** breaking `l9_register_cpp`'s
CMakeLists and running `NROS_FIXTURES_OPTIONAL=1` now fails with *"Test fixture
FAILED to build (not merely absent)"*, naming the fixture and builder. Restored →
green.

W3.c was met by the `.build-failed` MARKER rather than a per-row `toolchain`
declaration: the build stage already knows whether it could run a builder, so
recording the outcome it observed is strictly better than re-deriving it
test-side from a predicate that would have to be kept in sync. `nros_toolchain_present`
stays the workspace lane's mechanism; this lane did not need to duplicate it.

Two wrong shapes were caught by the acceptance test before landing, both mine and
both the same bash trap: **errexit is suppressed for anything in a condition
context**. `if ! builder` let a failing `cmake -S` fall through so the function
returned its trailing `echo`'s status — a broken fixture reported as BUILT — and
`( set -e; builder ) || rc=$?` inherited the same suppression through the `||`
list. The landed shape is an `ERR` trap with the builder called bare (needs
`set -E`): no condition context, fail-fast preserved, and the trap records which
fixture died on the way out.

## Deliberately NOT in scope

- **A `just check` step that re-runs the build stage.** Rejected in issue 0351:
  `check-fast` is buildless by design, the exit code already propagates, and the
  defect was never a missing assertion.
- **Moving compilation into nextest.** `AGENTS.md:79`'s reasons hold — in-test
  builds are wall-clock dominated by compile time (spurious `timed out` under
  load), serialize on the cargo/cmake locks, and conflate "does it build" with
  "does it behave". The one sanctioned exception (`cmake_node_register_misuse.rs`)
  earns it by testing a configure-FAIL, which cannot be a passing prebuilt
  artifact and fails fast.
- **Renaming or restructuring the fixtures themselves.** Inventory moves; the
  builds do not change.

## Consequences worth knowing

- **An ENVIRONMENTAL build failure now hard-fails rather than skipping.** A
  crates.io fetch blip hit `n9_form3` during the final full run; the marker
  recorded it and the row would have failed its test until rebuilt. That is the
  intended trade — do not test against a fixture that could not be built,
  whatever the cause — and the remedy is the same re-run. It does mean a flaky
  network turns into a red test rather than a quiet skip.
- **The compile-check probe runs AFTER the workspace probe in
  `check-fixtures-stale.sh`, which exits on its own failures first.** On a tree
  with stale workspace fixtures the compile-check section is never reached. That
  matches the gate's existing fail-fast shape, so it was left alone, but it means
  the two sections are not reported together.
- **Gate cost:** the compile-check section is 2.17s for all 26 rows. The gate's
  overall runtime is dominated by the workspace section (~1.4s per row across
  ~86), which is RFC-0061 / phase-318's subject, not this phase's.

## Risks

- ~~**W2.b touches shared validation.**~~ Did not materialise — the separate
  table meant no existing validation was relaxed. See W2.
- **W3.d changes what a light-tier run reports.** Fixtures that quietly skipped
  will start failing on machines that have the toolchain — which is the point, but
  it will look like new breakage on first run. Expect to fix real staleness the
  first time it lands.

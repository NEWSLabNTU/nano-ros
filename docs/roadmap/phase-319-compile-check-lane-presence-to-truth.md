# Phase 319 — the compile-check fixture lane answers truth, not presence

**Status (2026-07-30): W1 landed; W2–W3 pending.**
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

- [ ] **W2.a** Extend the manifest schema with an optional `builder` (default
      `cargo`, so all 251 existing rows are untouched) and an `output` (the path a
      test resolves, relative to the row's build root). Builders:
      `cargo-check`, `cargo-build`, `cmake-configure`, `cross-build`,
      `cxx-syntax`.
- [ ] **W2.b** Relax `platform`/`lang` to optional **only** when `builder` is not
      the default — a cmake-configure row has no meaningful lang, and inventing
      one would be a lie the checker then enforces. Validation stays strict on
      the default builder.
- [ ] **W2.c** Port all 26 entries as manifest rows; delete the six arrays.
- [ ] **W2.d** `compile-check-fixtures.sh` reads the manifest
      (`fixtures-manifest.py list --builder …`) instead of its arrays. Its
      per-builder functions stay; only the inventory moves.
- [ ] **W2.e** `NROS_FIXTURE_ID=<id>` narrowing, matching
      `workspace-fixtures-build.sh` (added while fixing #342) — the lane is
      currently all-or-nothing, which is why iterating on one fixture means
      rebuilding twenty-five.

**Done when:** `compile-check-fixtures.sh` contains no fixture inventory, the
manifest lists all 26, and the script's output is unchanged for a clean build.

## W3 — signature + toolchain predicate (issue 0351 defects 2 and 3)

- [ ] **W3.a** A signature per compile-check row, on the
      `workspace-fixture-signature.sh` model: hash the manifest record plus the
      row's source tree, write it **after** a successful build, and have the
      staleness probe recompute and compare. A failed build writes none; a source
      edit invalidates one — defects 1-and-2 closed together, per row.
- [ ] **W3.b** Teach `check-fixtures-stale.sh` the new rows (it already fans out
      over the manifest, so this is a second record kind, not a second probe).
- [ ] **W3.c** Declare each row's toolchain requirement in the manifest and gate
      on the SHARED `nros_toolchain_present`
      (`scripts/test/toolchain-gate.sh`) — never on a missing artifact.
- [ ] **W3.d** `require_compile_check` / `require_cmake_fixture`: when the
      artifact is missing AND its toolchain is present, hard-fail in **every**
      tier including `NROS_FIXTURES_OPTIONAL`. Only a genuinely absent toolchain
      skips.

**Done when:** breaking a compile-check fixture turns the suite red on a machine
that has the toolchain, in the light tier — the lane people actually run. That is
the acceptance test for the whole phase, and it is exactly what #350 failed.

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

## Risks

- **W2.b touches shared validation.** Relaxing required keys is the one change
  that can silently weaken the checker for the 251 existing rows. Gate it on
  `builder != default` and add a manifest test that a default-builder row missing
  `platform`/`lang` still fails.
- **W3.d changes what a light-tier run reports.** Fixtures that quietly skipped
  will start failing on machines that have the toolchain — which is the point, but
  it will look like new breakage on first run. Expect to fix real staleness the
  first time it lands.

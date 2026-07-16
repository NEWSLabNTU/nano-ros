---
id: 202
title: "nros-cli-core orchestration_e2e suite: 17/17 red (fixture paths predate the phase-218 in-tree move) and NO lane ever runs it"
status: resolved
type: tech-debt
area: testing
related: [issue-0181, phase-218, phase-287]
---

## Problem — two stacked gaps

### 1. The suite is wired into nothing (the #181 silent-lane class)

`packages/cli/nros-cli-core/tests/orchestration_e2e.rs` (17 tests: workspace
plan/check/build roundtrips for every platform family, metadata-mode builds,
the two-router bridge forward, the fibonacci action tick exchange) is not run
by ANY lane: no `just` recipe and no workflow executes `cargo test
--manifest-path packages/cli/Cargo.toml` — grep `just/`, `justfile`,
`.github/workflows/` for a cli test invocation: only the CLI *binary build*
exists (host-tests.yml caches/builds `packages/cli/target`, never tests).
The unit-level crates (rosidl-codegen etc.) are green, but only when someone
runs them by hand.

### 2. The suite is 17/17 red — fixture paths predate the in-tree move

Every failure is path rot, not orchestration logic:

- Tests resolve the fixture workspace at
  `<repo-root>/packages/testing_workspaces/orchestration_e2e/...`, but the
  fixture tree lives at **`packages/cli/testing_workspaces/`** since the
  phase-218 migration (the CLI sub-workspace owns `testing_workspaces/` —
  see CLAUDE.md "Codegen + orchestration CLI lives in-tree").
  Observed: `cc1: fatal error: .../packages/testing_workspaces/
  orchestration_e2e/src/c_counter/counter.c: No such file or directory`,
  `failed to read .../packages/testing_workspaces/orchestration_e2e/src/
  demo_pkg/Cargo.toml`.
- A second, distinct walk-up bug loses the repo dir entirely:
  `failed to read /home/aeon/repos/packages/core/nros/Cargo.toml` (note the
  missing `nano-ros/` segment) — some path is derived relative to the wrong
  anchor (likely the same root-derivation helper counting `../` from the old
  crate location).
- `metadata_build_ws` sibling test then asserts against artifacts the broken
  build never produced (`metadata-mode build wrote .../src/probe_pkg/
  node.metadata.json` — the build wrote into the source tree instead of the
  expected artifacts dir, again a wrong-anchor symptom).

The suite necessarily broke AT the phase-218 move and has been dead since —
every subsequent orchestration change (287 ament shape, 288, the CLI
staleness guard) landed with zero e2e coverage from this suite.

## Fix shape

1. Point the fixture-path helper(s) at the sub-workspace root
   (`CARGO_MANIFEST_DIR` of nros-cli-core → `../testing_workspaces/...`),
   killing both the `packages/testing_workspaces` and the
   dropped-`nano-ros`-segment derivations.
2. Re-triage whatever still fails after the paths are right — 287's ament
   reshape may have changed plan/build outputs the asserts encode.
3. Wire the suite into a lane: minimum `just check-cli-tests` (or fold into
   `test-all`) running `cargo test --manifest-path packages/cli/Cargo.toml`;
   the heavier fixture builds may need the same prebuilt-fixture treatment as
   the main suite (no compilation inside tests is the repo rule — several of
   these tests DO invoke cargo/cmake at runtime, which also wants a look
   against AGENTS.md Testing when re-wiring).

## Repro

```sh
cargo test --manifest-path packages/cli/Cargo.toml --test orchestration_e2e
# 17 failed, 0.03 s — all path errors, no orchestration logic reached
```

Found during the 2026-07-16 phase-229 completion audit (a full
`packages/cli` `cargo test` sweep).


## Resolution (2026-07-16) — re-triaged: retire the dead path, salvage the live one, wire the lane

Re-triage first: 15 of the 17 red tests exercised
`orchestration::build::build_generated_package` → `generate::generate_package`
— the phase-172-era "generated standalone system package" deployment model
whose CLI entry points (`nros build` / `nros run`) were REMOVED in phase-222
(`5159d88f1`). Zero production callers since; the current design is
`nros::main!` + `nros-build` (`planner::plan_system` → `run_plan.rs`) and the
RFC-0048 workspace verbs. Repairing those tests would have revived museum
coverage of an unreachable pipeline (they also ran cargo at runtime).

What landed instead:

1. **Dead path retired** (−9,346 lines): `tests/orchestration_e2e.rs`,
   `tests/orchestration_generate.rs`, `orchestration/build.rs`,
   `orchestration/generate.rs`. The two LIVE fns (`render_bridge_runtime_config`
   / `render_bridge_entry_fns`, consumed by `cmd::ws` + `nros-build`) moved
   verbatim with their helper cluster + 6 unit tests to
   `orchestration/bridge_gen.rs`. `plan.rs` lost the dead `TierSched`.
2. **Live path salvaged** as `tests/plan_pipeline_e2e.rs`: the
   metadata→plan→check roundtrip + the two metadata-mode build tests, with the
   root-cause path fix (the old helpers counted `ancestors()` from the crate's
   pre-phase-218 location). Four live bugs surfaced and fixed while reviving
   them — each rotted invisibly while nothing ran the suite:
   - the generated metadata-probe `Cargo.toml` lacked `[workspace]` → captured
     by any enclosing cargo workspace ("believes it's in a workspace");
   - the harness template still called the retired `record_component_metadata`
     (renamed `record_node_metadata`);
   - the `demo_pkg` fixture implemented a pre-212.K `RosAction` (missing the
     five wire-envelope assoc types);
   - `orchestration_self_bringup_cargo_metadata`'s record encoded the
     pre-M-F.17 `(package, executable)` pair (crate name; the α-bridge now
     synthesises the component name for staticlib pkgs).
   Assertions updated to the live recorder's id scheme (no more
   `node_`/`pub_`/`timer_` prefixes).
3. **Lane wired**: `just check-cli-tests` (private, in `check-build`) runs
   `cargo test --manifest-path packages/cli/Cargo.toml --workspace` — the ~870
   unit tests + the salvaged e2e now run on every `just check`. A cwd race in
   `phase_212_f_bringup` (two `set_current_dir` tests, process-global cwd)
   surfaced immediately and is serialized with a lock. 3× full sweeps green.

The metadata-mode tests compile tiny probe crates at runtime BY DESIGN — the
verb under test is a compile-driver; a prebuilt fixture would bypass exactly
what it tests (noted in the suite header against the no-compile-in-tests rule).

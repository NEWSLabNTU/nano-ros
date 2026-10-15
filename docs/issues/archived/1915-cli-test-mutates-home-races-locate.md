---
id: 1915
title: "A `doctor` test set the process-wide `$HOME`, and `board_facts`' agreeing-duplicates test resolved one FreeRTOS kernel two ways on the tier-2 runner"
status: resolved
type: bug
severity: medium
area: [cli, testing, ci]
related: [1911, 1905, phase-466]
found: 2026-10-15
resolved: 2026-10-15
---

**Home: [phase-466](../../roadmap/phase-466-scheduled-lanes-produce-verdicts.md)** — a scheduled lane that cannot reach its cells.

## Measured

`run-matrix` run 38663782341 was dispatched on `main` at `9e9f24392c`. Tier 2
got past `just build tier2` for the first time in this sequence. It then
stopped in `just ci matrix`'s `check::default`, before any cell, and
`check-cli-tests` was one of two red gates:

```
cmd::board_facts::tests::duplicate_deploys_that_agree_are_not_ambiguous --- FAILED
agreeing duplicates resolve: deploys `freertos` and `mps2-an385-freertos` resolve DIFFERENTLY (NROS_SDK_FREERTOS_KERNEL); pass --deploy or --board to say which one this build is
test result: FAILED. 1690 passed; 1 failed
```

The same test passes in this repository's local `ci gate` runs.

## Cause

Two tests in one process race on a process-global, `$HOME`:

- **Two resolutions.** `board_facts::resolve` resolves each deploy
  separately. For the board's index sources (`board_sources`) it calls
  `locate_source`, which takes the store rung from
  `nros_build_paths::store::root_unchecked()`, i.e. `$NROS_HOME` or
  `$HOME/.nros`. Two deploys on the same board therefore make two
  independent ladder walks, and the test expects them to agree.
- **A moving `$HOME`.** `doctor::tests::license_gate_board_filter_arm_fvp_warns_only`
  set `$HOME` to a temp dir, so the FVP probe would find no installer landing
  under `$HOME/.nros`. It restored `$HOME` afterwards. `cargo test` runs tests
  on parallel threads, so a walk that straddled the change saw the store in
  one call and not in the other.
- **Why only the runner.** The runner holds the FreeRTOS kernel in the store,
  with no checkout submodule, so the two walks gave different answers. A
  checkout that also has the submodule lands on the same tree either way.

The `$HOME` change bought the test nothing. The FVP probe only ever warns, and
the test asserts the problem count, which the probe never changes.
`doctor.rs`'s own doc comment on `install_report` already states the rule:
every input is a parameter, "without a test mutating `$NROS_HOME` (which in
this crate is a cross-thread race)". `check_license_gates` was the function
that did not follow it.

## Fix

- `check_license_gates` and `check_arm_fvp` take the store root as a
  parameter. `nros doctor` passes `store::root()`.
- The test passes a directory of its own and no longer touches `$HOME`.

Sweep for the class:

```
git grep -nE 'set_var\("(HOME|NROS_HOME|XDG_[A-Z_]+|NROS_WORKSPACE_ROOT)"' -- 'packages/cli/**/*.rs' 'packages/tooling/**/*.rs'
```

The only other hit is `NROS_WORKSPACE_ROOT` in
`packages/cli/nros-build/tests/pkg_index.rs`. That file builds its own test
binary, so it cannot race a test in `nros-cli-core`.

**Acceptance.** A scheduled tier-2 run in which `check-cli-tests` passes on
the runner.

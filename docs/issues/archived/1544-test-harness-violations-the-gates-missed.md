---
id: 1544
title: "Test-harness violations present on `main` that their own gates cannot see"
status: resolved
type: bug
area: [testing]
severity: medium
found: 2026-09-28
resolved: 2026-09-28
related: [phase-472, 0853, 0445, 1552]
---

Found by the phase-472 audit; each is a rule CLAUDE.md or an existing gate
states, broken in the tree today, invisible to the gate that enforces it.

## 18 fixture resolvers bypass `.require()` through `match`

VERIFIED count: 18 sites of `match build_x() { Ok(p) => p, Err(e) => panic!(…) }`
in `packages/testing/nros-tests/` — e.g. `binaries/mod.rs:6316/6351/6399/6430`,
`tests/action_raw_goal_e2e.rs:50`, `tests/contract_monitor_parity.rs:83`.
`check-fixture-require` recognises only `.expect(`, `.unwrap_or_else(`, `.unwrap(`;
its baseline happens to read 18 for a different reason. Its selftest exercises a
different matcher than the one it runs.

## 11 print-only tests in `src/` unit-test modules

`check-no-vacuous-tests` reads `tests/` only. Its own `scan()`, run over the other
tracked Rust, finds 11 — ten of them the 2026-08-21 "detection" shape the cleanup
claimed to remove (`nros-tests/src/esp32.rs`, `src/process.rs`, `src/ros2.rs`,
`src/fixtures/tls_certs.rs`).

## A zombie-blind process-group scan

VERIFIED: `packages/testing/nros-tests/src/process.rs:443` runs
`ps -eo pid,pgid --no-headers` with no `stat`, and the result is asserted empty
after the sweep. In a `container:` job PID 1 does not reap zombies, so this is
issue 0853's false verdict. `check-ps-zombie-blind.sh` matches shell syntax only.

## Six `ZenohRouter::start*().expect()` outside `packages/testing`

`packages/rmw/zenoh/nros-rmw-zenoh/tests/session_config_keys.rs:116` and
`zenoh_integration.rs:40, 708, 819, 1049, 1131`. `check-zenohd-router-skips`
reads `packages/testing` only. Most have an availability pre-guard, which the gate
does not model.

## Fix

Each gate's population moves to phase-472 W5/W7; the sites above convert.

## Resolution

All four sites are converted and all four gates widened; every widening was
mutation-tested against the origin/main gate (each restored hole: new gate rc 1,
old gate rc 0; fixed tree: rc 0).

- **Fixture resolvers.** `check-fixture-require` now counts a resolver that is
  the direct scrutinee of a `match`, `if let`/`while let` or `let … else`. That
  found exactly the 18 sites; all 18 now call `.require(…)`. Its self-test
  drives the real `sites()` over temp files instead of a private copy.
- **Print-only unit tests.** `check-no-vacuous-tests` reads every tracked `.rs`
  (with a floor that fails if it stops reaching `src/`) and runs its self-test
  on every invocation. The ten `*_detection` tests and zephyr's tautological
  `test_workspace_detection` are deleted; `print_bake_table` now asserts the
  table is non-empty. `test_generate_certs`'s `eprintln!`+`return` became
  `skip!`.
- **Zombie-blind scan.** `process.rs`'s group enumeration asks for `stat=` and
  drops `Z`. `check-ps-zombie-blind.sh` also matches the argv spelling
  (`"-eo"` + a column list naming `pgid` without `stat`, across up to three
  lines), through one awk program shared by the self-test and the scan.
- **Router `.expect()`s.** All six use `fixtures::or_skip`;
  `check-zenohd-router-skips` reads every tracked `.rs`, with a floor that fails
  if the scan stops reaching `packages/rmw/`.

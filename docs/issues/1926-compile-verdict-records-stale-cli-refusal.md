---
id: 1926
title: "A compile-check VERDICT recorded while the CLI refused as STALE is reused after `setup-cli`, so two `test-lane-contracts` tests stay red against a fresh CLI"
status: open
type: bug
severity: medium
area: [ci, testing, tooling]
related: [issue-0445, issue-1620, issue-0196]
found: 2026-10-16
---

## Measured

Phase-503 branch (`refactor/cpp-headers-rclcpp`), 2026-10-16:

1. Uncommitted edits to three CLI test files (`scaffold.rs`,
   `integration_tests.rs`, `tests_cpp.rs`) made the in-tree `nros` STALE.
2. `just test-lane-contracts` ran in that state. The
   `cmake-configure-verdict` rows `cmake_platform_threadx_requires_board` and
   `nano_ros_node_register_*` configured, cmake called `nros`, `nros` refused
   (`in-tree nros CLI is STALE — its sources changed since it was built`), and
   the builder RECORDED that refusal as the row's verdict (`exit=1`, stderr =
   the refusal).
3. The edits were committed, `just setup-cli` rebuilt the CLI, and
   `nros ws rmw-dispatch --known` ran clean.
4. `just test-lane-contracts` again: `reused … inputs unchanged (.inputsig)`
   for both rows, and two tests failed, asserting on the stale-CLI text:

   ```
   cmake_platform_threadx_requires_board: expected the FATAL_ERROR message to mention NROS_BOARD; got:
     CMake Error at cmake/NanoRosRmwDispatch.cmake:325 (message):
       nros_rmw_known: `nros ws rmw-dispatch --known` failed (1).
       Error: in-tree nros CLI is STALE — …
   nano_ros_node_register_rejects_unqualified_class: expected the RFC-0057 qualified-class diagnostic, got:
     … `nros codegen source-language` could not name the language … in-tree nros CLI is STALE …
   ```

5. `NROS_COMPILE_CHECK_REBUILD=1 just test-lane-contracts`: 443/443 pass.

## Cause

`cc_row_reusable` (`scripts/build/compile-check-fixtures.sh`) reuses a row when
`compile-check-signature.sh` matches. The signature covers the CLI's CODEGEN
fingerprint, not the CLI source stamp the stale refusal is keyed on. A rebuild
that leaves codegen output unchanged therefore leaves the signature unchanged,
and a verdict row's success marker is "a `.verdict` exists" whatever it says.
A verdict is ABSORBING (issue 0445's shape): it records the failure the row is
meant to observe and an environmental refusal identically.

## Fix direction

A verdict whose stderr is the CLI's stale refusal is not a verdict about the
row: mark the row `.build-failed` instead of recording it (so reuse cannot
fire), or fold the CLI source stamp into the signature of every row whose
configure invokes `nros`. The first is narrower and keyed on the one message
that cannot be a legitimate verdict.

Workaround: `NROS_COMPILE_CHECK_REBUILD=1`.

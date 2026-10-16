---
id: 1926
title: "A compile-check VERDICT recorded while the CLI refused as STALE is reused after `setup-cli`, so two `test-lane-contracts` tests stay red against a fresh CLI"
status: resolved
type: bug
severity: medium
area: [ci, testing, tooling]
related: [issue-0445, issue-1620, issue-0196]
found: 2026-10-16
---

## Measured

Phase-504 branch (`refactor/cpp-headers-rclcpp`), 2026-10-16:

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

## Resolution

Fixed the narrower way the fix direction preferred, at the one door every
verdict builder records through, plus the reuse path that would otherwise keep
serving a refusal recorded before the fix.

- `scripts/build/cli-usable.sh` holds the CLI's self-refusal markers as ONE
  list (`NROS_CLI_REFUSAL_MARKERS`: the STALE refusal and the
  foreign-to-this-checkout refusal, both printed by `stale_guard.rs` before
  any command is dispatched) and `nros_cli_refusal_in`, which matches them with
  whitespace collapsed, since cmake re-indents and wraps a FATAL_ERROR message.
- `_cc_write_verdict_stamp` (`scripts/build/compile-check-fixtures.sh`) is the
  only writer of a `.verdict` for all four verdict builders
  (`cargo-check-verdict`, `cmake-configure-verdict`, `cxx-syntax-verdict`,
  `cxx-compile-verdict`). It now refuses to record captured output, a
  prelude's included, that carries a refusal. The BUILD fails as an
  infrastructure failure (like an absent cmake): no `.verdict`, `.build-failed`
  marked, remedy `just setup-cli` printed.
- `cc_row_reusable` refuses to reuse any row whose `.verdict` on disk carries a
  refusal. The check runs after the per-`output` arms, because every verdict
  row declares `output = ".verdict"`; a first placement inside the `""` arm
  never fired on the real row, which the symptom run caught.

The alternative, folding the CLI source stamp into every signature, was not
taken: every CLI edit would then re-run every verdict row, to answer a
question that only the refusal text can actually answer.

Sweep: `git grep -nE '>[[:space:]]*"[^"]*/\.verdict"' -- 'scripts/*' 'just/*'`
finds exactly one writer, inside the door. `VERDICT_COMPILE_CHECK_BUILDERS` in
`fixtures-manifest.py` lists the same four builders, and no other builder
(west, workspace, zephyr) records a tool's output as a verdict.

Gate `just check verdict-cli-refusal` (fast lane, no build) checks four things.
Each marker must occur verbatim in `stale_guard.rs`. The stamp must have one
writer. The REAL `_cc_write_verdict_stamp` / `cc_row_reusable` bodies, lifted
from the builder, must refuse a cmake-wrapped STALE refusal and a foreign
refusal in a prelude, and must record a genuine diagnostic. A poisoned verdict
must not be reused while a clean twin under the same stubbed signature is.
Its negative control runs on the normal path first: it drops a marker from a copy of the producer and requires a red.

Mutation (each reverted with `cp` and checked with `cmp`):

| mutation | gate |
| --- | --- |
| writer's refusal check disabled | red: refusal RECORDED (stale + foreign) |
| reuse guard disabled | red: poisoned verdict REUSED (both `output` shapes) |
| `stale_guard.rs` refusal reworded | red: marker not in producer |
| a second `.verdict` writer added | red: one-writer rule |
| restored | green |

Symptom, row `cmake_platform_threadx_requires_board`. The CLI was made STALE
with one appended comment in a CLI test file, the row was built, the edit was
reverted (CLI fresh), and the row was built again before the test ran:

| builder | stale build | fresh build | test |
| --- | --- | --- | --- |
| origin/main | recorded `.verdict` (exit=1, stderr = STALE refusal) | `reused … inputs unchanged` | FAIL ("expected … NROS_BOARD") |
| this fix, poisoned verdict on disk | — | `not reusing … CLI refusing to run`, re-recorded | PASS |
| this fix | build rc=1, refusal not recorded, `.build-failed` | recorded `.verdict` (exit=1) | PASS |

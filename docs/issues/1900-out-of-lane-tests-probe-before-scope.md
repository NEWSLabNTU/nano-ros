---
id: 1900
title: "In a `linux`-scoped run, 28 tests of other platforms probe the host before their lane check, so they go red or bare-skip instead of `lane_skip!`; 12 hand-written `[SKIPPED]` panics survive in `logging_smoke.rs`"
status: open
type: bug
area: [testing, ci]
severity: medium
found: 2026-10-15
related: [1758, 1685, 0584, 0571, 0357, 0196, 1525]
---

## The rule this breaks

Issue 1758: a test never skips because something is missing; it fails. What a
run does not attempt is decided by its scope (`NROS_TEST_COORDS`,
`NROS_TEST_UNCLAIMED`), and the scope check comes **before any probe of the
host**. A test of a platform the lane did not select must reach
`lane_skip!` (`[SKIPPED:lane]`) whatever tools the host has.

## How `just test linux` scopes its run

`just test linux` (`justfile`, `_test-scope`) runs `test-all` with
`NROS_FIXTURE_LANE=linux`, `NROS_TEST_COORDS=<nros_scope_coords_file linux>`
and `NROS_TEST_UNCLAIMED=<nros_lane_unclaimed linux>` (empty: the lane claims
everything). It does **not** set `NROS_TEST_SCOPE`. Nothing else does either:

```
$ git grep -n 'NROS_TEST_SCOPE' -- justfile 'just/*' '.github/workflows/*' 'scripts/*.sh' 'scripts/**/*.sh' | grep -v '^\S*:\s*#'
justfile:2692:    done < <(bash scripts/test/lane-filter.sh "${NROS_TEST_SCOPE:-all}")
```

So `lane-filter.sh` gets `all` and prints no exclusions. The name-token filter
(issues 0357, 0571) no longer narrows any run. Every test of every platform is
selected, and only a coordinate check inside the test can deselect it. The
coordinate file for `linux` holds 10 rows, all `linux,*`.

## Measured

Tree at `e8400d5a92` plus a fresh `just build-test-fixtures lane=linux` (rc 0).
The runner below is `tmp/pissues/run-linux-scope.sh` (not committed). It exports
exactly the three variables `_test-scope linux` exports and runs
`cargo nextest run -p nros-tests --no-fail-fast`. The filter selects every test
the old `lane-filter.sh linux` would have excluded. These are the tests that
the coordinate scope alone now has to deselect:

```
(binary(~esp32) or binary(~freertos) or binary(~fvp) or binary(~nuttx) or binary(~px4)
 or binary(~qemu) or binary(~threadx) or binary(~zephyr))
or ((not test(~tests::)) and (test(~esp32) or test(~Esp32) or … or test(~zephyr) or test(~Zephyr)))
```

Each failure was classified by the first line of its panic.

### This host (has qemu-system-arm/riscv64, the ThreadX and FreeRTOS trees; no NuttX tree)

```
Summary [34.364s] 182 tests run: 14 passed (1 slow), 168 failed, 787 skipped
Counter({'LANE': 165, 'UNMET': 2, 'BARE-SKIP': 1})
BARE-SKIP | logging_smoke     | logging_smoke_nuttx_qemu_arm_emits_every_severity | logging_smoke.rs:156 | [SKIPPED] NuttX source tree not found
UNMET     | entry_build       | entry_demos_build::case_1_nuttx                    | entry_build.rs:66    | [UNMET PRECONDITION] no NuttX kernel tree provisioned …
UNMET     | integration_nuttx | nuttx_integration_shell_smoke                     | integration_nuttx.rs:29 | [UNMET PRECONDITION] no NuttX kernel tree provisioned …
```

### The same run with the emulators and RTOS trees hidden

`QEMU_SYSTEM_ARM`, `QEMU_SYSTEM_RISCV64`, `QEMU_SYSTEM_RISCV32`, `THREADX_DIR`
and `FREERTOS_DIR` were set to `/nonexistent/...`. These are the first rungs of
`qemu_system_path` and `nros_build_paths::locate`, so this is what a Linux
runner without those tools sees.

```
Summary [2.579s] 182 tests run: 9 passed, 173 failed, 787 skipped
Counter({'LANE': 145, 'UNMET': 24, 'BARE-SKIP': 4})
```

| file | test(s) | first verdict |
| --- | --- | --- |
| `logging_smoke.rs` | `…_qemu_baremetal_mps2…`, `…_freertos_mps2…`, `…_nuttx_qemu_arm…`, `…_threadx_linux_captures_stderr` | bare `[SKIPPED]` (×4) |
| `emulator.rs` | `test_qemu_{all_tests_pass,cdr_serialization,lan9118_driver,node_api,output_format,type_metadata,wcet_benchmark}` | `[UNMET]` (×7) |
| `zephyr_cortex_m_qemu.rs` | `zephyr_cortex_m_{c,cpp,rust}_zenoh_pubsub_e2e`, `…_c_boot_report_names_arena_exhaustion` | `[UNMET]` (×4) |
| `qemu_patched_binary.rs` | all 3 | `[UNMET]` (×3) |
| `freertos_posix.rs` | `freertos_posix_{c,cpp}_entry_delivers_over_cyclonedds` | `[UNMET]` (×2) |
| `entry_build.rs` | `entry_demos_build::case_1_nuttx`, `::case_2_threadx_linux` | `[UNMET]` (×2) |
| `platform.rs` | `test_qemu_cortex_m3_available`, `test_qemu_semihosting_support` | `[UNMET]` (×2) |
| `port_templates_e2e.rs` | `cpp_port_minimal_publisher_publishes_on_{freertos,zephyr}` | `[UNMET]` (×2) |
| `freertos_qemu.rs` | `an536_cyclonedds_cpp_entry_delivers_within_the_dds_heap_default` | `[UNMET]` |
| `integration_nuttx.rs` | `nuttx_integration_shell_smoke` | `[UNMET]` |

That is 28 tests in 10 files. On this host the same 28 resolve 3 red and 25
otherwise: some reach a resolver and `lane_skip`, and 5 **pass**
(`platform::test_qemu_*` ×2, `qemu_patched_binary` ×3). A test of a
deselected platform passing is the same defect. Its verdict depends on the
host, never on the scope.

The two sites named when this was found, in detail:

- `entry_build.rs`: `entry_demos_build` calls `platform.require()` first.
  `Platform::require()` probes `nuttx::is_nuttx_available()` /
  `threadx_linux::is_threadx_available()` and calls `unmet!`. The lane check
  happens only later, inside `nuttx::require_entry_binary`.
- `logging_smoke.rs`: 12 literal `panic!("[SKIPPED] …")` sites (lines 102,
  105, 128, 131, 156, 159, 162, 167, 199, 242, 245, 283). Four tests have no
  scope check before them. Two do (`threadx_riscv64`, `esp32_qemu` call
  `require_platform_in_lane` first), so their `[SKIPPED]` (199, 283) is now
  only the wrong marker for an in-scope miss, which should be `unmet!`. Line
  283 is the ESP32 test: `#[ignore]`d and dormant (issue 1525), mentioned for
  completeness and not part of this fix.

`git grep '\[SKIPPED'` over `packages/testing/**/*.rs` minus `[SKIPPED:lane]`
finds those 12 and the marker definitions and self-tests in
`nros-tests/src/lib.rs`, nothing else.

## Why no gate caught it

- **`check-skip-budget`** (`scripts/test/check-skip-budget.py`) is a runtime
  junit check. It sees only what ran on this host, and only skips: a bare
  `[SKIPPED]` here is caught (that is how `logging_smoke_nuttx` surfaced), but
  an `[UNMET PRECONDITION]` is a plain red to it, and a probe that passes on
  this host is invisible. 25 of the 28 do not show on a host with qemu and
  the RTOS trees.
- **`check-lane-scope-consumers`** requires a `matrix::CELLS` consumer to be
  "reachable by the lane filter" (file name carries a platform token) or to
  call `lane_scope::admits`. Its first branch assumes `lane-filter.sh` narrows
  the run. As measured above, no runner sets `NROS_TEST_SCOPE`, so that filter
  narrows nothing. The gate's rule is satisfied by a mechanism that no longer
  runs, and it only reads files that iterate `CELLS` anyway. This is the 0196
  reach shape: the rule is "deselect before probing", and the gate checks a
  proxy for it.
- **`check-skip-marker-matching`** keeps hand-written skip **matching** out of
  the tree. It accepts **producing** the marker by design: its self-test
  asserts `not line_offends('panic!("[SKIPPED] {msg}")')`. Issue 1758 retired
  `nros_tests::skip!`, which removed the spelling that most sites used. The
  hand-written `panic!("[SKIPPED] …")` never went through that macro, so it
  survived.

No static gate asks "does the first host probe in a test come after its scope
check?"

## Fix direction

1. Put the scope check first in each of the 10 files:
   `fixtures::lane::require_platform_in_lane(&[PlatformId::…], …)` at the head
   of the test, or inside its local `require_*` helper (`emulator.rs`'s
   `require_qemu`, `freertos_posix.rs`'s `require_freertos`,
   `entry_build.rs`'s `Platform::require`, `zephyr_cortex_m_qemu.rs`'s
   `run_pubsub_cell`).
2. Turn the 12 `panic!("[SKIPPED] …")` sites into `nros_tests::unmet!`, each
   placed after a scope check.
3. Gates:
   - **Static.** A test or test-local helper may not call a host probe
     (`is_*_available`, `sdk_tree`, `qemu_system_*`, `unmet!`, `which`/`have`)
     before a scope call (`require_*_in_lane`, `lane_scope::`, `lane_skip!`).
     A helper that is lane-first by construction (`require_threadx_riscv64`,
     `require_ros2`) counts as a scope call. A heuristic version of this sweep
     (`tmp/pissues/probe_before_scope.py`, not committed) reports 126
     candidate sites and over-reports, because it cannot see that such a
     helper is lane-first. A real gate needs that list stated.
   - **Static, small.** Refuse `"[SKIPPED]"` and `"[SKIPPED:` (other than
     `lane`) in a string literal under `packages/testing/**/tests/`. `lane_skip!`
     is the only producer.
   - **Runtime, the real proof.** Run the platform-token filter above under
     the `linux` scope with the emulators hidden, and assert every non-PASS is
     `[SKIPPED:lane]`. It needs no fixture and no tool (2.6 s here), so it can
     sit on the PR lane. It also keeps the token list honest, because the
     tokens come from `PlatformId` the same way `lane-filter.sh` derives them.
4. Retire or rewrite `check-lane-scope-consumers`'s "reachable by the lane
   filter" branch, and `lane-filter.sh linux` itself if nothing will set
   `NROS_TEST_SCOPE` again.

# Phase 511 — model the unmodeled runtime tests, and name the ones that are not cells

**Status (2026-10-23). IN PROGRESS — A1–A6 and D1 open.** Implements RFC-0051 §7c
(amendment 2026-10-22). It follows [phase-510](archived/phase-510-test-config-axis.md),
which bound every Runtime cell to the test that answers it and left 23 test
binaries as `NonCell::Unmodeled`, "a candidate cell, not an exemption".

## Why

`NonCell::Unmodeled` in `packages/testing/nros-tests/src/matrix.rs` holds 23
binaries. Its doc comment says each one is a candidate cell. Reading the 23
shows that is true of about half:

| Group | What it needs | Binaries |
| --- | --- | --- |
| A | Nothing but a cell: workload and axis values already exist | 6 |
| B | A new axis value, or a lane the matrix does not run | 5 |
| C | Interop-shaped, with no `interop::CELLS` row | 1 |
| D | Not a matrix workload: a knob, a platform contract, a single-feature probe | 11 |

Group D rows are not debt the matrix owes. Leaving them under a variant that
says "candidate cell" makes the real debt (A, B, C) unreadable: the list
looks twice as long as it is, and nothing distinguishes "model me" from
"never a cell".

## Work items

### A1 — `parameters_roundtrip`: Linux C and C++ Params examples

Cells `(Linux, C|Cpp, Zenoh, Params, Example)`. A new `Consumer` claims them
ahead of the `(_, _, _, Params, Example) => ParamStoreCarve` arm, which holds
the NuttX and ThreadX-RV64 param-store images.

### A2 — `qos_overrides_runtime_delivery`: Linux Rust Qos example

Cell `(Linux, Rust, Zenoh, Qos, Example)` (the `qos-override-pubsub` fixture).

### A3 — `custom_msg`: Linux Rust CustomMsg example

Cell `(Linux, Rust, Zenoh, CustomMsg, Example)`. The Workspace Rust CustomMsg
row stays BuildOnly; this is the example leaf.

### A4 — `safety_e2e`: Linux Safety examples, three languages

Cells `(Linux, Rust|C|Cpp, Zenoh, Safety, Example)`. The language is the
LISTENER's: the C and C++ cases pair their listener with the Rust safety
talker.

### A5 — `logging_smoke`: Logging examples on seven platforms

Cells `(P, Rust, Zenoh, Logging, Example)` for Linux, QemuBaremetal,
FreertosMps2, NuttxArm, ThreadxLinux, ThreadxRiscv64 and ZephyrNativeSim. No
Example Logging cell existed. The RMW is nominal for the RTOS smoke images,
which never open an executor; the fixture rows are recorded zenoh, and the
cell matches its row. The `QemuBaremetal` arm goes ahead of the
`(QemuBaremetal, _, _, _, Example) => Emulator` catch-all. ESP32 stays out
(issue 1525).

### A6 — `baremetal_run_plan_runtime`: the `nros::main!()` boot road on bare metal

The table recorded it as `EntryPubsub/Workspace`, which is wrong twice: the
fixture is a single crate (an Example-shaped row), and it delivers nothing.
Its node has no publisher; the proof is that boot reaches `Executor::open`.
Phase-510 W3 retiered eleven cells to BuildOnly for exactly that gap, so it
cannot be an `EntryPubsub` Runtime cell.

It gets its own workload, `BoardEntryBoot`, on the `TypedCarrier` precedent:
the subject is a build ROAD (reset vector, `BoardEntry::run_with_deploy`,
`init_hardware`, `Executor::open`, all emitted by the macro), not delivery.
Cell `(QemuBaremetal, Rust, Zenoh, BoardEntryBoot, Example)`.

### D1 — `NonCell::Probe` for the eleven that are not cells

A new variant, `Probe(&'static str)`: runtime, but the subject is a knob, a
platform contract, or one feature checked in one image, which no matrix
workload describes. The string says what it probes. Rows:

- Knobs: `image_config_workspace`, `image_env_cmake_knob`, `log_arena_knob`
- Platform contracts: `condvar_contract_rtos`, `pool_exhaustion_threadx_linux`
- No RMW: `borrowed_e2e`, `nvidia_ivc_mock_wire_format`, `trigger_conditions`
- One feature: `contract_monitor_parity`, `sim_time_clock_e2e`, `violation_channel_e2e`

After D1, `Unmodeled` means only "a cell is owed", which is what its doc
comment already claims.

### Acceptance

- `matrix_fixture_coverage` and `ci_lane` pass, with the tier-cover numbers
  re-measured and recorded (RFC-0061 table, `just/ci.just`).
- Each new consumer's `cases_bound_to_matrix_cells` passes.
- Every new Runtime cell's test runs green at least once on fresh fixtures,
  native solo and each RTOS platform it names.
- `Unmodeled` holds exactly the B and C rows (6).

## Not in this phase

- **B** — `freertos_qemu` (mps3-an536 is not a platform value),
  `fvp_runtime_ws` / `fvp_smoke` (FVP runs only in `verify-fvp-runtime`),
  `px4_xrce` (Px4 × XRCE), `esp32_emulator` (issue 1525).
- **C** — `cyclone_slirp_pair` to an `interop::CELLS` row.

## Related

- RFC-0051 §7c; [phase-510](archived/phase-510-test-config-axis.md) W3.
- RFC-0061 (the tier covers the new cells and workload join).

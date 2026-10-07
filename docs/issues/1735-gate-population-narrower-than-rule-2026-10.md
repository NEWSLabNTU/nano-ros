---
id: 1735
title: "Seven gates read a narrower population than the rule they state (W1/W5 class, 2026-10-07 re-audit)"
status: open
type: bug
area: [tooling, ci]
severity: medium
found: 2026-10-07
related: [phase-472, issue-0196, issue-1614, issue-1736]
---

## What the re-audit measured

The 2026-10-07 gate-reach re-audit
([audit-findings-2026-10-07](../development/audit-findings-2026-10-07.md))
re-ran phase-472's method over every gate changed since 2026-09-28. Each row
below is a mutation the gate PASSED (rc 0) with a control — the same defect
where the gate reads — that it FAILED (rc 1). Each restored clean.

| gate | mutation that passes | control that fails |
| --- | --- | --- |
| `cmake-image-policy` | delete `nros_apply_panic_policy` from `examples/templates/rclcpp-compat-smoke` — it links the umbrella transitively through `<msg>__nano_ros_cpp`, and image detection keys on the LITERAL `NanoRos::NanoRos(Cpp)` | same, plus a literal `NanoRos::NanoRosCpp` |
| `image-paths-apply-policy` | delete the policy call from the `rv-virt-threadx` board seam (carrier-linked via `nros_declare_rust_runtime_carrier`, issue 1467) | raw `add_executable` in `examples/native/c/custom-platform` |
| `config-header-single-writer` | a SHELL writer `cp … nros_cpp_config_generated.h` (population: "173 cmake file(s)"; also only `cmake/ packages/ zephyr/ integrations/` `.cmake` + `packages/**/CMakeLists.txt`) | a CMake `file(COPY_FILE …)` writer |
| `cargo-dir-knob-key` | `nros_knob_key_fields` ROAD 1 (the resolver registry `NROS_RESOLVED_*`) appends knob names without values — the probe exercises road 2 only and prints "negative control collided as required" | the same break on road 2 |
| `rustc-wrapper-staticlib` | cargo's own key `[build] rustc-wrapper = "sccache"` in the tracked root `.cargo/config.toml` | workflow env `RUSTC_WRAPPER: sccache` |
| `repr-memory-agreement` | swap `size`/`capacity` in the C++ pack's `nros_cpp_heap_str_t` (CLI rebuilt) — the heap STRING container is never measured | the same swap on the heap SEQUENCE helper (CLI rebuilt) |
| `skippable-tests-tolerant` | a bare `cargo nextest run --test xrce_ros2_interop` in a workflow step | **FIXED in the audit PR** (onto `workflow_commands.ci_files`) |

## Notes per row

- `image-paths-apply-policy` and `cmake-image-policy` state ONE rule (issue
  0719) with two different populations; issue 1614's W5 fix (carrier in
  `LINKS_NROS`) landed on the second only. One gate should go, or both should
  share a predicate.
- `rustc-wrapper-staticlib`: `CARGO_BUILD_RUSTC_WRAPPER` is the env spelling of
  the same key and is equally unread.

## Fix direction

Population from `scripts/lib/file_kinds.py` / `workflow_commands.ci_files`
(W1/W5), never a directory list or one literal spelling; give each gate the
normal-path selftest row that would have caught its mutation.

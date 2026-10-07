---
id: 1735
title: "Seven gates read a narrower population than the rule they state (W1/W5 class, 2026-10-07 re-audit)"
status: resolved
type: bug
area: [tooling, ci]
severity: medium
found: 2026-10-07
related: [phase-472, issue-0196, issue-1614, issue-1736]
resolved_in: "gate-reach follow-up, item 4"
---

## What the re-audit measured

The 2026-10-07 gate-reach re-audit
([audit-findings-2026-10-07](../../development/audit-findings-2026-10-07.md))
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

## Resolution

| gate | change (helper) | mutation | old rc | new rc |
| --- | --- | --- | --- | --- |
| `cmake-image-policy` | image detection also reads a TRANSITIVE link through a generated `<pkg>__nano_ros_c(pp)` library and the `idf_component_register` image shape; per `function()`/`macro()` SCOPE as well as per file | delete `nros_apply_panic_policy` from `examples/templates/rclcpp-compat-smoke` | 0 (and `image-paths-apply-policy` 0) | 1 |
| `image-paths-apply-policy` | **RETIRED** into `cmake-image-policy` — one rule (issue 0719), one gate, one population (`file_kinds` cmake). Its scope reading and its IDF shape moved with it; `.config/gate-registry-baseline.txt` loses the name deliberately | delete the policy call from the `rv-virt-threadx` board seam | old shell gate 0; `cmake-image-policy` 1 | 1 |
| `config-header-single-writer` | population by KIND (`file_kinds` cmake, was four directories) plus shell/just/make WRITERS — `cp`/`install`/`mv`/`ln`/`rsync`/`tee` or a `>` whose destination is a sizes header, the verb itself CODE (`comments.code_mask`), a temp-rooted destination being a selftest fixture | `cp "$out/nros_cpp_config_generated.h" "$inc/nros/…"` in `scripts/build/cargo.sh` | 0 | 1 |
| `cargo-dir-knob-key` | the probe drives ROAD 1 (`-DNROS_PROBE_RESOLVED=K=V` populates `NROS_RESOLVED_*` as the resolver does): values separate, key text carries the value, road 1 outranks env | `nros_knob_key_fields` road 1 appends the name without its value | 0 | 1 |
| `rustc-wrapper-staticlib` | cargo's own key `rustc-wrapper` / `build.rustc-wrapper` (TOML), and `CARGO_BUILD_RUSTC_WRAPPER` (env) | `[build] rustc-wrapper = "sccache"` in the root `.cargo/config.toml` | 0 | 1 |
| `repr-memory-agreement` | a third storage arm, `heap-strings`, written by the gate into its own work dir (NOT the corpus — the corpus feeds `codegen_fingerprint`, and moving it stales every workspace fixture); the arm FAILS if its output holds no `nros_cpp_heap_str_t` | swap `size`/`capacity` in the C++ pack's `nros_cpp_heap_str_t` (CLI rebuilt, then rebuilt clean) | 0 | 1 |
| control | | a CMake `file(COPY_FILE …)` writer | 1 | 1 |

### Found by the widened gates

- `examples/templates/topic-state-monitor-port` linked the runtime through its
  message libraries and applied no policy — fixed as its sibling
  `rclcpp-compat-smoke` does (configure verified against `/opt/ros/humble`).
- `nano_ros_node_register`'s NuttX/ThreadX carriers delegate the policy to a
  link seam, and only three of 17 seams apply it: filed as issue 1742 (needs a
  ruling). The gate reads a seam call as delegation and says so.

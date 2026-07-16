# Phase 293 — typed C/C++ bindings for workspace custom messages (issue #212)

Status: **Complete — 2026-07-16** (W1–W4 same-day; residual std_msgs raw-CDR components carved into issue #218) · Resolves issue #212 · Implements the
RFC-0023/0033 codegen-SSoT rule for the last in-tree violation · Related:
RFC-0048 (find_package(msg) validate-only, verbs drive codegen), issue #203
(the cpp mixed-generation regression site the raw-CDR examples dodge).

**Goal.** The `ws-custom-msg-{c,cpp,mixed}` workspaces hand-roll CDR for
`custom_msgs/Reading` (manual encapsulation header, fixed byte offsets,
hand-typed `custom_msgs::msg::dds_::Reading_` string) because the C/C++
lanes never got typed bindings for workspace-local msg packages. The
capability EXISTS — `nros generate c` emits full per-message C typesupport
(struct + `_serialize`/`_deserialize` + `_inline` stream variants), every
native/RTOS C example consumes it for `std_msgs`, and the Rust custom-msg
workspace already uses generated `custom_msgs::msg::Reading`. This phase is
wiring + example modernization, NOT a new generator.

**Audit context (2026-07-16 J1).** Copy-out examples teach hand-rolled wire
format — offset drift on any msg change; violates Architecture §5
("messages are never hand-written"). Both CMakeLists frame the raw path as
deliberate edge-dodging: "dodges any cpp codegen edge AND the
double-`builtin_interfaces`-glue a typed cpp interface link can hit" — that
edge is #203's residual ("the cpp pkg in the mixed generation is the
standing regression site").

## Waves

### W1 — prove/wire C generation for a workspace-local msg package — DONE
- [x] Spike: in `ws-custom-msg-c/src/reading_talker_pkg`, add
      `ament_target_dependencies(reading_talker custom_msgs)` (the exact
      shape native C examples use for `std_msgs`) and build the workspace.
      Expected per RFC-0048: `find_package(custom_msgs)` stub validates, the
      `nano_ros_add_node` verb drives `LANGUAGE C` generation via the
      `NROS_INTERFACE_SEARCH_PATH`/workspace-src discovery
      (`nros_find_interfaces` already resolves workspace roots).
- [x] Fix whatever the spike surfaces — TWO gaps found + fixed:
      (1) `nros codegen resolve-deps` only knew the ament env index + bundled
      interfaces — `load_index_with_fallback` (cargo-nano-ros) now layers
      `NROS_INTERFACE_SEARCH_PATH` roots FIRST (workspace shadows ament
      shadows bundled, mirroring the cmake smart-stub);
      (2) the cmake var never reached the CLI child process —
      `nros_find_interfaces` now injects it via `cmake -E env` (caller-
      exported env still wins). No example-local workarounds; the
      `ament_target_dependencies` lines from the spike proved unnecessary
      (`nano_ros_add_node` already generates the declared closure via
      `_nros_generate_declared_interfaces`).
      Original text: fix whatever the spike surfaces (stub not creating the
      `custom_msgs::custom_msgs` target for a src-sibling package, search
      path not defaulted to the workspace `src/`, generation not triggered
      for non-AMENT-prefix packages, …). Keep fixes in the cmake seam /
      codegen discovery — no example-local workarounds.

### W2 — rewrite ws-custom-msg-c onto typed bindings — DONE
- [x] `ReadingTalker.c`: replace the hand CDR block with the generated
      `custom_msgs_msg_reading` struct + `_serialize`; type name from the
      generated header, not a string literal.
- [x] `ReadingListener.c`: same, `_deserialize` on the raw sub callback.
- [x] CMakeLists + doc comments updated (raw rationale retired).
- [x] E2E: manual pair 9 sent / 9 heard with correct ramping values;
      `c_custom_msg_delivers_cross_process` green on rebuilt fixtures.

### W3 — cpp lane — DONE (the glue edge is DEAD)
- [x] The feared "double-`builtin_interfaces`-glue" failure does NOT
      reproduce — as #203's resolution predicted, the 263-A4 idempotency +
      269 header-mirror repairs killed it. Typed cpp interface link in a
      workspace pkg builds clean; no generation/cmake fix needed.
- [x] Rewrote `ws-custom-msg-cpp` talker/listener onto typed
      `Publisher<custom_msgs::msg::Reading>` + `bind_subscription<Reading>`
      member callback (the ReadingTag hand-struct died).
- [x] E2E: manual pair 9/9 correct values;
      `cpp_custom_msg_delivers_cross_process` green.

### W4 — mixed workspace + closure — DONE (scope note)
- [x] `ws-custom-msg-mixed`: node sources were byte-identical copies of the
      C workspace's — W2's rewritten sources copied in; builds + e2e green
      (`mixed_custom_msg_delivers_cross_process`).
- [x] Sweep ran — CUSTOM-msg raw CDR is gone. Finding: the same antipattern
      survives in the STD-msgs raw components (workspaces/c, ws-qos-*,
      ws-realtime-* C pkgs, mixed templates) — the phase-257 raw-component
      convention itself. Out of #212's scope; filed as **issue #218**.
- [x] Issue #212 resolved + archived. All four custom-msg lanes green
      (custom_msg suite + c/cpp/mixed workspace e2e = 10/10).

## Acceptance
- All three custom-msg workspaces run e2e on GENERATED bindings in every
  language ✓; the raw-CDR pattern for CUSTOM msgs survives nowhere (the
  std_msgs raw components are #218's scope).
- A field added to `Reading.msg` requires only regeneration — no example
  source edits beyond using the new field.
- `just ci` green; affected fixture families rebuilt + their tests pass.

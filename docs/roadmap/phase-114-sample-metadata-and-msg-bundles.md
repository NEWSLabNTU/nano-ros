# Phase 114: Sample Metadata + Curated Message Bundles

**Goal:** Adopt Zephyr's `sample.yaml` shape per example so test runners and humans get one source of truth, and ship a precompiled "common ROS 2 messages" bundle so users don't run codegen for every package they depend on.

**Status:** Not Started
**Priority:** Medium
**Depends on:** Phase 78 (colcon-nano-ros), Phase 111 (`nros` CLI — `nros run` consumes sample.yaml)
**Related:** `docs/research/sdk-ux/SYNTHESIS.md` UX-24, UX-25

---

## Overview

Two related additions, both about cutting ceremony:

1. **`sample.yaml` per example.** Every example today implicitly carries metadata — supported boards, expected stdout regex, integration tags — scattered across `Cargo.toml`, `nros_tests/` fixture lists, and tribal knowledge. Zephyr's Twister consumes a single `sample.yaml` per sample. nano-ros adopts the same format verbatim: same keys, same parser-compatible shape.
2. **`nros-msgs-common` bundle.** Today users run `cargo nano-ros generate-{rust,c,cpp}` per `package.xml`. micro-ROS Arduino ships ~100 pre-baked message types (`std_msgs`, `geometry_msgs`, `sensor_msgs`, `nav_msgs`, `tf2_msgs`, `lifecycle_msgs`, `action_msgs`, `example_interfaces`). nano-ros should ship the same bundle as a depend-and-link library; codegen still flows for *custom* messages.

---

## Architecture

### A. `sample.yaml`

One file per example, mirroring `zephyr-workspace/zephyr/samples/hello_world/sample.yaml`:

```yaml
sample:
  description: Publishes std_msgs/Int32 on /chatter
  name: nros_freertos_c_zenoh_talker
common:
  tags: nros nros-c rmw-zenoh
  harness: console
  harness_config:
    type: one_line
    regex:
      - "Published: 5"
tests:
  sample.nros.freertos.c.zenoh.talker:
    integration_platforms:
      - mps2/an385
    extra_args: NROS_RMW=zenoh NROS_LANG=c
```

Consumers:

- **Twister** (Zephyr examples) — already understands the format.
- **`nros run`** (Phase 111) — reads `sample.yaml` to know what board to run on, what regex marks success, what stdout to capture.
- **`just test-all`** — replaces the current per-test `nros_tests/` fixture lists with a discovery walk over `sample.yaml` files.
- **`book/src/getting-started/*.md`** auto-generated example tables read from sample.yaml descriptions.

### B. `nros-msgs-common`

New crate `packages/interfaces/nros-msgs-common/` that re-exports the standard ROS 2 message families. Pre-generates Rust, C, and C++ bindings at workspace build time using `cargo nano-ros generate-*`. Cargo feature flags select per-family inclusion to keep flash size in check on small targets:

```toml
[features]
default = ["std-msgs", "geometry-msgs", "sensor-msgs"]
std-msgs       = []
geometry-msgs  = ["std-msgs"]
sensor-msgs    = ["std-msgs", "geometry-msgs"]
nav-msgs       = ["geometry-msgs"]
tf2-msgs       = ["geometry-msgs"]
lifecycle-msgs = []
action-msgs    = []
example-interfaces = ["std-msgs"]
```

C/C++ side: ships a `find_package(NanoRosMsgsCommon)` cmake target. Consumer adds `target_link_libraries(my_app PRIVATE NanoRos::MsgsCommon)`. Headers under `<std_msgs/msg/int32.h>`, `<geometry_msgs/msg/twist.h>`, etc. — same paths as upstream rosidl C codegen so existing rclc/rclcpp code ports without `#include` changes.

Custom messages still flow through `cargo nano-ros generate-*` / `nros generate` as a sibling library. The bundle is *additive*.

---

## Work Items

### A — sample.yaml

- [ ] **114.A.1** Define the canonical `sample.yaml` schema in `docs/reference/sample-yaml-schema.md`. Subset of Twister format we promise to support.
- [ ] **114.A.2** Walker tool `tools/sample-walker.rs` (or in `nros-cli-core`) that discovers all `sample.yaml` files under `examples/` and emits a JSON catalog.
- [ ] **114.A.3** Backfill `sample.yaml` for every existing example. ~50 files. Include description, tags, integration_platforms, harness regex.
- [ ] **114.A.4** `nros run` reads `sample.yaml` to find the right board + success regex (Phase 111 hookup).
- [ ] **114.A.5** `just test-all` migrates from per-test fixture lists in `nros_tests/` to discovery via `sample-walker`.
- [ ] **114.A.6** `cargo nano-ros init` (Phase 111) emits a `sample.yaml` stub.
- [ ] **114.A.7** Auto-generated `book/src/examples.md` index page from the catalog.

### B — nros-msgs-common

- [ ] **114.B.1** Create `packages/interfaces/nros-msgs-common/`. Workspace member. `Cargo.toml` with feature-gated re-exports.
- [ ] **114.B.2** `build.rs` invokes `cargo nano-ros generate-rust` for each enabled family. Cache outputs under `OUT_DIR`.
- [ ] **114.B.3** CMake target `NanoRos::MsgsCommon` exposing C headers + static lib per RTOS.
- [ ] **114.B.4** C++ wrapper `nros::msgs::common` namespace.
- [ ] **114.B.5** Migrate existing examples to depend on the bundle when applicable; keep custom-message examples as the codegen-flow demonstrators.
- [ ] **114.B.6** Phase 111 release pipeline pubishes `nros-msgs-common` to crates.io.
- [ ] **114.B.7** Phase 111 Arduino / IDF / PIO bundles include the C bundle by default.
- [ ] **114.B.8** Document feature flags + flash-cost trade-offs in `book/src/user-guide/message-generation.md`.

**Files:**
- `docs/reference/sample-yaml-schema.md` (new)
- `tools/sample-walker.rs` *or* `packages/codegen/packages/nros-cli-core/src/sample_walker.rs` (new)
- `examples/**/sample.yaml` (~50 new files)
- `packages/interfaces/nros-msgs-common/` (new crate)
- `cmake/NanoRosMsgsCommon-config.cmake.in` (new)
- `book/src/examples.md` (auto-generated)
- `book/src/user-guide/message-generation.md` (update)

---

## Acceptance criteria

- Every example has a `sample.yaml`. CI fails if one is missing.
- `nros run examples/qemu-arm-freertos/c/zenoh/talker/` reads `sample.yaml` for both the board and the success regex; no per-platform hardcoding.
- `cargo add nros-msgs-common --features "std-msgs geometry-msgs"` works on a fresh project (depends on Phase 111 crates.io publish).
- A C/C++ user adds `find_package(NanoRosMsgsCommon REQUIRED)` + `target_link_libraries(... NanoRos::MsgsCommon)` and includes `<std_msgs/msg/int32.h>` without running codegen.
- Flash cost of `default` feature set on Cortex-M3 ≤ 8 KB.

## Notes

- `sample.yaml` schema is a subset of Twister's. We do not promise to support every Twister key; we promise the keys we list are interpreted identically.
- `nros-msgs-common` has version pinning to a ROS distro (Humble for now). Iron support gated on Phase 41.
- Risk: per-example `sample.yaml` adds ~50 small files. Auto-validate via JSON schema in CI to keep them honest.

# Phase 124 Remaining Failure Groups

Date: 2026-05-15

This document extracts the remaining Phase 124 failure work into parallelizable
groups. Historical run details remain in
`docs/roadmap/phase-124-test-triage-2026-05-14.md`.

## Current Baseline

Recent sync/fix context:

- Parent `main` was merged from `origin/main` at merge commit `255046a2`.
- `packages/codegen` was updated to submodule commit
  `3069524eb1e4b8d33da0de77a9e83df7681aac36`.
- ESP32 subscriber creation OOM was fixed in
  `8094047c fix(esp32): avoid subscriber heap allocation`.
- The latest focused ESP32 run now reaches `Subscriber declared` and
  `Waiting for messages...`; remaining ESP32 failures are message delivery,
  not heap allocation.

Because Phase 126 codegen/orchestration changes landed after the older full
Phase 124 snapshots, refresh the full matrix before treating historical counts
as current:

```bash
just ci
just build-all
just test-all
```

## Group A: ESP32 Zenoh Delivery

Scope:

- `esp32_emulator::test_esp32_talker_listener_e2e`
- `esp32_emulator::test_esp32_to_native`
- `esp32_emulator::test_native_to_esp32`

Current signal:

- ESP32 listener/talker build and boot checks pass.
- Listener reaches `Subscriber declared` and waits.
- No messages are delivered across ESP32-to-ESP32, ESP32-to-native, or
  native-to-ESP32 paths.

Suggested owner output:

- Determine whether the break is router discovery, TCP session open, publish
  path, receive path, or smoltcp polling cadence.
- Include QEMU logs, `zenohd` logs, and one minimal focused fix or a narrowed
  failure cause.

Focused commands:

```bash
cargo build --release
just esp32 test --no-capture
```

Run the `cargo build --release` command from both:

- `examples/qemu-esp32-baremetal/rust/zenoh/listener`
- `examples/qemu-esp32-baremetal/rust/zenoh/talker`

## Group B: RTOS/QEMU Platform E2E

Scope:

- FreeRTOS runtime/E2E
- NuttX runtime/E2E
- ThreadX Linux/RISC-V runtime/E2E
- bare-metal DDS runtime/E2E
- shared platform DDS runtime/E2E

Current signal:

- Last full `just ci` bucket before the Phase 126 pull had 39 failures here.
- The harness-reported ThreadX-Linux DDS prerequisite miss is an environment
  skip, not a product failure.

Suggested owner output:

- Split failures by platform first.
- For each platform, label failures as fixture build, boot, network setup,
  router/discovery, or protocol handshake.
- Preserve exact QEMU and test harness logs.

Focused commands:

```bash
just build-test-fixtures
just test-all
```

## Group C: Zephyr Runtime/E2E

Scope:

- Zephyr native/host runtime tests
- Zephyr DDS runtime tests
- Zephyr XRCE runtime tests
- Cross-language Zephyr interop cases

Current signal:

- Last full `just ci` bucket before the Phase 126 pull had 29 failures.
- Build/smoke coverage was mostly passing; failures are concentrated in boot,
  runtime handshakes, and message flow.

Suggested owner output:

- Separate host/board boot failures from DDS/XRCE message-flow failures.
- Include `west`, QEMU, and nextest logs.
- Identify whether the failure is common platform startup or backend-specific.

Focused commands:

```bash
just zephyr build-fixtures
just zephyr test --no-capture
```

## Group D: Bare-Metal Zenoh QEMU

Scope:

- RTIC action E2E
- RTIC service E2E
- serial pub/sub E2E

Current signal:

- Last full `just ci` bucket before the Phase 126 pull had 3 failures.
- Native RTIC pattern fixtures were repaired earlier, so these should be
  treated as bare-metal/QEMU-specific until proven otherwise.

Suggested owner output:

- Determine whether failures share session readiness, router timing, serial
  framing, or executor wake behavior.
- Compare against passing native RTIC action/service/pubsub cases.

Focused commands:

```bash
just build-test-fixtures
cargo nextest run -p nros-tests --no-capture rtic
```

## Group E: Native DDS Action

Scope:

- Native DDS action server/client E2E.

Current signal:

- Last full `just ci` bucket before the Phase 126 pull had 1 DDS native action
  failure.
- Zenoh and XRCE action paths have focused passing coverage after earlier
  fixes.

Suggested owner output:

- Capture server/client action logs.
- Compare goal acceptance, feedback, result, and cancellation behavior against
  the passing Zenoh/XRCE action paths.

Focused commands:

```bash
cargo nextest run -p nros-tests --no-capture action
```

## Group F: ROS 2 Lifecycle Interop

Scope:

- Lifecycle full-cycle ROS 2 interop.

Current signal:

- Last full `just ci` bucket before the Phase 126 pull had 1 lifecycle interop
  failure.

Suggested owner output:

- Identify whether failure is graph discovery, transition service availability,
  transition execution, or state observation timing.
- Include ROS 2 CLI/log output and nano-ros process logs.

Focused commands:

```bash
cargo nextest run -p nros-tests --no-capture lifecycle
```

## Group G: Full-Matrix Refresh

Scope:

- Refresh the authoritative counts after the parent/submodule pull and ESP32
  allocation fix.

Current signal:

- Historical counts in the triage doc are useful for direction but stale after
  the Phase 126 pull and the ESP32 allocation fix.

Suggested owner output:

- Produce a fresh table by category.
- Keep failed, skipped, and harness-reported environment skips separate.
- Include nextest run id, JUnit path, and `test-logs/latest/` path.

Commands:

```bash
just format
just ci
just build-all
just test-all
```

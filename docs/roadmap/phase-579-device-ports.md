# Phase 579 — Device ports

**Status (2026-11-18). OPEN: design in review; no item started. Owner: nros
241.** Implements [RFC-0139](../design/0139-device-ports.md): accelerators,
cameras and other job devices as executor event sources, behind a C port ABI
that board packages implement and an opt-in companion package that node
authors use. The ROS-counterpart user API is unchanged.

Each item is one PR, claimed with `just claim phase-579-wN` before it starts.
No item starts before the user accepts RFC-0139.

## Why

A perception chain needs device completions to reach the executor without
holding it. Today a device callback blocks its executor for the whole job,
completion cannot be signalled from an interrupt handler (issue 2696), a
buffer cannot outlive the callback that received it, and no contract names a
device. RFC-0139's Motivation has the measurements and the driver survey.

## Work items

### W1 — Interrupt-safe executor wake

- **Owner:** nros 142.
- **Status:** DONE (2026-11-18, the issue 2696 fix)
- Issue 2696's fix: `NodeWake::signal_from_isr`, an interrupt-safe trigger
  path, a real body for the ISR wake callback, and a wake reachable without
  `alloc` + `rmw-cffi` (or the API stating it is observed at the next spin).
- **Acceptance (amended 2026-11-18):** a guard condition triggered from a
  FreeRTOS interrupt handler wakes a blocked spin on the MPS2 QEMU image, and
  every port's interrupt-safe signal either REFUSES (-1) or is reached by a
  test. A port no test reaches does not claim the signal.
- **As built:** `GuardCondition::trigger_from_isr() -> bool` (Rust),
  `WakeCtx`/`NodeWake::signal_from_isr`, and `BandWaiter::new` taking a
  `wake_from_isr`. FreeRTOS and POSIX are tested (`tests/isr_wake_rtos.rs`,
  83 to 102 us; `c_port_posix_wake.rs`, a real signal handler). mps2 bare
  metal followed (2026-11-19, the same test file, 182 to 247 us), after its
  park stopped reading the wake flag outside the idle's critical section.
  Zephyr followed (2026-11-19, a `k_timer` expiry on native_sim), then
  ThreadX (2026-11-19, the RISC-V `virt` machine timer) and NuttX
  (2026-11-20, a watchdog, over a semaphore park and a handle park); see
  [issue 2706](../issues/archived/2706-isr-wake-ports-untested.md). Only the
  hosted `threadx-linux` port still refuses
  ([issue 2745](../issues/2745-threadx-linux-isr-wake-refused.md)).
  The C/C++ front is W1b.

### W1b — The C and C++ interrupt-safe wake

- **Owner:** unassigned (with W3).
- **Status:** OPEN
- W1 gave Rust `GuardCondition::trigger_from_isr`. C and C++ get no method
  on the ROS-mirrored types: `rcl_guard_condition_t` and
  `rclcpp::GuardCondition` stay thread-only, as stock ROS has them (RFC-0139
  D1). Their interrupt wake is W3's device completion handle
  (`nros_completion_t`, an interrupt-safe "mark slot ready and wake"), on W1's
  plumbing (`WakeCtx::signal_from_isr`).
- **Acceptance:** a C image's interrupt handler wakes a parked executor
  through the completion handle on the FreeRTOS MPS2 QEMU image, as W1's Rust
  cell does.

### W2 — The port ABI header

- **Owner:** unassigned.
- **Status:** OPEN
- `<nros/device_port.h>` (RFC-0139 D3): worker and source ops, the completion
  handle and its two signal functions, the generic job and result, the return
  codes, `abi_version`. Committed bindgen output for Rust; layout checks on
  both sides, as for the custom transport.
- **Acceptance:** the layout check fails when a field is reordered on one side
  only (mutation-tested). `check-abi-bindings` covers the new header.

### W3 — The executor hook and owned slots

- **Owner:** unassigned.
- **Status:** OPEN
- The `Device` entry kind beside `GuardCondition`, registered before the first
  spin into a fixed slot; completion through `WakeCtx::mark_ready`; priority
  through the bound sched context; sampling in LET mode; a completion handle
  as a park handle (file descriptor, Zephyr `k_poll_signal`) through the
  existing park (RFC-0139 D4).
- The owned slot handle over the large-message pool (D6), with `move` and
  release.
- **Acceptance:** with a fake device whose completion is fired from a timer
  interrupt, a continuation runs at its bound priority, ahead of a
  lower-priority ready callback, on the host and on the MPS2 FreeRTOS QEMU
  image. A slot used after `move` asserts in a debug build.

### W4 — The companion package, in three languages

- **Owner:** unassigned.
- **Status:** OPEN
- `nros-device` (crate), `NanoRos::Device` (CMake), `<nros/device.h>`,
  `<nros/device.hpp>` (D7): worker and source registration, stages, slot
  views, the per-class headers for inference, 2D operation, camera frame and
  sensor batch. The C++ implementor base and the Rust implementor trait.
- The mock worker and mock source (RFC-0139 D11), and the port conformance
  tests that any adapter, real or mock, must pass.
- **Acceptance:** one fake device adapter written in C is driven by a C node,
  a C++ node and a Rust node, and one written in Rust by a C node. That proves
  the ABI is the one seam in both directions.

### W5 — Queue policies, data age, trace

- **Owner:** unassigned.
- **Status:** OPEN
- The per-device priority queue, `KeepLatest` and `Fifo { depth }`, capture
  time carried through every stage, `max_age` drops reported, and the
  four-point stage trace (D5).
- **Acceptance:** a test per policy. A two-stage chain carries the source's
  capture time unchanged, and an over-age result is dropped and reported.

### W6 — Drivers that can only block

- **Owner:** unassigned.
- **Status:** OPEN
- `BlockingWorker` (D8): one task per device at the stage's priority.
- **Acceptance:** a blocking fake device beside a periodic control callback.
  The control callback's latency stays within its period while the device
  runs, against a baseline where the same call is made from a callback.

### W7 — First accelerator: Ethos-U55 on the Corstone-300 FVP

- **Owner:** unassigned.
- **Status:** OPEN
- An Ethos-U port adapter in a board package, on the emulator the FVP board
  machinery already runs (Corstone-300 models the Ethos-U55).
- Measurement: a control callback's latency next to an inference, with the
  driver's blocking invoke against the split stage.
- **Acceptance:** the measurement recorded in RFC-0139, and an inference whose
  completion interrupt drives the continuation.

### W8 — Zephyr sources: camera and sensor stream

- **Owner:** unassigned.
- **Status:** OPEN
- In-tree Zephyr adapters for the video API (`video_set_signal`, dequeue on
  completion) and RTIO sensor streaming (a watermark batch as one result).
- **Acceptance:** a source on Zephyr `native_sim` (the emulated camera, and an
  RTIO sensor emulator) feeds a stage. A watermark batch's data age includes
  its batching delay.

### W9 — Linux adapters

- **Owner:** unassigned.
- **Status:** OPEN
- In-tree: V4L2 (`QBUF`/`DQBUF`, `poll`) and eventfd completion through the
  park. Optional, not built by default: a TensorRT adapter package with
  completion from a CUDA event (D9).
- **Acceptance:** a V4L2 `vivid` virtual camera feeds a stage on the host. The
  TensorRT package builds on a host with CUDA and is skipped, by name, where
  there is none.

### W10 — Contract fields and checker rules

- **Owner:** unassigned, with the launch toolchain's owner.
- **Status:** OPEN
- The `devices` and `stages` contract fields and the checker rules (D10):
  utilisation per device, the non-preemptive blocking term, and a published
  suspension-aware chain bound.
- **Acceptance:** an over-committed device is refused before build, naming
  every stage on it. A path whose bound exceeds its budget is refused, naming
  the device that dominates.

### W11 — Book pages

- **Owner:** nros 139 AUO book (to confirm).
- **Status:** OPEN
- Two pages: "Use a device from a node" (C, C++, Rust) and "Add device
  support to a board" (an adapter in C, then in Rust).
- **Acceptance:** both pages' examples build, fenced by probes once W4 lands.

## Acceptance for the phase

- **No device holds the executor.** A control callback's latency is unchanged
  by a concurrent inference (W6, W7).
- **One seam.** An adapter in any of the three languages serves a node in any
  of them (W4).
- **The ROS API is untouched.** No new name is added under `rclc/`,
  `rclcpp/` or the `nros` user API; a gate checks it.
- **Device time is stated and checked.** A chain's budget includes device
  queueing, and an over-committed device is refused before build (W10).

## Related

- RFC-0139; RFC-0089, RFC-0054, RFC-0106, RFC-0135.
- Phase-574 (perception and large messages); phase-575 and phase-576 (the
  inter-core link).

## Issues homed here

| issue | why it belongs here |
| --- | --- |

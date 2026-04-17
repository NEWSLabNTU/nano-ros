# Autoware Safety Island vs Autoware Sentinel — Comparison

Comparison report filed 2026-04-17 during Phase 65 book work. Both projects
are safety-focused Autoware components running on Zephyr RTOS, but they
target different scopes and use fundamentally different language /
middleware stacks.

**Sources:**
- `~/repos/autoware-safety-island` (Arm / Autoware Foundation, C++ + CycloneDDS)
- `~/repos/autoware_sentinel` (nano-ros based, Rust `no_std` + Zenoh)

---

## Overview & Purpose

| Dimension | Autoware Safety Island | Autoware Sentinel |
|-----------|------------------------|-------------------|
| **Core Purpose** | Standalone Zephyr app running Autoware's trajectory follower (MPC lateral + PID longitudinal) on a safety-class processor | Independent safety island porting 7 Autoware safety/control nodes to nano-ros, running on a dedicated safety MCU with formal verification |
| **Primary Function** | Actuation layer: executes MPC/PID to track planned trajectories | Fail-safe layer: emergency/comfortable stop (MRM), heartbeat watchdog, command gating, control validation |
| **Maturity** | Production-ready for trajectory tracking | Phases 1–8 complete, phase 9 (behavioral verification) planned |

## Languages & Source Code

| Dimension | Autoware Safety Island | Autoware Sentinel |
|-----------|------------------------|-------------------|
| **Primary Language** | C++ | Rust (`#![no_std]`, zero heap) |
| **Secondary Language** | C (Zephyr, CycloneDDS) | C (Zephyr kernel for target) |
| **Source File Count** | ~93 C/C++/H files | ~41 Rust files across 24 crates |
| **Build Artifacts** | Single C++ binary | 24 Cargo crates → 2 binaries (Zephyr + Linux native) |
| **Code Paradigm** | OOP with ROS-like nodes | Systems Rust: static allocation, type-state, no heap |

## Target Platforms & RTOS

| Dimension | Autoware Safety Island | Autoware Sentinel |
|-----------|------------------------|-------------------|
| **RTOS** | Zephyr 3.6.0 | Zephyr 3.7.0 (target) + Linux native |
| **Target Boards** | FVP (ARMv8 simulation), NXP S32Z270DC2 RTU0 (Cortex-R) | STM32H743 (Cortex-M7 prototype), NXP S32K344 (Cortex-M4 production) + Linux x86-64 |
| **Processor Arch** | ARMv8 64-bit Cortex-R | ARMv7-EM 32-bit Cortex-M4/M7 + x86-64 |
| **Memory Model** | Standard Zephyr heap | Zero heap — single static `SafetyIsland` struct |
| **Safety Class** | Arm safety-class processor | Cortex-M safety MCUs + formal verification (ASIL-relevant) |

## Middleware & Messaging

| Dimension | Autoware Safety Island | Autoware Sentinel |
|-----------|------------------------|-------------------|
| **RMW Protocol** | **CycloneDDS 0.11.x** (native OMG DDS) | **rmw_zenoh** via **nano-ros** |
| **Message Format** | Autoware ROS 2 native IDL, DDS CDR | Auto-generated Rust from ROS `.msg`, Zenoh CDR (ROS Humble) |
| **Interoperability** | Peer-to-peer DDS with any ROS 2 DDS stack | Zenoh bridge to ROS 2 via rmw_zenoh; Linux binary works in planning simulator |
| **Input Topics** | 5 (Trajectory, Odometry, Steering, Accel, OperationMode) | 5 (VelocityReport, GearReport, ControlCmd, AutowareState, Heartbeat) |
| **Output Topics** | 1 (ControlCmd) | 30 (control + emergency + hazard + MRM state + operation mode + status + debug) |
| **Services** | None | 1 (`/api/operation_mode/change_to_autonomous`) |

## Build System & Dependencies

| Dimension | Autoware Safety Island | Autoware Sentinel |
|-----------|------------------------|-------------------|
| **Build Tool** | CMake 3.22+ with Zephyr/west | Cargo for crates + west for Zephyr workspace |
| **Package Management** | CMake subdirectories | Cargo workspace (19 members) |
| **Key External Deps** | Eigen3, OSQP (MPC solver), CycloneDDS, Autoware.msgs 1.3.0, Autoware 2025.02 | nano-ros (git patch), 25 auto-generated Autoware message crates, Zephyr |
| **Compiler Targets** | C++17 host + ARM C++ cross | Rust 2024; `thumbv7em-none-eabihf` + `x86_64-unknown-linux-gnu` |

## Architecture & Layers

| Dimension | Autoware Safety Island | Autoware Sentinel |
|-----------|------------------------|-------------------|
| **High-Level Design** | Single node: Controller Node instantiates lateral (MPC) and longitudinal (PID) controllers | Two-layer: (1) 11 algorithm crates (no ROS / no heap); (2) thin nano-ros wiring layer composing them deterministically |
| **Core Components** | Controller Node (MPC + PID) | MRM Handler → Emergency/Comfortable Stop → Heartbeat Watchdog → Vehicle Cmd Gate → Control Validator → Operation Mode Manager |
| **Data Flow** | Reactive ROS 2 spinloop (callback-driven) | Timer-driven 30 Hz deterministic + event-driven subscription callbacks |
| **State Management** | Dynamic allocation via ROS 2 node | Static allocation: all 11 algorithms + shared data in single `SafetyIsland` struct; `RefCell` for interior mutability |
| **Control Loop** | Implicit via ROS 2 spin | Explicit 30 Hz timer: watchdog→MRM, gate, validator, publish |

## Safety Features & Verification

| Dimension | Autoware Safety Island | Autoware Sentinel |
|-----------|------------------------|-------------------|
| **Safety Level** | Arm safety-class processor certification | Formally verified: 13 Kani harnesses + 8 Verus proofs |
| **NaN Handling** | Not explicitly addressed | NaN-safe by design: `!(x.abs() >= threshold)` pattern so NaN → safe zero |
| **Watchdog** | None (relies on Autoware heartbeat externally) | Heartbeat watchdog on `/api/system/heartbeat`, MRM on ~1–2s timeout |
| **Redundancy** | Single control channel | Dual-mode MRM: Emergency (~1 m/s² decel) or Comfortable (~0.5 m/s²) |
| **E2E Protection** | None | 30 validation failures (~1s @ 30Hz) → MRM escalation; jerk/accel/steering bounds |
| **Memory Safety** | C++ (UB possible) + Zephyr isolation | Type-safe Rust, no unsafe in `#![no_std]` code, zero heap = no OOM |
| **Formal Methods** | None | Kani (bounded model checking, 13 harnesses) + Verus (deductive proofs, 8 theorems) |
| **Testing** | DDS pub/sub unit tests | 130+ unit tests + integration with planning simulator + transport smoke tests |

## ROS 2 Compatibility

| Dimension | Autoware Safety Island | Autoware Sentinel |
|-----------|------------------------|-------------------|
| **ROS 2 Version** | Autoware 2025.02 / Universe 0.40.0 | Humble (via nano-ros + rmw_zenoh) |
| **Node Model** | Standard rclcpp (Zephyr-adapted) | nano-ros node (custom executor, `#![no_std]` capable) |
| **Interop Path** | Direct DDS peer | Zenoh broker bridge |
| **Planning Simulator** | Via native Autoware integration | Linux native binary runs in planning simulator (`just launch-autoware-baseline` / `launch-autoware-modified`) |
| **Zephyr Entry** | `main()` in C++ → instantiate Controller Node → spin | `extern "C" fn main()` in Rust via `zephyr-lang-rust` → `SafetyIsland` + `executor.spin()` |
| **Linux Entry** | N/A | `src/autoware_sentinel_linux/src/main.rs` → `SafetyIsland` with Linux clock → `executor.spin_blocking()` |

## Documentation

| Dimension | Autoware Safety Island | Autoware Sentinel |
|-----------|------------------------|-------------------|
| **README** | 73 lines: overview + components + Autoware version table | 173 lines: architecture + 7 replaced nodes + key design decisions + roadmap + quick start |
| **Design Docs** | 4 files in `/documentation/design/` (architecture, FreeRTOS porting plan, topics) | 12 phase docs in `/docs/roadmap/` + presentations + Zenoh latency research |
| **API Docs** | Inline Doxygen-ready C++ comments | Rust `///` comments, `cargo doc` |
| **Build Automation** | `build.sh` shell wrapper | `justfile` with 30+ targets including `just verify` |

---

## Key Differences

1. **Control scope** — Safety Island does **trajectory tracking** (MPC/PID); Sentinel does **fail-safe** (watchdog, MRM, gating).
2. **Language** — C++ vs Rust `#![no_std]`.
3. **Processor class** — 64-bit Cortex-R (industrial) vs 32-bit Cortex-M (embedded, low-power) + Linux dev.
4. **Middleware** — CycloneDDS (native) vs Zenoh + nano-ros (ROS 2 bridge).
5. **Memory model** — dynamic allocation vs zero-heap static.
6. **Safety validation** — Arm safety class (hardware) vs formal verification (Kani + Verus).
7. **Watchdog** — none vs heartbeat-timeout → MRM.
8. **Architecture** — single monolithic ROS node vs 11 independent algorithm crates + thin wiring layer.

## Similarities

1. Both port Autoware components for embedded safety scenarios.
2. Both target Zephyr RTOS as primary platform.
3. Both position as "safety island" — dedicated embedded processors isolated from main compute.
4. Sentinel's Linux binary includes **trajectory follower algorithms** (MPC/PID) from Safety Island for parity testing.
5. Both use Autoware message types.
6. Both are Apache-2.0, public GitHub projects from Arm / Autoware Foundation.
7. Both support multiple target boards including simulation (FVP for Safety Island, Linux native for Sentinel).

## Relationship

The two projects are **complementary rather than competing**: Safety Island delivers the *control* (trajectory follower), Sentinel delivers the *fail-safe* (watchdog + MRM + validation). An automotive safety island could run both — Safety Island's control binary on one core, Sentinel's fail-safe binary on another — with Sentinel's validator monitoring the control output.

Sentinel explicitly ports Safety Island's MPC/PID algorithms into its Linux binary for the planning-simulator integration, indicating the projects are aware of each other and the Sentinel team uses Safety Island as a reference for control behavior.

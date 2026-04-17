# Phase 65 — nano-ros Book

**Goal**: Produce a self-contained mdbook user guide covering installation,
concepts, platform guides, API reference, and advanced topics. Targets embedded
developers adopting nano-ros.

**Status**: In Progress (65.1–65.41 done; 65.42–65.55 revision pass filed)

**Priority**: Medium

**Depends on**: Phase 59 (API Documentation)

## Overview

Many user-facing docs currently live in `docs/` alongside contributor-only
material. This phase consolidates user-facing content into an mdbook at
`book/`, moving existing docs files where they map 1:1 to book chapters,
writing new content where needed, and deleting superseded docs.

After this phase, `docs/` contains only contributor/internal material
(design decisions, research, roadmap, internal analyses). All user-facing
guides, references, and tutorials live in `book/src/`.

### Strategy

Each work item uses one of three approaches:

- **Move** — relocate a `docs/` file to `book/src/`, fix internal links,
  delete the original. No major rewriting needed.
- **Write** — new content with no direct `docs/` equivalent.
- **Delete** — remove a `docs/` file that has been superseded by newly
  written book content (e.g., getting-started.md superseded by 65.3-65.5).

### Tooling

mdbook is the static site generator. The book skeleton exists at `book/`
with `book.toml` and `src/SUMMARY.md`.

## Book Structure

```
book/src/
SUMMARY.md
introduction.md

getting-started/
  installation.md
  first-app-rust.md
  first-app-c.md
  ros2-interop.md

concepts/
  architecture.md
  no-std.md
  rmw-backends.md
  platform-model.md

guides/
  message-generation.md
  creating-examples.md
  qemu-bare-metal.md
  esp32.md
  serial-transport.md
  configuration.md
  porting-platform.md
  adding-rmw-backend.md
  board-crate.md
  troubleshooting.md

platforms/
  README.md
  posix.md
  zephyr.md
  freertos.md
  nuttx.md
  threadx.md

reference/
  rust-api.md
  c-api.md
  environment-variables.md
  embedded-tuning.md
  build-commands.md
  rmw-zenoh-protocol.md

advanced/
  verification.md
  realtime-analysis.md
  safety.md
  contributing.md
```

## docs/ File Disposition

### Move to book (user-facing, 1:1 mapping)

| docs/ source                                | Book destination                     | Action                       |
|---------------------------------------------|--------------------------------------|------------------------------|
| `guides/message-generation.md`              | `guides/message-generation.md`       | Move, fix links              |
| `guides/creating-examples.md`               | `guides/creating-examples.md`        | Move, fix links              |
| `guides/qemu-bare-metal.md`                 | `guides/qemu-bare-metal.md`          | Move, fix links              |
| `guides/esp32-setup.md`                     | `guides/esp32.md`                    | Move, rename, fix links      |
| `guides/zephyr-setup.md`                    | `platforms/zephyr.md`                | Move, rename, fix links      |
| `guides/troubleshooting.md`                 | `guides/troubleshooting.md`          | Move, fix links              |
| `guides/quick-reference.md`                 | `reference/build-commands.md`        | Move, rename, fix links      |
| `guides/embedded-tuning.md`                 | `reference/embedded-tuning.md`       | Move, fix links              |
| `guides/verus-verification.md`              | `advanced/verification.md`           | Move, fix links              |
| `guides/realtime-lint-guide.md`             | `advanced/realtime-analysis.md`      | Move, fix links              |
| `guides/freertos-lan9118-debugging.md`      | `platforms/freertos.md`              | Move, rename, fix links      |
| `reference/environment-variables.md`        | `reference/environment-variables.md` | Move, fix links              |
| `reference/c-api-cmake.md`                  | `reference/c-api.md`                 | Move, rename, fix links      |
| `reference/rmw_zenoh_interop.md`            | `reference/rmw-zenoh-protocol.md`    | Move, rename, fix links      |
| `reference/std-alloc-requirements.md`       | `concepts/no-std.md`                 | Move, rename, fix links      |
| `reference/wcet-baselines.md`               | `advanced/realtime-analysis.md`      | Merge into realtime-analysis |
| `design/architecture-overview.md`           | `concepts/architecture.md`           | Move, fix links              |
| `design/e2e-safety-protocol-integration.md` | `advanced/safety.md`                 | Move, fix links              |

### Delete from docs (superseded by new book content)

| docs/ file                          | Superseded by                                              |
|-------------------------------------|------------------------------------------------------------|
| `guides/getting-started.md`         | 65.3 installation + 65.4 first-app-rust + 65.5 first-app-c |
| `reference/micro-ros-comparison.md` | 65.2 introduction (comparison table)                       |

### Keep in docs (contributor/internal only)

| docs/ file                                | Reason                           |
|-------------------------------------------|----------------------------------|
| `reference/api-comparison-rclrs.md`       | Internal API alignment reference |
| `reference/rmw-h-analysis.md`             | Internal rmw.h feasibility study |
| `reference/xrce-dds-analysis.md`          | Internal XRCE-DDS analysis       |
| `reference/executor-fairness-analysis.md` | Internal phase 37 analysis       |
| `design/rmw-layer-design.md`              | Internal crate rename plan       |
| `design/example-directory-layout.md`      | Internal reorg proposal          |
| `design/zonal-vehicle-architecture.md`    | Research                         |
| `research/*`                              | All internal research            |
| `roadmap/*`                               | All internal roadmap             |

## Work Items

- [x] 65.1 — mdbook setup and SUMMARY.md
- [x] 65.2 — Introduction (write)
- [x] 65.3 — Getting Started: installation (write)
- [x] 65.4 — Getting Started: first Rust app (write)
- [x] 65.5 — Getting Started: first C app (write)
- [x] 65.6 — Getting Started: ROS 2 interop (write)
- [x] 65.7 — Concepts: architecture (move `design/architecture-overview.md`)
- [x] 65.8 — Concepts: no_std (move `reference/std-alloc-requirements.md`)
- [x] 65.9 — Concepts: RMW backends (write, draw from `reference/xrce-dds-analysis.md`)
- [x] 65.10 — Concepts: platform model (write)
- [x] 65.11 — Guides: message generation (move `guides/message-generation.md`)
- [x] 65.12 — Guides: creating examples (move `guides/creating-examples.md`)
- [x] 65.13 — Guides: QEMU bare-metal (move `guides/qemu-bare-metal.md`)
- [x] 65.14 — Guides: ESP32 (move `guides/esp32-setup.md`)
- [x] 65.15 — Guides: troubleshooting (move `guides/troubleshooting.md`)
- [x] 65.16 — Platforms: overview (write)
- [x] 65.17 — Platforms: POSIX (write)
- [x] 65.18 — Platforms: Zephyr (move `guides/zephyr-setup.md`)
- [x] 65.19 — Platforms: FreeRTOS (move `guides/freertos-lan9118-debugging.md`)
- [x] 65.20 — Platforms: NuttX (write)
- [x] 65.21 — Platforms: ThreadX (write)
- [x] 65.22 — Reference: Rust API (write)
- [x] 65.23 — Reference: C API (move `reference/c-api-cmake.md`)
- [x] 65.24 — Reference: environment variables (move `reference/environment-variables.md`)
- [x] 65.25 — Reference: embedded tuning (move `guides/embedded-tuning.md`)
- [x] 65.26 — Reference: build commands (move `guides/quick-reference.md`)
- [x] 65.27 — Reference: RMW Zenoh protocol (move `reference/rmw_zenoh_interop.md`)
- [x] 65.28 — Advanced: verification (move `guides/verus-verification.md`)
- [x] 65.29 — Advanced: real-time analysis (move `guides/realtime-lint-guide.md` + merge `reference/wcet-baselines.md`)
- [x] 65.30 — Advanced: safety (move `design/e2e-safety-protocol-integration.md`)
- [x] 65.31 — Advanced: contributing (write)
- [ ] 65.32 — Delete superseded docs (deferred: user should review book content before deletion)
- [x] 65.33 — Update SUMMARY.md for embedded-tuning chapter
- [x] 65.34 — Update CLAUDE.md docs index
- [x] 65.35 — Review, cross-links, and polish
- [x] 65.36 — Add `just book` recipe
- [x] 65.37 — Guides: porting to a new platform (write)
- [x] 65.38 — Guides: adding an RMW backend (write)
- [x] 65.39 — Guides: configuration (write)
- [x] 65.40 — Guides: board crate implementation (write)
- [x] 65.41 — Update SUMMARY.md for new guides

### 65.1 — mdbook setup and SUMMARY.md

Set up `book/book.toml` configuration and write the complete `SUMMARY.md`
with all chapters. Create placeholder files so the book builds immediately.

**Files**: `book/book.toml`, `book/src/SUMMARY.md`, all `book/src/**/*.md`

### 65.2 — Introduction

What is nano-ros, why it exists, how it compares to micro-ROS. Target audience,
project status, supported platforms at a glance.

**Action**: Write new content.

**Files**: `book/src/introduction.md`

### 65.3 — Getting Started: installation

Prerequisites, `just setup`, zenohd, Docker.

**Action**: Write new content.

**Files**: `book/src/getting-started/installation.md`

### 65.4 — Getting Started: first Rust app

Step-by-step pub/sub in Rust.

**Action**: Write new content.

**Files**: `book/src/getting-started/first-app-rust.md`

### 65.5 — Getting Started: first C app

Same walkthrough using the C API + CMake.

**Action**: Write new content.

**Files**: `book/src/getting-started/first-app-c.md`

### 65.6 — Getting Started: ROS 2 interop

Connecting to ROS 2 via rmw_zenoh.

**Action**: Write new content.

**Files**: `book/src/getting-started/ros2-interop.md`

### 65.7 — Concepts: architecture

Layer diagram, crate map, executor model, board crates, data flow.

**Action**: Move `docs/design/architecture-overview.md` to
`book/src/concepts/architecture.md`. Fix internal cross-references.

**Files**: `book/src/concepts/architecture.md`
**Delete**: `docs/design/architecture-overview.md`

### 65.8 — Concepts: no_std

Feature tiers (`no_std`, `alloc`, `std`). Per-crate requirements table.

**Action**: Move `docs/reference/std-alloc-requirements.md` to
`book/src/concepts/no-std.md`. Fix links.

**Files**: `book/src/concepts/no-std.md`
**Delete**: `docs/reference/std-alloc-requirements.md`

### 65.9 — Concepts: RMW backends

Zenoh vs XRCE-DDS architecture, when to use which, feature matrix.

**Action**: Write new chapter. Draw from `reference/xrce-dds-analysis.md`
(kept in docs/) and `reference/micro-ros-comparison.md` (will be deleted in
65.32).

**Files**: `book/src/concepts/rmw-backends.md`

### 65.10 — Concepts: platform model

Three orthogonal axes. Mutual exclusivity. Cargo feature enforcement.

**Action**: Write new chapter.

**Files**: `book/src/concepts/platform-model.md`

### 65.11 — Guides: message generation

`cargo nano-ros generate-rust` workflow, `package.xml`, bundled vs ament-index.

**Action**: Move `docs/guides/message-generation.md` to
`book/src/guides/message-generation.md`. Fix links.

**Files**: `book/src/guides/message-generation.md`
**Delete**: `docs/guides/message-generation.md`

### 65.12 — Guides: creating examples

Example directory layout, `.cargo/config.toml` patches, build isolation.

**Action**: Move `docs/guides/creating-examples.md` to
`book/src/guides/creating-examples.md`. Fix links.

**Files**: `book/src/guides/creating-examples.md`
**Delete**: `docs/guides/creating-examples.md`

### 65.13 — Guides: QEMU bare-metal

QEMU MPS2-AN385 setup, TAP networking, Docker Compose, manual test.

**Action**: Move `docs/guides/qemu-bare-metal.md` to
`book/src/guides/qemu-bare-metal.md`. Fix links.

**Files**: `book/src/guides/qemu-bare-metal.md`
**Delete**: `docs/guides/qemu-bare-metal.md`

### 65.14 — Guides: ESP32

ESP32-C3 toolchain, espflash, QEMU, TAP networking, heap tuning.

**Action**: Move `docs/guides/esp32-setup.md` to
`book/src/guides/esp32.md`. Fix links.

**Files**: `book/src/guides/esp32.md`
**Delete**: `docs/guides/esp32-setup.md`

### 65.15 — Guides: troubleshooting

Common issues and solutions.

**Action**: Move `docs/guides/troubleshooting.md` to
`book/src/guides/troubleshooting.md`. Fix links.

**Files**: `book/src/guides/troubleshooting.md`
**Delete**: `docs/guides/troubleshooting.md`

### 65.16 — Platforms: overview

How to read platform chapters. Common patterns (zpico-platform + board crate).
Network stack options per platform.

**Action**: Write new chapter.

**Files**: `book/src/platforms/README.md`

### 65.17 — Platforms: POSIX

Linux/macOS native development. Simplest path.

**Action**: Write new chapter.

**Files**: `book/src/platforms/posix.md`

### 65.18 — Platforms: Zephyr

Zephyr module integration, West workspace, Kconfig, TAP bridge.

**Action**: Move `docs/guides/zephyr-setup.md` to
`book/src/platforms/zephyr.md`. Fix links.

**Files**: `book/src/platforms/zephyr.md`
**Delete**: `docs/guides/zephyr-setup.md`

### 65.19 — Platforms: FreeRTOS

FreeRTOS + lwIP on QEMU MPS2-AN385. Task priorities, heap config, debugging.

**Action**: Move `docs/guides/freertos-lan9118-debugging.md` to
`book/src/platforms/freertos.md`. Fix links.

**Files**: `book/src/platforms/freertos.md`
**Delete**: `docs/guides/freertos-lan9118-debugging.md`

### 65.20 — Platforms: NuttX

NuttX RTOS on QEMU. POSIX-like API. `just nuttx setup`.

**Action**: Write new chapter.

**Files**: `book/src/platforms/nuttx.md`

### 65.21 — Platforms: ThreadX

ThreadX + NetX Duo. SIL 4 / ASIL D context. Linux sim + QEMU RISC-V.

**Action**: Write new chapter.

**Files**: `book/src/platforms/threadx.md`

### 65.22 — Reference: Rust API

Node, Publisher, Subscription, Service, Client, Action, Timer, Guard,
Lifecycle, Parameters, Executor. Error types.

**Action**: Write new chapter.

**Files**: `book/src/reference/rust-api.md`

### 65.23 — Reference: C API

C types/functions by module. CMake integration. Header structure. RMW selection.

**Action**: Move `docs/reference/c-api-cmake.md` to
`book/src/reference/c-api.md`. Fix links.

**Files**: `book/src/reference/c-api.md`
**Delete**: `docs/reference/c-api-cmake.md`

### 65.24 — Reference: environment variables

Runtime + build-time configuration.

**Action**: Move `docs/reference/environment-variables.md` to
`book/src/reference/environment-variables.md`. Fix links.

**Files**: `book/src/reference/environment-variables.md`
**Delete**: `docs/reference/environment-variables.md`

### 65.25 — Reference: embedded tuning

Compile-time constants for transport buffer sizing on embedded targets.

**Action**: Move `docs/guides/embedded-tuning.md` to
`book/src/reference/embedded-tuning.md`. Fix links.

**Files**: `book/src/reference/embedded-tuning.md`
**Delete**: `docs/guides/embedded-tuning.md`

### 65.26 — Reference: build commands

All `just` recipes, manual testing commands, Docker, QEMU, Zephyr quick ref.

**Action**: Move `docs/guides/quick-reference.md` to
`book/src/reference/build-commands.md`. Fix links.

**Files**: `book/src/reference/build-commands.md`
**Delete**: `docs/guides/quick-reference.md`

### 65.27 — Reference: RMW Zenoh protocol

Key expression format, QoS, discovery, liveliness tokens, CDR encoding,
RMW attachment, wire compatibility with rmw_zenoh_cpp.

**Action**: Move `docs/reference/rmw_zenoh_interop.md` to
`book/src/reference/rmw-zenoh-protocol.md`. Fix links.

**Files**: `book/src/reference/rmw-zenoh-protocol.md`
**Delete**: `docs/reference/rmw_zenoh_interop.md`

### 65.28 — Advanced: verification

Kani bounded model checking, Verus deductive proofs, Miri, ghost types.

**Action**: Move `docs/guides/verus-verification.md` to
`book/src/advanced/verification.md`. Fix links.

**Files**: `book/src/advanced/verification.md`
**Delete**: `docs/guides/verus-verification.md`

### 65.29 — Advanced: real-time analysis

WCET measurement, real-time lint guide, cargo-call-stack.

**Action**: Move `docs/guides/realtime-lint-guide.md` to
`book/src/advanced/realtime-analysis.md`. Merge
`docs/reference/wcet-baselines.md` content into it. Fix links.

**Files**: `book/src/advanced/realtime-analysis.md`
**Delete**: `docs/guides/realtime-lint-guide.md`, `docs/reference/wcet-baselines.md`

### 65.30 — Advanced: safety

E2E safety protocol, AUTOSAR/ISO 26262 context.

**Action**: Move `docs/design/e2e-safety-protocol-integration.md` to
`book/src/advanced/safety.md`. Fix links.

**Files**: `book/src/advanced/safety.md`
**Delete**: `docs/design/e2e-safety-protocol-integration.md`

### 65.31 — Advanced: contributing

Development practices, quality checks, testing, code style, PR workflow.

**Action**: Write new chapter from CLAUDE.md and `tests/README.md`.

**Files**: `book/src/advanced/contributing.md`

### 65.32 — Delete superseded docs

Remove docs files that have been fully superseded by new book content.

**Delete**:
- `docs/guides/getting-started.md` (superseded by 65.3 + 65.4 + 65.5)
- `docs/reference/micro-ros-comparison.md` (superseded by 65.2 introduction)

### 65.33 — Update SUMMARY.md for embedded-tuning chapter

Add the `embedded-tuning.md` entry to SUMMARY.md (added after 65.1).

**Files**: `book/src/SUMMARY.md`

### 65.34 — Update CLAUDE.md docs index

Update the `Documentation Index` section in CLAUDE.md to reflect that
user-facing docs have moved to `book/src/`.

**Files**: `CLAUDE.md`

### 65.35 — Review, cross-links, and polish

Cross-link chapters. Consistent terminology. Verify code snippets. Ensure
`mdbook build book/` has zero warnings.

**Files**: all `book/src/**/*.md`

### 65.36 — Add `just book` recipe

Add `just book` (build) and `just book-serve` (dev server with watch)
recipes to justfile.

**Files**: `justfile`

### 65.37 — Guides: porting to a new platform

Developer guide for porting nano-ros to a new platform (RTOS or bare-metal).
Lists all required FFI symbols (clock, memory, sleep, random, threading,
sockets, libc stubs), the two-crate pattern (zpico-platform + nros-board),
and a step-by-step porting procedure.

**Action**: Write new chapter.

**Files**: `book/src/guides/porting-platform.md`

### 65.38 — Guides: adding an RMW backend

Developer guide for implementing a new RMW backend. Covers the full trait
hierarchy (Rmw, Session, Publisher, Subscriber, ServiceServerTrait,
ServiceClientTrait), message buffering patterns, QoS mapping, feature flag
wiring, and testing.

**Action**: Write new chapter.

**Files**: `book/src/guides/adding-rmw-backend.md`

### 65.39 — Guides: configuration

Comprehensive configuration reference across all four layers: config.toml,
build-time environment variables, Cargo features, and runtime environment.
Includes deployment scenario examples and precedence rules.

**Action**: Write new chapter. Consolidates information from
`reference/environment-variables.md` and `reference/config-toml.md` into
a unified guide.

**Files**: `book/src/guides/configuration.md`

### 65.40 — Guides: board crate implementation

Developer guide for creating a new board crate. Covers Config struct with
feature-gated fields, hardware init sequence, Ethernet/Serial/WiFi/lwIP/NetX
transport setup, the run() entry point, re-exports, and a checklist.

**Action**: Write new chapter.

**Files**: `book/src/guides/board-crate.md`

### 65.41 — Update SUMMARY.md for new guides

Add entries for the four new guide chapters to SUMMARY.md.

**Files**: `book/src/SUMMARY.md`


## Book Revision (65.42–65.55)

Revision pass filed 2026-04-17 after Phase 79 (platform abstraction), Phase 80
(networking unification), and Phase 82 (C++ Future/Stream). Covers stale
content, missing chapters, duplicated topics, and architecture drift.

### Missing chapters

- [x] 65.42 — Reference: C++ API (`reference/cpp-api.md`)
  - [x] 65.42.1 — Write reference doc covering `nros::Node`, `nros::Publisher<M>`,
    `nros::Subscription<M>`, `nros::Service<S>`, `nros::Client<S>`,
    `nros::ActionServer<A>`, `nros::ActionClient<A>`, `nros::Timer`,
    `nros::GuardCondition`, `nros::Executor`
  - [x] 65.42.2 — Document `nros::Result` + `NROS_TRY` error handling
  - [x] 65.42.3 — Document freestanding vs std mode (`NROS_CPP_STD`):
    `const char*` / C function pointers / integer ms (freestanding) vs
    `std::string` / `std::function` / `std::chrono` (std mode)
  - [x] 65.42.4 — Document `Future<T>` (Phase 82) for non-blocking service
    calls and action goals: `client.call(req)` → `Future<Response>`,
    `action_client.send_goal(goal)` → `Future<bool>`
  - [x] 65.42.5 — Document `Stream<T>` (Phase 82) for action feedback
  - [x] 65.42.6 — CMake integration: `nano_ros_generate_interfaces(... LANGUAGE CPP)`,
    Zephyr `CONFIG_NROS_CPP_API=y`
  - [x] 65.42.7 — Add to SUMMARY.md under Reference section

- [x] 65.43 — Reference: RMW API (`reference/rmw-api.md`)
  - [x] 65.43.1 — Write reference-style doc for `nros-rmw` trait signatures:
    `Session`, `SessionPublisher`, `SessionSubscriber`, `ServiceServerTrait`,
    `ServiceClientTrait`; associated types `Error`, `Publisher`, `Subscriber`
  - [x] 65.43.2 — Document zenoh-specific extensions: `ZenohSession`,
    `ZenohPublisher`, `LivelinessToken`, `RmwAttachment`, `Ros2Liveliness`
  - [x] 65.43.3 — Document XRCE-specific extensions: `XrceRmw`, transport
    init callbacks
  - [x] 65.43.4 — Cross-link from `concepts/rmw-api-design.md` (architectural)
    to this doc (reference)
  - [x] 65.43.5 — Add to SUMMARY.md

- [x] 65.44 — Guides: platform customization (`guides/platform-customization.md`)
  - [x] 65.44.1 — Write a unified guide explaining which packages are
    user-customizable vs core (must not be modified):
    - **Customizable**: `nros-platform-<name>` (one per RTOS/bare-metal),
      `nros-rmw-<name>` (one per transport backend), board crates
      (`nros-<board>`, `nros-<board>-<rtos>`), driver crates
      (`nros-smoltcp`, `lan9118-smoltcp`, etc.)
    - **Core (do not modify)**: `nros`, `nros-core`, `nros-node`,
      `nros-serdes`, `nros-macros`, `nros-params`, `nros-rmw`,
      `nros-platform` (the trait crate), `zpico-platform-shim`,
      `xrce-platform-shim`, `zpico-sys`, `xrce-sys`
  - [x] 65.44.2 — Diagram showing the customization boundary: core crates
    (fixed) → trait boundary → user crates (platform, RMW, board, drivers)
  - [x] 65.44.3 — Cross-link from `concepts/architecture.md` and
    `guides/porting-platform/README.md`
  - [x] 65.44.4 — Add to SUMMARY.md

### Stale content updates

- [x] 65.45 — Update Rust API reference for non-blocking calls (Phase 68+77+82)
  - [x] 65.45.1 — Document `call()` → `Promise<Reply>` (non-blocking) vs
    `call_blocking()` (old blocking API). `Promise` resolves via `spin_once()`
  - [x] 65.45.2 — Document action client: `send_goal()` → `(GoalId,
    Promise<bool>)`, `get_result()` → `Promise<(GoalStatus, Result)>`
  - [x] 65.45.3 — Document `spin_async()` for async executors (Embassy, tokio)
  - [x] 65.45.4 — Document `spin_period(Duration)` return type `SpinPeriodResult`
  - [x] 65.45.5 — Document manual-poll action server: `create_action_server()`
    is NOT arena-registered; must call `server.try_handle_get_result()` explicitly
  - [x] 65.45.6 — Verify all code snippets compile against current API

- [x] 65.46 — Update C API reference for actions + non-blocking patterns
  - [x] 65.46.1 — Document C action server/client API: `nros_create_action_server()`,
    `nros_create_action_client()`, `nros_action_send_goal()`,
    `nros_action_get_result()`
  - [x] 65.46.2 — Document C non-blocking get: `nros_action_send_goal_start()` /
    `nros_action_send_goal_check()` poll pattern
  - [x] 65.46.3 — Verify CMake examples match current `nano_ros_generate_interfaces()` API

- [x] 65.47 — Update platform API reference for Phase 80 networking traits
  - [x] 65.47.1 — Add `PlatformTcp` trait: `create_endpoint`, `free_endpoint`,
    `open`, `listen`, `close`, `read`, `read_exact`, `send`
  - [x] 65.47.2 — Add `PlatformUdp` trait: `create_endpoint`, `free_endpoint`,
    `open`, `close`, `read`, `read_exact`, `send`
  - [x] 65.47.3 — Add `PlatformSocketHelpers` trait: `set_non_blocking`,
    `accept`, `close`, `wait_event`
  - [x] 65.47.4 — Add `PlatformUdpMulticast` trait: `mcast_open`,
    `mcast_listen`, `mcast_close`, `mcast_read`, `mcast_read_exact`, `mcast_send`
  - [x] 65.47.5 — Update symbol count: was ~53 zenoh-pico symbols, now ~80+
    with networking forwarders; update the table in the doc
  - [x] 65.47.6 — Note which platforms have Rust networking (POSIX, bare-metal,
    FreeRTOS, Zephyr) vs C networking (NuttX, ThreadX) vs deferred (multicast
    on Zephyr)

- [x] 65.48 — Update architecture diagrams for platform abstraction layer
  - [x] 65.48.1 — Add `nros-platform` trait layer to the main architecture
    diagram in `concepts/architecture.md`. Show: Application → nros facade →
    nros-node → nros-rmw-zenoh → zpico-platform-shim → nros-platform →
    nros-platform-<impl>
  - [x] 65.48.2 — Add networking flow: zenoh-pico C transport calls →
    shim `_z_open_tcp` etc. → `ConcretePlatform::tcp_open()` →
    platform-specific socket API (libc, lwIP, Zephyr POSIX, smoltcp)
  - [x] 65.48.3 — Update the "Board Crates" section to show the split
    between `nros-platform-<rtos>` (generic) and `nros-<board>-<rtos>`
    (hardware-specific)
  - [x] 65.48.4 — Add `nros-smoltcp` in the driver layer diagram (replaces
    the deleted `zpico-smoltcp`)

### Deduplication

- [x] 65.49 — Consolidate platform porting material
  - [x] 65.49.1 — Audit the 11 files that mention `PlatformClock` /
    `zpico-platform-shim` / `nros-platform`. In most cases (concepts/,
    platforms/, reference/) the mentions should be replaced with a brief
    summary + cross-link to `guides/porting-platform/` as the single source
    of truth for the porting procedure
  - [x] 65.49.2 — `concepts/platform-model.md` should explain the *model*
    (three axes, feature flags) but NOT repeat the porting steps;
    link to `guides/porting-platform/` for how-to
  - [x] 65.49.3 — `platforms/README.md` should list platforms with links
    but NOT repeat the trait list; link to `reference/platform-api.md`

- [x] 65.50 — Consolidate configuration material
  - [x] 65.50.1 — Audit `guides/configuration.md`, `reference/config-toml.md`,
    `reference/environment-variables.md`, `reference/embedded-tuning.md`:
    - `guides/configuration.md` = unified guide (4-layer overview + examples)
    - `reference/config-toml.md` = reference for config.toml fields
    - `reference/environment-variables.md` = reference for env vars
    - `reference/embedded-tuning.md` = deep-dive on transport buffer sizing
  - [x] 65.50.2 — Remove duplicated env var tables from `guides/configuration.md`
    (keep in `reference/environment-variables.md` only, link from the guide)
  - [x] 65.50.3 — Remove duplicated ZPICO_MAX_* descriptions from
    `reference/environment-variables.md` if already in `embedded-tuning.md`

- [x] 65.51 — Consolidate executor/spin pattern descriptions
  - [x] 65.51.1 — `reference/rust-api.md` is the single source for executor
    spin patterns (`spin_once`, `spin_blocking`, `spin_period`, `spin_async`).
    Other files that explain spin should have a brief sentence + cross-link
  - [x] 65.51.2 — Remove detailed spin explanations from
    `concepts/architecture.md` (keep the architectural overview, link to
    Rust API ref for details)

- [x] 65.52 — Consolidate board crate material
  - [x] 65.52.1 — `guides/board-crate.md` is the single source. Platform
    chapters (freertos.md, nuttx.md, threadx.md) should reference it rather
    than repeating the `Config` struct / `run()` pattern
  - [x] 65.52.2 — `guides/porting-platform/implementing-a-platform.md`
    should reference `guides/board-crate.md` for the board crate step

### New concepts chapter

- [x] 65.53 — Concepts: scheduling models update
  - [x] 65.53.1 — `concepts/scheduling-models.md` exists but is not listed
    in SUMMARY.md → verify it's actually linked (it IS in SUMMARY.md per
    grep; confirmed). Review content for staleness — verify priority
    recommendations match current CLAUDE.md (poll task ≥ 4 on FreeRTOS)

### Acceptance criteria update

- [x] 65.54 — Update SUMMARY.md for all new chapters
  - [x] 65.54.1 — Add `reference/cpp-api.md`
  - [x] 65.54.2 — Add `reference/rmw-api.md`
  - [x] 65.54.3 — Add `guides/platform-customization.md`

- [x] 65.55 — Final cross-link pass
  - [x] 65.55.1 — Every chapter that mentions a topic covered in another
    chapter should have a `[see X](../path.md)` link instead of inline
    re-explanation
  - [x] 65.55.2 — `mdbook build book/` zero warnings after all changes


## Book Reorganization (65.56–65.65)

Reorganization filed 2026-04-17. The book is primarily for users; dev/contributor
content moves to an Internals chapter. Getting Started becomes example-driven
(one page per platform). Configuration consolidated into a single User Guide page.

### New SUMMARY.md structure

```
Introduction
# Getting Started
  Installation
  Native (Linux / macOS)         ← merge first-app-rust + first-app-c + posix.md
  Zephyr                         ← merge zephyr.md + relevant config
  FreeRTOS (QEMU)                ← merge freertos.md
  NuttX (QEMU)                   ← merge nuttx.md
  ThreadX                        ← merge threadx.md
  Bare-metal (QEMU ARM)          ← merge qemu-bare-metal.md
  ESP32                          ← merge esp32.md
  ROS 2 Interoperability
# User Guide
  Choosing an RMW Backend        ← practical guide (from rmw-backends.md)
  Configuration                  ← ONE page: features + config.toml + env vars + tuning
  Message Generation
  Serial Transport
  Troubleshooting
# Reference
  Rust API
  C API
  C++ API
  Platform API
  Environment Variables          ← lookup table only
  Build Commands
# Concepts
  Architecture Overview
  no_std Support
  Platform Model
# Internals
  RMW API Design                 ← from Concepts
  RMW API Reference              ← from Reference
  RMW Zenoh Protocol             ← from Reference
  Scheduling Models              ← from Concepts
  Formal Verification            ← from Advanced
  Real-Time Analysis             ← from Advanced
  Safety Protocol                ← from Advanced
  Porting to a New Platform      ← from Guides
  Adding an RMW Backend          ← from Guides
  Board Crate Implementation     ← from Guides
  Platform Customization         ← from Guides
  Platform Porting Pitfalls      ← from Advanced
  Contributing                   ← from Advanced
```

### Work items

- [ ] 65.56 — Restructure Getting Started: per-platform example pages
  - [ ] 65.56.1 — Merge `getting-started/first-app-rust.md` +
    `getting-started/first-app-c.md` + `platforms/posix.md` into a
    single `getting-started/native.md` page (show both Rust and C
    on the same page, explain RMW/feature selection in context)
  - [ ] 65.56.2 — Move `platforms/zephyr.md` → `getting-started/zephyr.md`
  - [ ] 65.56.3 — Move `platforms/freertos.md` → `getting-started/freertos.md`
  - [ ] 65.56.4 — Move `platforms/nuttx.md` → `getting-started/nuttx.md`
  - [ ] 65.56.5 — Move `platforms/threadx.md` → `getting-started/threadx.md`
  - [ ] 65.56.6 — Move `guides/qemu-bare-metal.md` → `getting-started/bare-metal.md`
  - [ ] 65.56.7 — Move `guides/esp32.md` → `getting-started/esp32.md`
  - [ ] 65.56.8 — Keep `getting-started/ros2-interop.md` in place
  - [ ] 65.56.9 — Delete `platforms/README.md` (overview absorbed into
    Getting Started intro or architecture page)

- [ ] 65.57 — Create User Guide section
  - [ ] 65.57.1 — Move `concepts/rmw-backends.md` → `user-guide/rmw-backends.md`,
    rewrite to practical tone ("use zenoh for X, XRCE for Y")
  - [ ] 65.57.2 — Merge `guides/configuration.md` + `reference/config-toml.md` +
    `reference/embedded-tuning.md` into single `user-guide/configuration.md`.
    Keep `reference/environment-variables.md` as lookup-only table.
  - [ ] 65.57.3 — Move `guides/message-generation.md` → `user-guide/message-generation.md`
  - [ ] 65.57.4 — Move `guides/serial-transport.md` → `user-guide/serial-transport.md`
  - [ ] 65.57.5 — Move `guides/troubleshooting.md` → `user-guide/troubleshooting.md`

- [ ] 65.58 — Slim down Reference section
  - [ ] 65.58.1 — Delete `reference/config-toml.md` (merged into user-guide/configuration.md)
  - [ ] 65.58.2 — Delete `reference/embedded-tuning.md` (merged into user-guide/configuration.md)
  - [ ] 65.58.3 — Move `reference/rmw-api.md` → `internals/rmw-api.md`
  - [ ] 65.58.4 — Move `reference/rmw-zenoh-protocol.md` → `internals/rmw-zenoh-protocol.md`

- [ ] 65.59 — Slim down Concepts section
  - [ ] 65.59.1 — Move `concepts/rmw-api-design.md` → `internals/rmw-api-design.md`
  - [ ] 65.59.2 — Move `concepts/scheduling-models.md` → `internals/scheduling-models.md`

- [ ] 65.60 — Create Internals section (rename Advanced → Internals)
  - [ ] 65.60.1 — Rename `advanced/` dir → `internals/`
  - [ ] 65.60.2 — Move porting guides: `guides/porting-platform/*` → `internals/porting-platform/*`
  - [ ] 65.60.3 — Move `guides/adding-rmw-backend.md` → `internals/adding-rmw-backend.md`
  - [ ] 65.60.4 — Move `guides/board-crate.md` → `internals/board-crate.md`
  - [ ] 65.60.5 — Move `guides/platform-customization.md` → `internals/platform-customization.md`
  - [ ] 65.60.6 — Move `guides/creating-examples.md` → `internals/creating-examples.md`

- [ ] 65.61 — Rewrite SUMMARY.md for new structure
- [ ] 65.62 — Fix all internal cross-links (relative paths changed by moves)
- [ ] 65.63 — Delete empty directories and orphaned files
- [ ] 65.64 — `mdbook build book/` zero warnings
- [ ] 65.65 — Update CLAUDE.md documentation index


## Acceptance Criteria

### Original (65.1–65.41)

- [x] `mdbook build book/` succeeds with zero warnings
- [x] `just book` builds the book
- [x] All content chapters written or moved
- [ ] No links from book to `docs/` directory
- [ ] All moved docs deleted from `docs/`
- [ ] `docs/` contains only contributor/internal material (design decisions,
      research, roadmap, internal analyses)
- [ ] Code snippets are accurate (match current API)
- [x] Getting Started path works end-to-end on a fresh `just setup` environment
- [x] Platform chapters cover all five supported platforms
- [x] CLAUDE.md docs index updated

### Revision (65.42–65.55) — all done

- [x] C++ API reference exists and covers Future/Stream patterns (Phase 82)
- [x] RMW API reference exists with trait signatures for both backends
- [x] Platform customization guide exists; clearly marks core vs user packages
- [x] Rust API reference documents `call()` → `Promise`, action client
      `send_goal()` / `get_result()`, `spin_async()`, `spin_period()`
- [x] C API reference documents action server/client and non-blocking patterns
- [x] Platform API reference documents all Phase 80 networking traits
- [x] Architecture diagram shows nros-platform layer + networking flow
- [x] No topic is explained in detail in more than one chapter — duplicates
      replaced with cross-links
- [x] `mdbook build book/` zero warnings after all changes

### Reorganization (65.56–65.65)

- [ ] Getting Started has one page per platform (example-driven)
- [ ] User Guide exists with config, RMW selection, message gen, serial, troubleshooting
- [ ] Reference is lookup-only (no prose duplication with User Guide)
- [ ] Concepts is minimal (3 pages: architecture, no_std, platform model)
- [ ] Internals chapter contains all dev/contributor content
- [ ] No empty directories or orphaned files
- [ ] `mdbook build book/` zero warnings
- [ ] CLAUDE.md docs index updated

## Notes

- After this phase, `docs/` retains: `design/` (3 files: rmw-layer-design,
  example-directory-layout, zonal-vehicle-architecture), `reference/` (4
  files: api-comparison-rclrs, rmw-h-analysis, xrce-dds-analysis,
  executor-fairness-analysis), `research/` (all files), `roadmap/` (all
  files).
- Book content should be kept concise — link to rustdoc/Doxygen for
  exhaustive API details rather than duplicating every function signature.
- Platform chapters for in-progress platforms (FreeRTOS 54, NuttX 55,
  ThreadX 58) should document current state and note incomplete areas.
- Moved files need link fixup: internal `docs/` cross-references must be
  rewritten as relative book paths.

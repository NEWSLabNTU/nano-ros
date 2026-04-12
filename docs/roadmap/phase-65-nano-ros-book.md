# Phase 65 — nano-ros Book

**Goal**: Produce a self-contained mdbook user guide covering installation,
concepts, platform guides, API reference, and advanced topics. Targets embedded
developers adopting nano-ros.

**Status**: In Progress

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


## Acceptance Criteria

- [ ] `mdbook build book/` succeeds with zero warnings
- [ ] `just book` builds the book
- [ ] All content chapters written or moved
- [ ] No links from book to `docs/` directory
- [ ] All moved docs deleted from `docs/`
- [ ] `docs/` contains only contributor/internal material (design decisions,
      research, roadmap, internal analyses)
- [ ] Code snippets are accurate (match current API)
- [ ] Getting Started path works end-to-end on a fresh `just setup` environment
- [ ] Platform chapters cover all five supported platforms
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

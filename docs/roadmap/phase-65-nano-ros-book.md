# Phase 65 — nano-ros Book (Revision + Reorganization)

**Goal**: Revise and reorganize the mdbook at `book/` for a user-first
structure. Original content (65.1–65.41) archived in
`archived/phase-65-nano-ros-book-original.md`.

**Status**: Complete

**Priority**: Medium

## Book Structure (current)

```
Introduction
# Getting Started
  Installation, Native, Zephyr, FreeRTOS, NuttX, ThreadX, Bare-metal, ESP32, ROS 2 Interop
# User Guide
  RMW Backends, Configuration, Message Generation, Serial Transport, Troubleshooting
# Reference
  Rust API, C API, C++ API, Platform API, Environment Variables, Build Commands
# Concepts
  Architecture, no_std, Platform Model
# Internals
  RMW API Design, RMW API Reference, RMW Zenoh Protocol, Scheduling Models,
  Verification, Real-Time Analysis, Safety, Porting Platform, Adding RMW,
  Board Crate, Platform Customization, Creating Examples, Porting Pitfalls, Contributing
```

## Completed work

### 65.42–65.55 — Revision pass (done)

- [x] 65.42 — C++ API reference (`reference/cpp-api.md`)
- [x] 65.43 — RMW API reference (`internals/rmw-api.md`)
- [x] 65.44 — Platform customization guide (`internals/platform-customization.md`)
- [x] 65.45 — Rust API: Promise, action client, spin_async, spin_period
- [x] 65.46 — C API: action server/client, non-blocking patterns
- [x] 65.47 — Platform API: Phase 80 networking traits
- [x] 65.48 — Architecture diagrams: nros-platform layer, feature axes table
- [x] 65.49–52 — Deduplication: platform porting, config, executor/spin, board crate
- [x] 65.53 — Scheduling models review (current, no changes needed)
- [x] 65.54–55 — SUMMARY.md + cross-link pass

### 65.56–65.64 — Reorganization (done)

- [x] 65.56 — Getting Started: per-platform example pages
- [x] 65.57 — User Guide section (rmw-backends, config, message-gen, serial, troubleshooting)
- [x] 65.58 — Slim Reference (removed config-toml, embedded-tuning, moved rmw refs to internals)
- [x] 65.59 — Slim Concepts (moved rmw-api-design, scheduling-models to internals)
- [x] 65.60 — Internals section (renamed Advanced, moved porting/dev guides)
- [x] 65.61 — SUMMARY.md rewrite
- [x] 65.62 — Cross-link fixes for all moved files
- [x] 65.63 — Deleted empty directories + orphaned files
- [x] 65.64 — `mdbook build book/` zero warnings

### 65.65 + 65.32 — Final cleanup (done)

- [x] 65.65 — CLAUDE.md documentation index updated for 5-section structure
- [x] 65.32 — Deleted superseded docs (`docs/guides/getting-started.md`,
  `docs/reference/micro-ros-comparison.md`)

## Acceptance Criteria

- [x] Getting Started has one page per platform (example-driven)
- [x] User Guide exists with config, RMW selection, message gen, serial, troubleshooting
- [x] Reference is lookup-only (no prose duplication with User Guide)
- [x] Concepts is minimal (3 pages: architecture, no_std, platform model)
- [x] Internals chapter contains all dev/contributor content
- [x] No empty directories or orphaned files
- [x] `mdbook build book/` zero warnings
- [x] CLAUDE.md docs index updated

# Phase 65 — nano-ros Book (Revision + Reorganization)

**Goal**: Revise and reorganize the mdbook at `book/` for a user-first
structure. Original content (65.1–65.41) archived in
`archived/phase-65-nano-ros-book-original.md`.

**Status**: In Progress (65.66–65.72 Porting section)

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

## Porting Section (65.66–65.72)

Add a top-level "Porting" section for users building custom RMW backends,
platform packages, or board crates. Move RMW API Reference back to public
Reference. Absorb scattered Internals porting pages into the new chapters.

### Target structure

```
# Porting
  Overview                    ← decision table + what stays untouched
  Custom RMW Backend          ← nros-rmw traits, use nros-platform for networking,
                                Rust path + C/C++ path (cffi vtable) + example
  Custom Platform             ← nros-platform traits, wiring (Cargo features,
                                ConcretePlatform), Rust + C/C++ paths + example
  Custom Board Package        ← Config, run(), hardware init, driver wiring, example
# Reference (updated)
  + RMW API                   ← moved back from Internals (public for RMW porters)
# Internals (slimmed)
  - porting-platform/*        ← README + implementing absorbed into Porting chapters
  - adding-rmw-backend        ← absorbed into Custom RMW Backend
  - board-crate               ← absorbed into Custom Board Package
  - platform-customization    ← absorbed into Porting Overview
```

### Work items

- [ ] 65.66 — Write `porting/overview.md`
  - [ ] 65.66.1 — Decision table: "I want to…" → which chapter
  - [ ] 65.66.2 — Core vs customizable package summary (from platform-customization.md)
  - [ ] 65.66.3 — Trait requirements table (which traits each RMW needs)

- [ ] 65.67 — Write `porting/custom-rmw.md`
  - [ ] 65.67.1 — nros-rmw trait hierarchy overview (Session, Publisher, etc.)
  - [ ] 65.67.2 — How to use nros-platform for networking (don't roll own sockets)
  - [ ] 65.67.3 — Rust path: implement traits, wire Cargo feature, skeleton example
  - [ ] 65.67.4 — C/C++ path: nros-rmw-cffi vtable, register at init
  - [ ] 65.67.5 — Minimal "hello world" RMW example

- [ ] 65.68 — Write `porting/custom-platform.md`
  - [ ] 65.68.1 — nros-platform trait list (required vs optional)
  - [ ] 65.68.2 — Wiring: new crate, Cargo features, resolve.rs, zpico-sys activation
  - [ ] 65.68.3 — Rust path: inherent methods on ZST, show FreeRTOS as real example
  - [ ] 65.68.4 — C/C++ path: nros-platform-cffi vtable, nros_platform_cffi_register()
  - [ ] 65.68.5 — Networking traits: when to implement PlatformTcp/Udp vs keep C network.c

- [ ] 65.69 — Write `porting/custom-board.md`
  - [ ] 65.69.1 — What a board package provides (Config, run(), hardware init)
  - [ ] 65.69.2 — Board = platform + hardware specifics (PHY/MAC, driver wiring)
  - [ ] 65.69.3 — Annotated skeleton (Cargo.toml, lib.rs with run(), config.rs)
  - [ ] 65.69.4 — Force-link of shim crates (extern crate zpico_platform_shim)

- [ ] 65.70 — Move RMW API Reference back to Reference
  - [ ] 65.70.1 — Move `internals/rmw-api.md` → `reference/rmw-api.md`
  - [ ] 65.70.2 — Update SUMMARY.md

- [ ] 65.71 — Clean up Internals (remove absorbed pages)
  - [ ] 65.71.1 — Delete `internals/porting-platform/README.md` (absorbed into overview + custom-platform)
  - [ ] 65.71.2 — Delete `internals/porting-platform/implementing-a-platform.md` (absorbed into custom-platform)
  - [ ] 65.71.3 — Delete `internals/adding-rmw-backend.md` (absorbed into custom-rmw)
  - [ ] 65.71.4 — Delete `internals/board-crate.md` (absorbed into custom-board)
  - [ ] 65.71.5 — Delete `internals/platform-customization.md` (absorbed into overview)
  - [ ] 65.71.6 — Keep `internals/porting-platform/zenoh-pico.md` + `xrce-dds.md`
    (internal FFI symbol tables — link from custom-rmw for deep reference)

- [ ] 65.72 — Update SUMMARY.md + CLAUDE.md + verify build


## Acceptance Criteria

- [x] Getting Started has one page per platform (example-driven)
- [x] User Guide exists with config, RMW selection, message gen, serial, troubleshooting
- [x] Reference is lookup-only (no prose duplication with User Guide)
- [x] Concepts is minimal (3 pages: architecture, no_std, platform model)
- [x] Internals chapter contains all dev/contributor content
- [x] No empty directories or orphaned files
- [x] `mdbook build book/` zero warnings
- [x] CLAUDE.md docs index updated

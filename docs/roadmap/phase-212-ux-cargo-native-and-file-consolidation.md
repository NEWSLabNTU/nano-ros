# Phase 212 — UX revision: cargo-native flow + component file consolidation

**Status:** OPEN (tracking)
**Priority:** P1
**Depends on:** Phase 211 (orchestration foundation), Phase 175 (Cyclone-Rust cargo path)

## Goal

Make nano-ros's developer surface feel cargo-native for Rust users while keeping
the existing CMake path for C/C++ users. Three concrete pain points captured
from the Phase 211 design review (2026-05-31):

1. **Cyclone-Rust requires CMake.** A single-node Rust example builds with
   `cargo build` for zenoh / xrce but `cargo build --features rmw-cyclonedds`
   can't link because `nros_rmw_cyclonedds_register` lives only in a CMake-built
   sibling crate (Phase 175.A). The user accepts CMake for C/C++; Rust must be
   `cargo` end-to-end.
2. **Single-node ≠ multi-node mental model.** Single-node:
   `cargo build && cargo run`. Multi-node:
   `nros metadata → plan → check → build → deploy`. Two disjoint pipelines;
   no graduation path documented from one to the other.
3. **Component "triple-file dance".** Each component declares itself across
   `component_nros.toml` (or `nros/components/*.toml`), `metadata/*.json`,
   `package.xml`, `Cargo.toml` + `src/lib.rs` (exported FFI symbol), AND the
   launch file's `<node pkg= exec=>`. Five names must hand-align; no
   `nros check` cross-file ref validation catches a mismatch with a useful error.

## Architecture

The current shape:

```text
single-node Rust:
  cargo build → bin
  cargo run → executes
  RMW = cargo feature; zenoh/xrce OK; cyclone needs cmake

multi-node Rust:
  user writes nros.toml + component_nros.toml + metadata/*.json + Cargo.toml + src/
  nros metadata → metadata/*.json
  nros plan → record.json + nros-plan.json
  nros build → entry lib (delegates to cargo)
  nros deploy → spawn binary
```

Target shape (proposed direction, not yet committed):

```text
single-node Rust:
  cargo build → bin                          (unchanged, even for cyclonedds)
  cargo run → executes

multi-node Rust:
  cargo nano-ros plan        OR    cargo nano-ros deploy   (cargo subcommand wraps nros)
  cargo build                       (cargo discovers nros workspace via Cargo.toml metadata)
  one declaration per component, in [package.metadata.nros] of the crate's Cargo.toml
```

## Work Items

### 212.A — Cyclone-Rust under pure cargo

Phase 175.A landed CMake + Corrosion as the Cyclone-Rust example path because:
- `ddsc` is a C library; Cyclone's CMake config exports it as an imported target
  that Corrosion can hand to cargo as a link flag.
- `nros_rmw_cyclonedds_register` is a C++ symbol from a CMake-built rlib (the
  `packages/dds/nros-rmw-cyclonedds/` standalone CMake project).
- The descriptor whole-archive-link trick that keeps the static-init register TU
  alive is a `target_link_options` setting only CMake knows how to apply.

Goal: make `cargo build --features rmw-cyclonedds` work end-to-end on hosted
targets (Phase 175.B already deferred the embedded ddsrt port; 212.A targets
native_sim / x86_64).

- [ ] **Investigate `*-sys` crate for Cyclone DDS.** A `cyclonedds-sys` crate
      that runs the Cyclone CMake build in `build.rs` and emits `cargo:rustc-link-*`
      directives. Pattern matches `freertos-lwip-sys` / `threadx-netx-sys`.
- [ ] **Vendor or auto-locate `ddsc`.** Either a submodule under
      `third-party/dds/cyclonedds/` (already there for the CMake path) that
      `cyclonedds-sys`'s build.rs invokes, or a `pkg-config` lookup for a
      system Cyclone install with a clear error if missing.
- [ ] **Move `nros_rmw_cyclonedds_register` into a Rust-buildable crate.**
      Today the symbol lives in `packages/dds/nros-rmw-cyclonedds/`'s CMake
      project. Either:
      (a) make that crate buildable from cargo via `build.rs` (cc crate +
      the Cyclone idlc-generated descriptors), OR
      (b) split out a smaller Rust-side `nros-rmw-cyclonedds-rs` crate that
      registers the vtable directly via the cffi shim.
- [ ] **`examples/native/rust/talker/` cyclone variant.** Remove the
      "cyclone needs CMake" carve-out from the example's README and the
      CMakeLists.txt cyclonedds path becomes optional (CMake path stays for
      users who want to integrate into an existing CMake project).
- **Files:** `packages/dds/nros-rmw-cyclonedds/` (likely split),
  `packages/dds/cyclonedds-sys/` (new), `examples/native/rust/talker/Cargo.toml`,
  `examples/native/rust/listener/Cargo.toml`,
  `book/src/getting-started/` (drop the Cyclone-needs-CMake caveat).

### 212.B — Cargo-native multi-node workflow

The multi-node pipeline (`nros metadata/plan/check/build/deploy`) is a separate
universe from `cargo build`. Goal: make multi-node feel like a cargo workspace
to a Rust user.

- [ ] **`cargo nano-ros` subcommand.** Wrap every nros CLI verb as a cargo
      subcommand. `cargo nano-ros plan`, `cargo nano-ros deploy`, etc.
      Distribution: `cargo install cargo-nano-ros` (mirror of the existing
      `cargo-nano-ros` crate already in `nros-cli`'s workspace).
- [ ] **Cargo workspace discovery.** Today `nros plan` requires a
      `nros.toml` at the workspace root. A cargo-native flow would also accept
      a `Cargo.toml` `[workspace.metadata.nros]` table — single source for
      both rust workspace AND nros system configuration. Reuse cargo's
      workspace inheritance.
- [ ] **`cargo build` picks up nros codegen.** A `cargo nano-ros` build-script
      helper crate that, when added as a `[build-dependencies]` entry, runs
      `nros generate-rust` automatically on `cargo build`. Removes the
      `nros generate-rust --force` prerequisite step from the single-node
      flow; users get auto-codegen on first `cargo build`.
- [ ] **Document graduation path.** A book page that walks the user from
      `examples/native/rust/talker/` (single-node, pure cargo) to the same
      talker as a workspace component (multi-node, `cargo nano-ros plan/deploy`).
      Same Cargo.toml shape, two new files (workspace `nros.toml` + launch).
- **Files:** `nros-cli` (subcommand impl), `nano-ros`
  (`book/src/user-guide/single-to-multi.md`,
  `examples/templates/multi-package-workspace/`).

### 212.C — Component file consolidation

Each component today carries:
1. `package.xml` — ROS-standard, needed for ament tooling interop.
2. `Cargo.toml` (Rust) or `CMakeLists.txt` (C/C++) — needed for the
   build system.
3. `src/lib.rs` (or src/*.cpp) — the actual implementation; carries the
   `#[nros::component]` macro / `nros_component!` macro that exports the FFI
   symbol.
4. `component_nros.toml` (or `nros/components/<name>.toml` for multi-component
   packages) — declares package/component/language + `[linkage]
   crate_name/executable/exported_symbol` + `[metadata]
   source_metadata="metadata/<n>.json"` + `[overrides]
   default_namespace/parameters/remaps`.
5. `metadata/<comp>.json` — emitted by `nros metadata` from source attributes
   (the `#[nros::publisher]` / `#[nros::timer]` etc. macros).

Goal: shrink this to the minimum a user has to hand-author. Anything
derivable from source should be derived.

- [ ] **Move component declaration into `Cargo.toml`.** Replace
      `component_nros.toml` with a `[package.metadata.nros.component]` table.
      Build / planner / metadata tools read it from the manifest. One fewer
      file, one fewer name to align (Cargo enforces the crate name; the
      `[lib]`/`[[bin]]` target is the natural exported_symbol root).
- [ ] **Auto-derive `metadata/*.json`.** Today `nros metadata` runs a
      build-script-driven attribute scan and emits the JSON. Goal:
      `cargo build` runs the same scan as a `build.rs` step and emits the
      JSON into `target/nros-metadata/<crate>.json`. The planner reads that
      path directly instead of a committed `metadata/*.json`. Removes the
      committed `metadata/` dir from every component (it becomes a build
      artifact like `target/`).
- [ ] **Audit `[overrides]`.** The
      `default_namespace`/`parameters`/`remaps` table in `component_nros.toml`
      is rarely set in practice (most components inherit from launch file
      overrides). Survey existing fixtures + Autoware: is this table actually
      used? If <10% adoption, drop it; if used, move into
      `[package.metadata.nros.component.overrides]`.
- [ ] **`package.xml` still required** (ament interop), but
      auto-generate it from `Cargo.toml`'s `[package.metadata.nros]`
      `dependencies` list at codegen time. User maintains the cargo manifest;
      ament tooling gets the package.xml. (Or accept the duplication — this
      is the smallest pain of the five.)
- **Files:** `nros-cli`
  (`packages/nros-cli-core/src/orchestration/workspace.rs`,
  `metadata_build.rs`), `nano-ros` (every fixture under
  `packages/testing/nros-tests/fixtures/orchestration_*` migrates).

## Acceptance

- [ ] **Single-node Rust = `cargo build && cargo run` for ALL three RMWs**
      (zenoh, xrce, cyclonedds). No CMake step. (212.A)
- [ ] **Multi-node Rust = `cargo nano-ros plan && cargo nano-ros deploy`** —
      no separate `nros` binary install required for Rust users. (212.B)
- [ ] **One file per component for the user** — `Cargo.toml` carries the
      `[package.metadata.nros]` table; `metadata/*.json` becomes a build
      artifact; `component_nros.toml` retired. (212.C)
- [ ] **End-to-end multi-node tutorial** in the book that walks from
      single-node to multi-node without changing build tools. (212.B last
      bullet)

## Notes

- C/C++ flow stays CMake-based. 212.A/B/C target Rust UX only.
  The CMake `nano_ros_generate_interfaces()` + `find_package(NanoRos)` path
  (Phase 137 / 140 / 144) continues to serve C/C++ users.
- Component file consolidation (212.C) must not break the existing
  `nros/components/*.toml` multi-component layout — backwards-compat is
  important since several in-tree fixtures + Autoware integration depend on
  the current shape. A migration script (`nros migrate component-to-cargo`)
  may be needed.
- 212.B's `cargo nano-ros` subcommand benefits from prior `cargo-nano-ros`
  crate work in `nros-cli` (already named that way) — likely incremental
  surface expansion rather than a new crate from scratch.
- Phase 175.B (embedded Cyclone Rust w/ ddsrt port) remains research-grade
  and out of scope for 212.A; 212.A only targets hosted (native_sim / x86_64).

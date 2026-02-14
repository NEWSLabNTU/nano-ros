# Phase 27: Message Codegen Automation

## Goal

Improve the ROS 2 message codegen experience for nano-ros users:
1. **Bundled interfaces**: Ship standard `.msg` files so codegen works without ROS 2 sourced
2. **Inline codegen mode**: Alternative code generation mode for single-crate use cases
3. **heapless re-export**: Reduce dependency count for generated code
4. **CLI rename**: Standalone `nano-ros` binary alongside `cargo nano-ros`
5. **CMake improvements**: Use bundled interfaces when ament index unavailable
6. **Bundled codegen library (27B)**: Eliminate need to install `nano-ros` binary for C users

## Status: Complete (27B In Progress)

### Completed
- Phase 0: Bundle standard interface files
- Phase 1: Inline codegen mode in rosidl-codegen
- Phase 2: nano-ros-core heapless re-export
- Phase 4: CLI rename to standalone `nano-ros` binary
- Phase 5: CMake generator discovery updated for standalone binary
- Phase 6: Documentation updates

### Dropped
- **Phase 3: nano-ros-build crate (build.rs automation)** — Dropped because inline mode
  causes type conflicts in multi-crate workspaces (each crate gets its own copy of
  `std_msgs::Int32`, making types incompatible across crate boundaries) and duplicates
  compilation of the same message packages. The primary workflow remains
  `cargo nano-ros generate-rust` which produces separate shared crates.

## Background

The codegen pipeline (`cargo nano-ros generate-rust`) requires:
- A ROS 2 environment sourced (for ament index to locate `.msg` files)
- Manual invocation before building
- Each generated package as a separate Cargo crate with cross-package references via crate names

This phase addresses the first point (bundled interfaces for offline codegen) and adds
infrastructure for future automation possibilities.

## Design Decisions

### Primary Workflow: Separate Crates (cargo nano-ros generate)

The primary codegen method generates one Cargo crate per ROS package. This approach:
- Compiles each package once (shared across workspace)
- Produces compatible types across crate boundaries
- Works well with Cargo's dependency resolution

### Inline Codegen Mode (available but not primary)

An alternative inline mode exists where all packages merge into a single module tree:

```
generated/                        # root module
├── std_msgs/
│   └── msg/
│       └── header.rs             # cross-ref: super::super::super::builtin_interfaces::msg::Time
├── builtin_interfaces/
│   └── msg/
│       └── time.rs
└── mod.rs                        # pub mod std_msgs; pub mod builtin_interfaces;
```

Cross-ref paths:
- **Crate mode** (primary): `builtin_interfaces::msg::Time`
- **Inline mode**: `super::super::super::builtin_interfaces::msg::Time`

Inline mode uses `nano_ros_core::` prefixed imports instead of `nano_ros_serdes::`.

**When to use inline mode**: Single-crate projects where type sharing isn't needed.
**When NOT to use**: Multi-crate workspaces (causes type conflicts and duplicate compilation).

### Bundled Standard Interfaces

Common `.msg`/`.srv` files ship in `packages/codegen/interfaces/`:
- `std_msgs` (Bool, Int32, String, Header, etc.)
- `builtin_interfaces` (Time, Duration)
- `rcl_interfaces` (already vendored in `packages/interfaces/rcl-interfaces/`)

The ament index (from ROS 2 environment) takes precedence; bundled files fill gaps.

### heapless Re-export

`nano-ros-core` re-exports `heapless` (`pub use heapless;`) so generated code can
reference `nano_ros_core::heapless::String<256>` in inline mode.

## Implementation Details

### Phase 0: Bundle Standard Interface Files (Complete)

**Files created**:
- `packages/codegen/interfaces/std_msgs/msg/*.msg` — Copied from ROS 2
- `packages/codegen/interfaces/std_msgs/package.xml`
- `packages/codegen/interfaces/builtin_interfaces/msg/*.msg` — Copied from ROS 2
- `packages/codegen/interfaces/builtin_interfaces/package.xml`

**Files modified**:
- `packages/codegen/packages/rosidl-bindgen/src/ament.rs` — Added `from_directory()`, `merge()`
- `packages/codegen/packages/cargo-nano-ros/src/lib.rs` — Added `load_index_with_fallback()`

### Phase 1: Inline Codegen Mode (Complete)

**Files modified**:
- `rosidl-codegen/src/types.rs` — Added `NanoRosCodegenMode` enum, `nano_ros_type_for_field_with_mode()`
- `rosidl-codegen/src/templates.rs` — Added `inline_mode: bool` to nano-ros template structs
- `rosidl-codegen/src/generator.rs` — Added `generate_nano_ros_inline_{message,service,action}()`
- `rosidl-codegen/templates/message_nano_ros.rs.jinja` — Conditional `nano_ros_core::` imports
- `rosidl-codegen/templates/service_nano_ros.rs.jinja` — Same
- `rosidl-codegen/templates/action_nano_ros.rs.jinja` — Same
- `rosidl-codegen/src/lib.rs` — Exported new functions and types

### Phase 2: heapless Re-export (Complete)

**Files modified**:
- `packages/core/nano-ros-core/Cargo.toml` — Added `heapless = { workspace = true }`
- `packages/core/nano-ros-core/src/lib.rs` — Added `pub use heapless;`

### Phase 4: CLI Rename (Complete)

Added standalone `nano-ros` binary alongside `cargo-nano-ros` for backward compatibility.
Both binaries share the same library code. The standalone binary is simpler to invoke
(`nano-ros generate-rust` vs `cargo nano-ros generate-rust`).

**Files created**:
- `packages/codegen/packages/cargo-nano-ros/src/standalone.rs` — Standalone binary entry point

**Files modified**:
- `packages/codegen/packages/cargo-nano-ros/Cargo.toml` — Added second `[[bin]]` entry

### Phase 5: CMake Generator Discovery (Complete)

Updated CMake `_nano_ros_find_generator()` to prefer the standalone `nano-ros` binary
over `cargo-nano-ros`, simplifying the invocation (no `nano-ros` prefix arg needed).

**Files modified**:
- `packages/core/nano-ros-c/cmake/nano_ros_generate_interfaces.cmake` — Updated binary search order

### Phase 6: Documentation (Complete)

Updated `docs/message-generation.md` and `CLAUDE.md` to reflect:
- Standalone `nano-ros` binary alongside `cargo nano-ros`
- Bundled interfaces for offline codegen
- Updated prerequisites (ROS 2 optional for standard types)

## Verification

```bash
# After each phase:
just quality

# Phase 4 verification (CLI):
nano-ros generate-rust --help
nano-ros generate --help  # backward compat (hidden)

# Offline codegen (no ROS 2 sourced):
cargo nano-ros generate-rust  # Should work with bundled interfaces

# Phase 27B verification (bundled codegen library):
just build-codegen-lib
cd examples/native/c-custom-msg && rm -rf build && mkdir build && cd build
cmake -DNANO_ROS_ROOT=/path/to/nano-ros .. && make
```

## Phase 27B: Bundled Codegen Library for CMake

### Goal

Eliminate the need for C users to separately install the `nano-ros` binary. The CMake
`nano_ros_generate_interfaces()` function currently calls an external program that users
must install via `cargo install`. Phase 27B bundles the codegen as a staticlib with
a thin C wrapper that CMake builds at configure time.

Also rename the CLI `generate` subcommand to `generate-rust` for consistency with `generate-c`.

### Status: Complete

### Design Decisions

- **Rust staticlib with C FFI**: `nano-ros-codegen-c` crate exposes `nano_ros_codegen_generate_c()`
  as a C-callable function, compiled to a `.a` static library
- **Thin C wrapper**: `codegen_main.c` provides a `main()` that parses `--args-file` and
  calls the Rust function — shipped alongside the CMake module
- **CMake `try_compile`**: `FindNanoRosCodegen.cmake` builds the wrapper at configure time,
  linking against the staticlib
- **No fallback to external binaries**: `nano_ros_generate_interfaces()` uses only the
  bundled tool. If the staticlib isn't built, CMake emits a `FATAL_ERROR` with build instructions
- **JSON args file unchanged**: Same data interchange format as before

### Implementation

**New crate:** `packages/codegen/packages/nano-ros-codegen-c/`
- `Cargo.toml` — staticlib crate depending on `cargo-nano-ros`
- `src/lib.rs` — Single `extern "C"` function wrapping `generate_c_from_args_file()`
- `include/nano_ros_codegen.h` — C header
- `src/codegen_main.c` — Thin wrapper with `main()`

**New CMake module:** `cmake/FindNanoRosCodegen.cmake`
- Finds staticlib and wrapper source
- `try_compile` builds wrapper executable at configure time
- Sets `_NANO_ROS_CODEGEN_TOOL` cache variable

**Modified files:**
- `packages/core/nano-ros-c/cmake/nano_ros_generate_interfaces.cmake` — Uses `FindNanoRosCodegen`
  instead of `_nano_ros_find_generator()`
- `packages/codegen/packages/Cargo.toml` — Added `nano-ros-codegen-c` to workspace
- `packages/codegen/packages/cargo-nano-ros/src/standalone.rs` — `generate` → `generate-rust`
  with hidden backward-compat alias
- `packages/codegen/packages/cargo-nano-ros/src/main.rs` — Same rename
- `justfile` — Added `build-codegen-lib` recipe

## Key Files

| Component | File |
|-----------|------|
| Type resolution | `packages/codegen/packages/rosidl-codegen/src/types.rs` |
| Message template | `packages/codegen/packages/rosidl-codegen/templates/message_nano_ros.rs.jinja` |
| Template structs | `packages/codegen/packages/rosidl-codegen/src/templates.rs` |
| Generator functions | `packages/codegen/packages/rosidl-codegen/src/generator.rs` |
| CLI main | `packages/codegen/packages/cargo-nano-ros/src/main.rs` |
| CLI lib | `packages/codegen/packages/cargo-nano-ros/src/lib.rs` |
| CLI standalone | `packages/codegen/packages/cargo-nano-ros/src/standalone.rs` |
| CMake integration | `packages/core/nano-ros-c/cmake/nano_ros_generate_interfaces.cmake` |
| CMake codegen finder | `cmake/FindNanoRosCodegen.cmake` |
| Codegen staticlib | `packages/codegen/packages/nano-ros-codegen-c/` |
| nano-ros-core lib | `packages/core/nano-ros-core/src/lib.rs` |
| Bundled interfaces | `packages/codegen/interfaces/` |
| Ament index | `packages/codegen/packages/rosidl-bindgen/src/ament.rs` |

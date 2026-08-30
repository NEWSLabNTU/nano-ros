# Contributing to colcon-cargo-ros2

Thank you for your interest in contributing! This document provides development setup instructions, architecture details, and guidelines for contributors.

## Table of Contents

- [Development Setup](#development-setup)
- [Repository Structure](#repository-structure)
- [Build Workflows](#build-workflows)
- [Testing](#testing)
- [Code Quality](#code-quality)
- [Architecture](#architecture)
- [Release Process](#release-process)

## Development Setup

### Prerequisites

- **ROS 2**: Humble, Iron, or Jazzy
- **Rust**: 1.70+ (stable and nightly toolchains)
- **Python**: 3.8+
- **Build Tools**: maturin, just (optional but recommended)

### Environment Setup

**Automatic (Recommended)**:
```bash
# Install direnv: https://direnv.net/
direnv allow  # Once - permits .envrc to run

# ROS 2 environment now automatically loads when entering directory
cd colcon-cargo-ros2/  # ✅ Sourced ROS 2 environment
```

**Manual**:
```bash
source /opt/ros/jazzy/setup.bash  # Or humble, iron, etc.
```

### Clone and Install

```bash
# Clone repository
git clone https://github.com/jerry73204/colcon-cargo-ros2.git
cd colcon-cargo-ros2

# Install in development mode
pip install -e packages/colcon-cargo-ros2/

# Or use justfile
just install-python
```

## Repository Structure

```
colcon-cargo-ros2/
├── .envrc                        # Automatic ROS 2 environment setup (direnv)
├── CLAUDE.md                     # AI assistant project instructions
├── README.md                     # User-facing documentation
├── CONTRIBUTING.md               # This file (developer documentation)
├── justfile                      # Build automation (dual workspace + Python)
│
├── # USER-FACING LIBRARIES (requires ROS 2)
├── user-libs/
│   ├── Cargo.toml                # Workspace manifest
│   ├── rclrs/                    # ROS 2 client library for Rust
│   └── rosidl-runtime-rs/        # Runtime library for ROS messages
│
├── # BUILD INFRASTRUCTURE (no ROS required)
├── build-tools/
│   ├── Cargo.toml                # Workspace manifest
│   ├── rosidl-parser/            # ROS IDL parser (.msg, .srv, .action)
│   ├── rosidl-codegen/           # Code generator with Askama templates
│   ├── rosidl-bindgen/           # Binding generator (embeds user-libs)
│   ├── cargo-ros2/               # Build orchestrator
│   └── colcon-cargo-ros2/        # Python/PyO3 colcon extension
│       ├── colcon_cargo_ros2/    # Python module
│       ├── cargo-ros2-py/        # Rust library exposed to Python
│       └── test/                 # Python tests
│
├── # PACKAGES (published to PyPI)
├── packages/
│   └── colcon-cargo-ros2/        # Python package directory
│       ├── pyproject.toml        # Package metadata
│       ├── Cargo.toml            # Rust extension manifest
│       └── colcon_cargo_ros2/    # Python source (symlinks to build-tools)
│
└── # BUILD & CI
    ├── .github/workflows/        # CI for both workspaces
    ├── .gitignore                # Rust + Python patterns
    └── docs/                     # Architecture and design docs
```

### Dual Workspace Architecture

The project uses two independent Cargo workspaces:

**user-libs/** (Requires ROS 2 environment):
- `rclrs`: ROS 2 client library (Node, Publisher, Subscription)
- `rosidl-runtime-rs`: Runtime support (Message trait, Sequence, String)

**build-tools/** (No ROS required):
- `rosidl-parser`: IDL parsing logic
- `rosidl-codegen`: Code generation with templates
- `rosidl-bindgen`: Embeds user-libs at compile time
- `cargo-ros2`: Build orchestrator
- `colcon-cargo-ros2`: Python/PyO3 colcon extension

**Benefits**:
- Independent development: Work on build tools without ROS environment
- Faster CI: Build tools workspace doesn't require ROS installation
- Clear separation: User-facing APIs separate from build infrastructure
- Embedded user-libs: `rosidl-bindgen` embeds `rclrs` at compile time using `include_dir!`

## Build Workflows

### Quick Commands

```bash
# Build everything (both workspaces + Python wheel)
just build

# Test everything
just test

# Format all code
just format

# Lint all code
just check

# Run full quality checks (format + lint + test)
just quality

# Clean everything
just clean
```

### Build-Tools Workspace (No ROS Required)

```bash
# Build build-tools workspace
just build-build-tools

# Test build-tools workspace
just test-build-tools

# Clean build-tools workspace
just clean-build-tools

# Or use cargo directly
cd build-tools && cargo build --workspace
```

### User-Libs Workspace (Requires ROS 2)

```bash
# Ensure ROS is sourced first!
source /opt/ros/jazzy/setup.bash  # Or use direnv

# Build user-libs workspace
just build-user-libs

# Test user-libs workspace
just test-user-libs

# Clean user-libs workspace
just clean-user-libs

# Or use cargo directly
cd user-libs && cargo build --workspace
```

### Python Package (colcon-cargo-ros2)

```bash
# Build Python wheel (includes Rust extension via maturin)
just build-python

# Install wheel
just install

# Or install in development mode (editable)
just install-python
```

### After Template Changes

**CRITICAL**: Askama embeds templates at compile time. After modifying `.jinja` templates:

```bash
just clean-build-tools   # Clear build cache (forces template re-embedding)
just build-python        # Rebuild wheel (includes cargo-ros2 + rosidl-bindgen)
just install             # Install wheel with all tools
```

### Testing Changes in a Workspace

```bash
# After making changes and rebuilding
cd testing_workspaces/complex_workspace
rm -rf build/ros2_bindings build/.colcon src/*/.cargo/config.toml
colcon build --symlink-install
```

## Testing

### Run All Tests

```bash
just test               # Test both workspaces + Python
just test-build-tools   # Test build-tools only (no ROS required)
just test-user-libs     # Test user-libs only (requires ROS)
just test-python        # Test Python only
```

### Run Specific Tests

```bash
# Rust tests
cd build-tools
cargo test --package rosidl-parser
cargo test --package rosidl-codegen
cargo test --package cargo-ros2

# Python tests
cd packages/colcon-cargo-ros2
pytest test/
```

### Test Coverage

Current test coverage:
- **194 tests** in build-tools workspace (rosidl-parser, rosidl-codegen, rosidl-bindgen, cargo-ros2)
- **Edge cases**: Arrays, strings, constants, nested messages, keywords
- **Integration tests**: Full workflow, caching, config patching
- **Parity tests**: Generated code matches ros2_rust reference implementation

## Code Quality

### Pre-Commit Requirements

**ALWAYS run `just quality` before committing**:

```bash
just quality  # Format + lint + test (REQUIRED)
```

This ensures:
- Code formatted with nightly rustfmt (both workspaces)
- All clippy warnings fixed (treated as errors with `-D warnings`)
- All tests pass (Rust + Python)
- Zero warnings in codebase

### Individual Quality Commands

```bash
just format               # Format code only
just check                # Lint only
just test                 # Test only

# Per-half (this workspace is Rust packages + a Python package)
just format-packages      # Format the Rust packages only
just check-packages       # Lint the Rust packages only
just format-python        # Format Python only
just check-python         # Lint Python only
```

### Code Style Guidelines

1. **File Operations**: Always use Write/Edit tools, never bash commands (`cat`, `echo`)
2. **Temporary Files**: Create in `tmp/` directory
3. **Documentation**: Update CLAUDE.md and docs/ when changing architecture
4. **Error Handling**: No Pokemon exception handling (empty catch-all blocks)
5. **String Formatting**: Use named parameters (e.g., `println!("{e}")` not `println!("{}", e)`)

## Architecture

### Two-Tool Design

**cargo-ros2-bindgen** (Low-level binding generator):
- Generates Rust bindings for a single ROS interface package
- Can be used standalone
- Pure Rust implementation with native IDL parser

**colcon-cargo-ros2** (High-level colcon integration):
- Wraps `cargo-ros2-bindgen` for workspace builds
- Manages workspace-level binding cache
- Handles ament-compatible installation
- Three-phase workflow: generate bindings → build → install

### Workspace-Level Binding Generation

In colcon workspaces, bindings are generated once at workspace level:

```
workspace/
├── build/
│   └── ros2_bindings/            # Generated once, shared by all packages
│       ├── std_msgs/
│       ├── geometry_msgs/
│       └── custom_interfaces/
└── src/
    ├── robot_controller/
    │   └── .cargo/config.toml    # Points to ../../build/ros2_bindings/
    └── robot_driver/
        └── .cargo/config.toml    # Also points to ../../build/ros2_bindings/
```

**Benefits**:
- No duplication of generated code
- Faster builds
- Smaller disk usage
- Follows ROS conventions

### Shared Runtime Library

`rosidl_runtime_rs` provides:

1. **FFI Layer** (`ffi.rs`): Raw C bindings to `rosidl_runtime_c`
   - String operations (init, fini, assign, copy, are_equal)
   - Primitive sequence operations for all types (f32, f64, i8-i64, u8-u64, bool)

2. **Idiomatic API** (`string.rs`, `sequence.rs`): Safe Rust wrappers
   - Automatic memory management via Drop
   - Conversions to/from Rust std types
   - Clone and PartialEq implementations

3. **Core Traits** (`traits.rs`):
   - `SequenceElement`: Type relationships (idiomatic ↔ RMW)
   - `SequenceAlloc`: Message-specific sequence operations
   - `Message`, `RmwMessage`, `Service`, `Action`: Core ROS type traits

This eliminates 100+ lines of duplicated code per generated package.

### colcon Integration

The Python package registers three extension points:

1. **Package Augmentation** (`colcon_core.package_augmentation`):
   - Runs after package discovery, before build tasks
   - Generates workspace-level bindings once
   - Leverages colcon's native package discovery (respects `--base-paths`, `--packages-select`)

2. **Build Task** (`colcon_core.task.build`):
   - Builds individual Rust packages
   - Uses workspace-level bindings via `--config` flag
   - Installs to ament layout

3. **Test Task** (`colcon_core.task.test`):
   - Runs `cargo test` for Rust packages

## Release Process

### Version Bumping

1. Update version in `packages/colcon-cargo-ros2/pyproject.toml`
2. Update version in `build-tools/colcon-cargo-ros2/Cargo.toml`
3. Update `CLAUDE.md` status section
4. Create git tag: `git tag -a v0.2.0 -m "Release 0.2.0"`

### Building Distribution

**There is no wheel release process, and this section used to describe one in
detail.** Every part of it was stale — checked, not assumed:

| the doc said | reality |
| --- | --- |
| `.github/workflows/wheels.yml` builds + publishes to PyPI | no such workflow in the repo |
| `.github/workflows/test-build.yml` | no such workflow |
| `just build-python`, `just publish-check`, `just publish-test`, `just publish` | none of these recipes exist |
| `packages/colcon-cargo-ros2/` | the path is `packages/cli/colcon-cargo-ros2/` |
| `build-tools/colcon-cargo-ros2/Cargo.toml` | no `build-tools/` directory |

It also described wheels for macOS and Windows, which is not what this project
targets — phase-260 dropped macOS as a host.

What IS true: `packages/cli/colcon-cargo-ros2/` declares
`build-backend = "maturin"`, so a wheel can be built locally with maturin
directly. Nothing automates it, nothing publishes it, and no CI lane exercises
it.

If wheel publishing is wanted again, it needs a workflow and recipes first — and
this section rewritten from what they actually do, rather than restored from
what these ones claimed.

### Post-Release

1. Create GitHub release with changelog
2. Update documentation if needed
3. Announce on ROS Discourse and Rust forums

## Documentation

- **CLAUDE.md** - AI assistant project instructions
- **README.md** - User-facing documentation (this is what users see)
- **CONTRIBUTING.md** - This file (developer documentation)
- **docs/ARCH.md** - Detailed architecture documentation
- **docs/DESIGN.md** - Implementation details
- **docs/ROADMAP.md** - Development roadmap

## Getting Help

- **Questions**: Open a [GitHub Discussion](https://github.com/jerry73204/colcon-cargo-ros2/discussions)
- **Bug Reports**: Open a [GitHub Issue](https://github.com/jerry73204/colcon-cargo-ros2/issues)
- **Feature Requests**: Open a [GitHub Issue](https://github.com/jerry73204/colcon-cargo-ros2/issues) with "enhancement" label

## Code of Conduct

Be respectful, constructive, and collaborative. We follow the [Contributor Covenant Code of Conduct](https://www.contributor-covenant.org/version/2/1/code_of_conduct/).

## License

All contributions are licensed under Apache-2.0 (same as the project).

---

Thank you for contributing to colcon-cargo-ros2!

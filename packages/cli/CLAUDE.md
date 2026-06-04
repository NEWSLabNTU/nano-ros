# colcon-cargo-ros2: Development Guide

**Build Rust ROS 2 packages with automatic message binding generation.**

This file contains development instructions for Claude Code. User documentation is in [README.md](README.md).

## Repository Structure

```
colcon-cargo-ros2/  (THIS REPOSITORY)
├── .envrc                        # Automatic ROS 2 environment setup (direnv)
├── CLAUDE.md                     # This file (development instructions)
├── README.md                     # User-facing documentation
├── justfile                      # Build automation (dual workspace + Python)
│
├── # USER-FACING LIBRARIES (requires ROS 2)
├── user-libs/
│   ├── Cargo.toml                # Workspace manifest
│   ├── rclrs/                    # ROS 2 client library for Rust
│   └── rosidl-runtime-rs/        # Runtime library for ROS messages
│
├── # BUILD INFRASTRUCTURE (no ROS required)
├── packages/                     # Renamed from build-tools/
│   ├── Cargo.toml                # Workspace manifest
│   ├── rosidl-parser/            # ROS IDL parser (.msg, .srv, .action)
│   ├── rosidl-codegen/           # Code generator with Askama templates
│   ├── rosidl-bindgen/           # Binding generator
│   ├── cargo-ros2/               # Build orchestrator (pre-build + post-build)
│   └── colcon-cargo-ros2/        # Python/PyO3 colcon extension
│       ├── colcon_cargo_ros2/    # Python module
│       ├── cargo-ros2-py/        # Rust library exposed to Python
│       └── test/                 # Python tests
│
└── .github/workflows/            # CI for both workspaces
```

## Development Workflows

### Environment Setup

**Automatic (Recommended)**:
```bash
# Install direnv: https://direnv.net/
direnv allow  # Once - permits .envrc to run
# ROS 2 environment now automatically loads when entering directory
```

**Manual**:
```bash
source /opt/ros/jazzy/setup.bash  # Or humble, iron, etc.
```

### Quick Build Commands

```bash
# Build everything (both workspaces + Python wheel)
just build

# Test everything
just test

# Format and lint all code
just format
just check

# Full quality check (format + lint + test)
just quality

# Clean everything
just clean
```

### Workspace-Specific Commands

```bash
# Build-tools/packages workspace (no ROS required)
just build-build-tools
just test-build-tools
just clean-build-tools

# User-libs workspace (requires ROS)
just build-user-libs
just test-user-libs
just clean-user-libs

# Python package
just build-python
just install-python
just install  # Install from wheel
```

### Version Management

```bash
# Bump version in both pyproject.toml and Cargo.toml
just bump-version 0.4.0

# Output:
# ✓ Updated packages/colcon-cargo-ros2/pyproject.toml
# ✓ Updated packages/colcon-cargo-ros2/Cargo.toml
# Next steps:
#   1. Review changes: git diff
#   2. Commit: git add -u && git commit -m 'Bump version to 0.4.0'
#   3. Tag: git tag v0.4.0
#   4. Build: just build-python
```

### Development Cycle

**CRITICAL**: After modifying code, rebuild and reinstall to see changes:

#### Template Changes (Most Important!)

Askama embeds templates at compile time. Simply rebuilding isn't enough - you must **clean and reinstall**:

```bash
# REQUIRED after modifying .jinja templates
just clean-build-tools   # Clear build cache (forces template re-embedding)
just build-python        # Rebuild wheel (includes cargo-ros2 + rosidl-bindgen)
just install             # Install wheel with all tools
```

**Why this matters**:
- Templates in `build-tools/rosidl-codegen/templates/*.jinja` are embedded into `rosidl-bindgen` at compile time
- Without cleaning, Askama may use cached template artifacts
- The Python wheel bundles the Rust tools, so `just build-python` rebuilds everything
- Without reinstalling, the old wheel continues to be used

#### Code Changes (Less Critical)

For regular code changes (not templates):

```bash
just build-python    # Rebuild wheel
just install         # Install updated wheel
```

#### Testing Workspaces

**IMPORTANT**: Always use `just clean && just build` in testing workspaces if a justfile exists:

```bash
# Testing workspaces with justfiles (all 4 have them):
# - testing_workspaces/my_robot_node/
# - testing_workspaces/autoware_msgs_test/
# - testing_workspaces/complex_workspace/
# - testing_workspaces/ros2_rust_examples/

cd testing_workspaces/my_robot_node
just clean && just build

# ❌ NEVER use: rm -rf build install && colcon build
# ✅ ALWAYS use: just clean && just build
```

**Rationale**:
- Justfiles provide consistent build commands across all testing workspaces
- They handle proper cleanup and environment setup
- They abstract away colcon-specific details
- They make build commands shorter and easier to remember

#### Quick Development Cycle

```bash
# 1. Make changes to code/templates

# 2. Clean and rebuild (if templates changed)
just clean-build-tools
just build-python
just install

# 3. Test changes in workspace
cd testing_workspaces/complex_workspace
just clean && just build

# 4. Verify results
```

**Common Mistake**: Forgetting to rebuild the wheel after making changes. The Python wheel bundles the Rust binaries, so changes won't take effect until you run `just build-python && just install`.

### `[package.metadata.ros]` Installation Support (2025-11-17)

**Problem**: Rust ROS 2 packages often need to install additional files beyond binaries (launch files, config files, URDF models, RViz configs, etc.), but there was no way to specify these in Cargo.toml.

**Solution**: Implemented `[package.metadata.ros]` support for installing additional files and directories, providing 100% backward compatibility with cargo-ament-build:

```toml
[package.metadata.ros]
install_to_share = ["launch", "config", "README.md"]  # Directories and files
install_to_include = ["include", "mylib.h"]
install_to_lib = ["scripts", "setup.sh"]
```

**Semantics**:
- **Directories**: Copied recursively with name preserved
  - `"launch"` → `install/<pkg>/share/<pkg>/launch/`
- **Individual files**: Filename preserved (parent path dropped)
  - `"README.md"` → `install/<pkg>/share/<pkg>/README.md`
  - `"config/params.yaml"` → `install/<pkg>/share/<pkg>/params.yaml`
- **Missing paths**: Build fails with clear error message showing expected location

**Destination Mapping**:
- `install_to_share` → `install/<pkg>/share/<pkg>/`
- `install_to_include` → `install/<pkg>/include/<pkg>/`
- `install_to_lib` → `install/<pkg>/lib/<pkg>/`

**Files Modified**:
- `packages/cargo-ros2/src/ament_installer.rs`:
  - Enhanced `install_metadata_ros_files()` documentation with examples
  - Changed from "warn on missing" to "error on missing" for stricter validation
  - Already supported both directories and files (via `.is_dir()` check)
- `packages/cargo-ros2/tests/test_metadata_ros.rs`: Added 4 new integration tests
  - `test_metadata_ros_install_individual_file_to_share`
  - `test_metadata_ros_install_individual_file_to_include`
  - `test_metadata_ros_install_individual_file_to_lib`
  - `test_metadata_ros_mixed_files_and_dirs`
- `tmp/metadata_ros_recommendation.md`: Updated design documentation

**Benefits**:
- ✅ 100% backward compatible with cargo-ament-build
- ✅ Simple and declarative (no complex installation scripts)
- ✅ Supports common ROS 2 use cases (launch, config, urdf, rviz, meshes)
- ✅ Works with both standalone packages and colcon workspaces
- ✅ Clear error messages for missing paths

**Test Coverage**:
- ✅ All 206 tests passing (203 Rust + 3 Python)
- ✅ Test coverage for directories, individual files, and mixed usage
- ✅ Zero clippy warnings

**Example Usage**:
```toml
[package]
name = "my_robot_controller"
version = "0.1.0"

[[bin]]
name = "controller_node"

[package.metadata.ros]
install_to_share = ["launch", "config", "urdf", "README.md", "LICENSE"]
```

Results in:
```
install/my_robot_controller/
├── lib/my_robot_controller/
│   └── controller_node
└── share/my_robot_controller/
    ├── launch/            # Directory preserved
    ├── config/            # Directory preserved
    ├── urdf/              # Directory preserved
    ├── README.md          # Individual file
    ├── LICENSE            # Individual file
    ├── rust/              # Source code (automatic)
    └── package.xml        # Metadata (automatic)
```

---

## Recent Architectural Improvements (2025-11-14)

### Constant Type Fix and Colcon Package Discovery (2025-11-14)

**Problem 1**: String constants in generated code used non-const-compatible types (`String`, `rosidl_runtime_rs::String`), causing compilation errors:
```rust
pub const PASSWORD: String = "hunter2";  // ❌ Cannot initialize const String with &str
```

**Solution**: Created `rust_type_for_constant()` function that returns `&'static str` for all string types in constants:
```rust
pub const PASSWORD: &'static str = "hunter2";  // ✅ Const-compatible
```

**Files Modified**:
- `build-tools/rosidl-codegen/src/types.rs`: Added `rust_type_for_constant()` function
- `build-tools/rosidl-codegen/src/generator.rs`: Updated all constant generation (messages, services, actions) to use new function

---

**Problem 2**: Workspace binding generator hardcoded directory names `["src", "ros"]`, failing to discover packages in non-standard locations (e.g., `rclrs/`):
```python
for src_dir_name in ["src", "ros"]:  # ❌ Doesn't respect colcon's discovery
    src_dir = self.workspace_root / src_dir_name
```

**Solution**: Implemented `PackageAugmentationExtensionPoint` to leverage colcon's native package discovery:
- Receives ALL discovered packages from colcon (respects `--base-paths`, `--packages-select`, etc.)
- Runs after package discovery, before build tasks start
- Eliminates fragile filesystem scanning
- Properly integrates with colcon's architecture

**Files Created**:
- `build-tools/colcon-cargo-ros2/colcon_cargo_ros2/package_augmentation/__init__.py`: New extension point

**Files Modified**:
- `build-tools/colcon-cargo-ros2/colcon_cargo_ros2/workspace_bindgen.py`: Uses packages from `RustBindingAugmentation._interface_packages`
- `build-tools/colcon-cargo-ros2/pyproject.toml`: Registered new `colcon_core.package_augmentation` entry point

**Benefits**:
- **Proper colcon integration**: Uses colcon's package discovery instead of re-implementing it
- **Respects user configuration**: Works with `--base-paths`, `--packages-select`, and other colcon flags
- **No hardcoded paths**: Eliminates fragile directory name assumptions
- **Architecturally correct**: `PackageAugmentationExtensionPoint` is designed for workspace-level operations

---

## Recent Architectural Improvements (2025-11-11)

### Dual Workspace Architecture (2025-11-11)

Split the project into two independent Cargo workspaces to separate concerns:

**Problem**: Building `rclrs` requires ROS 2 environment (`ROS_DISTRO`, sourced setup.bash), which made it impossible to run `cargo check` on build tools without ROS installed.

**Solution**: Separate user-facing libraries from build infrastructure:

```
user-libs/          # Requires ROS 2 environment to build
├── rclrs/          # ROS 2 client library (Node, Publisher, Subscription)
└── rosidl-runtime-rs/  # Runtime support (Message trait, Sequence, String)

build-tools/        # No ROS required - can develop without ROS installed!
├── rosidl-parser/      # IDL parsing logic
├── rosidl-codegen/     # Code generation with templates
├── rosidl-bindgen/     # Binding generator
├── cargo-ros2/         # Build orchestrator
└── colcon-cargo-ros2/  # Python/PyO3 colcon extension
```

**Benefits**:
- **Independent development**: Can work on build tools without ROS environment
- **Faster CI**: Build tools workspace checks don't require ROS installation
- **Clear separation**: User-facing APIs separate from build infrastructure
- **Uses crates.io**: `rclrs` and `rosidl-runtime-rs` are published on crates.io for easy distribution

**New justfile commands**:
```bash
# 1. Make changes to code/templates

# 2. Clean and rebuild (REQUIRED for template changes)
just clean-build-tools   # Only needed if templates changed
just build-python        # Rebuild wheel (includes all Rust tools)
just install             # Install updated wheel

# 3. Test in a workspace
cd ~/test_workspace
rm -rf build/.colcon build/ros2_bindings
colcon build --packages-select <package>
```

**Why**: Templates are embedded at compile time. Python wheel bundles Rust binaries. Must reinstall to use updated tools.

## Key Development Guidelines

### File Manipulation Rules

**CRITICAL: ALWAYS use Write/Edit tools for file operations**

- ❌ **NEVER**: `cat > file`, `echo > file`, `cat <<EOF`, or any shell redirection
- ✅ **ALWAYS**: Use `Write` tool to create files
- ✅ **ALWAYS**: Use `Edit` tool to modify files
- ✅ **ALWAYS**: Use `Read` tool to view files

**Exception**: Bash is only for system commands (git, cargo, colcon, etc.), not file I/O.

### Temporary Files

**IMPORTANT**: Only create temporary files when explicitly requested by the user.

**When temporary files ARE needed** (user explicitly requests them or they're necessary for task execution):
- Create them in `$PROJECT_ROOT/tmp/` using the Write tool
- Examples: test scripts, temporary test data, exploration/debugging files

**Do NOT create**:
- Summary documents or notes in `tmp/` unless user explicitly asks
- Documentation files to record work completed
- Session reports or progress tracking files

**Rationale**: The `tmp/` directory should remain clean and only contain files that are truly temporary or explicitly requested by the user. Work summaries and documentation should be communicated directly to the user, not written to files.

```bash
# ✅ CORRECT - User requested a test script
Write: tmp/test_build.sh
Content: |
  #!/bin/bash
  cargo build --release

Bash: chmod +x tmp/test_build.sh && tmp/test_build.sh

# ✅ CORRECT - Needed for debugging
Write: tmp/test_data.json
Content: {"key": "value"}

# ❌ WRONG - Unsolicited summary document
Write: tmp/session_summary.md
Content: |
  # Session Summary
  Today we completed...

# ❌ WRONG - Using bash redirects instead of Write tool
Bash: echo '{"key": "value"}' > /tmp/test_data.json
Bash: cat > tmp/test.sh <<'EOF'
  #!/bin/bash
  ...
EOF
```

### Code Quality

**REQUIRED before committing**:
```bash
just quality  # Format + lint + test
```

Ensures:
- Code formatted with nightly rustfmt
- All clippy warnings fixed (`-D warnings`)
- All tests pass (Rust + Python)
- Zero warnings

## Recent Architectural Improvements

### Capitalized Boolean Literals Support (2025-11-18)

**Problem**: Parser failed on ROS 2 action files using capitalized boolean literals (`True`/`False`):
```
Failed to parse action: DockRobot: Unexpected token: expected Identifier, got string
```

**Root Cause**: Lexer only recognized `true`/`TRUE` and `false`/`FALSE`, but not `True`/`False`.

**Solution**: Extended lexer token definitions in `packages/rosidl-parser/src/lexer.rs`:
```rust
#[token("true")]
#[token("TRUE")]
#[token("True")]   // Added
True,

#[token("false")]
#[token("FALSE")]
#[token("False")]  // Added
False,
```

**Files Modified**:
- `packages/rosidl-parser/src/lexer.rs`

**Impact**: Successfully parses nav2_msgs actions (DockRobot, etc.) and other packages using capitalized booleans.

**Tests**: Added `lex_capitalized_boolean_literals` test.

---

### Bounded Sequence Support for Services and Actions (2025-11-23)

**Problem**: Service and action idiomatic templates generated incorrect code for bounded sequences, causing type mismatch errors:
```rust
error[E0308]: mismatched types
  --> autoware_adapi_v1_msgs/src/srv/initialize_localization_idiomatic.rs:59:23
   |
59 |                   pose: crate::rosidl_runtime_rs::Sequence::from(
   |                         ^ expected `BoundedSequence<..., 1>`, found `Sequence<_>`
   |
   = note: expected struct `BoundedSequence<geometry_msgs::msg::rmw::PoseWithCovarianceStamped, 1>`
              found struct `Sequence<_>`
```

**Root Cause**: Service and action idiomatic templates were generating `Sequence::from(...)` for all sequences, not distinguishing between bounded and unbounded sequences. The RMW layer correctly used `BoundedSequence<T, N>` for bounded sequences (e.g., `geometry_msgs/PoseWithCovarianceStamped[<=1]`), but the idiomatic layer tried to convert Vec to Sequence instead of BoundedSequence.

**Solution**: Updated service and action idiomatic templates to check for bounded sequences before regular sequences and use `BoundedSequence::try_from(...)` instead of `Sequence::from(...)`:

```jinja
{% else if field.is_bounded_sequence && field.is_primitive_sequence %}
// Vec<primitive> → BoundedSequence<primitive> (use try_from)
{{ field.name }}: crate::rosidl_runtime_rs::BoundedSequence::try_from(idiomatic.{{ field.name }}.clone()).unwrap(),
{% else if field.is_bounded_sequence && field.is_string_sequence %}
// Vec<String> → BoundedSequence<rosidl_runtime_rs::String> (use try_from)
{{ field.name }}: crate::rosidl_runtime_rs::BoundedSequence::try_from(
    idiomatic.{{ field.name }}.iter().map(|item| crate::rosidl_runtime_rs::String::from(item.as_str())).collect::<Vec<_>>()
).unwrap(),
{% else if field.is_bounded_sequence %}
// Vec → BoundedSequence conversion with element conversion for nested messages (use try_from)
{{ field.name }}: crate::rosidl_runtime_rs::BoundedSequence::try_from(
    idiomatic.{{ field.name }}.iter().map(|item| item.into()).collect::<Vec<_>>()
).unwrap(),
```

**Files Modified**:
- `packages/rosidl-codegen/templates/service_idiomatic.rs.jinja` - Added bounded sequence checks for request and response
- `packages/rosidl-codegen/templates/action_idiomatic.rs.jinja` - Added bounded sequence checks for goal, result, and feedback

**Testing**:
- Created `testing_workspaces/autoware_msgs_test` to reproduce the issue with `autoware_adapi_v1_msgs::srv::InitializeLocalization`
- Verified fix builds successfully with Autoware ADAPI messages

**Impact**: Enables proper handling of bounded sequences in services and actions, fixing compilation errors when using Autoware and other packages with bounded sequence fields.

**Note**: Message idiomatic template already had correct bounded sequence handling - this fix brings service and action templates in line with message template.

---

### Action Constant Namespace Collision Fix (2025-11-18)

**Problem**: Action constants with duplicate names across Goal/Result/Feedback sections caused compilation errors:
```rust
error[E0428]: the name `NONE` is defined multiple times
  --> nav2_msgs/src/action/dock_robot_rmw.rs:32:1
   |
13 | pub const NONE: u16 = 0;  // Result section
32 | pub const NONE: u16 = 0;  // Feedback section
   | ^^^^^^^^^^^^^^^^^^^^^^^^ `NONE` redefined here
```

**Root Cause**: All constants were generated at module top level without namespace separation.

**Solution**: Wrapped constants in separate modules in `packages/rosidl-codegen/templates/action_rmw.rs.jinja`:
```rust
// Before (conflicting)
pub const NONE: u16 = 0;  // Result
pub const NONE: u16 = 0;  // Feedback (error!)

// After (namespaced)
pub mod result_constants {
    pub const NONE: u16 = 0;
}
pub mod feedback_constants {
    pub const NONE: u16 = 0;
}
```

**Usage**:
```rust
nav2_msgs::action::rmw::dock_robot::result_constants::NONE
nav2_msgs::action::rmw::dock_robot::feedback_constants::NONE
```

**Files Modified**:
- `packages/rosidl-codegen/templates/action_rmw.rs.jinja`

**Impact**: Enables actions with duplicate constant names (common in ROS 2 ecosystem).

---

### Package Version Extraction from package.xml (2025-11-18)

**Problem**: All generated message crates used hardcoded `ROSIDL_RUNTIME_RS_VERSION` ("0.5") instead of actual ROS package version.

**Solution**:
1. Added `version: String` field to `Package` struct
2. Implemented `parse_package_version()` to extract `<version>` from package.xml
3. Updated `generate_cargo_toml()` to use package version

**Example**:
```toml
# Before (incorrect)
[package]
name = "std_msgs"
version = "0.5.0"  # Wrong - used ROSIDL_RUNTIME_RS_VERSION

# After (correct)
[package]
name = "std_msgs"
version = "5.3.0"  # Correct - from package.xml
```

**Files Modified**:
- `packages/rosidl-bindgen/src/ament.rs`
- `packages/rosidl-bindgen/src/generator.rs`

**Impact**: Generated crates now have correct semantic versions matching ROS packages.

**Tests**: Added 6 new tests for version parsing and generation.

---

### Workspace-Local Library Linking Fix (2025-11-18)

**Problem**: Rust binaries failed to link against workspace-local ROS interface libraries:
```
rust-lld: error: unable to find library -lsplat_msgs__rosidl_typesupport_c
```

**Root Cause**: Cargo `build.rs` linker search paths (`cargo:rustc-link-search`) don't propagate to downstream binaries.

**Solution**: Added `[build]` rustflags to `ros2_cargo_config.toml`:
```toml
[build]
rustflags = [
    "-L", "native=/path/to/install/package/lib",
    "-L", "native=/opt/ros/jazzy/lib"
]
```

**Files Modified**: `packages/colcon-cargo-ros2/colcon_cargo_ros2/workspace_bindgen.py`

**Impact**: Enables workspaces with custom interface packages used by Rust packages.

---

### `[package.metadata.ros]` Installation Support (2025-11-17)

Implemented support for installing additional files (launch, config, URDF, etc.):

```toml
[package.metadata.ros]
install_to_share = ["launch", "config", "README.md"]  # Directories and files
install_to_include = ["include"]
install_to_lib = ["scripts"]
```

**Semantics**:
- **Directories**: Copied recursively with name preserved
- **Individual files**: Filename preserved (parent path dropped)
- **Missing paths**: Build fails with clear error

100% backward compatible with cargo-ament-build.

---

### WString Array/Sequence Support (2025-11-17)

Added complete WString support across all idiomatic templates to fix type mismatches in generated bindings.

---

### `--cargo-args` Support (2025-11-17)

Added ability to pass arguments to Cargo:
```bash
colcon build --cargo-args --release
colcon build --cargo-args --profile dev-release
```

---

### Ruff Linter Migration (2025-11-17)

Migrated from flake8 to ruff (10-100x faster, written in Rust, no plugin dependencies).

---

### Dual Workspace Architecture (2025-11-11)

Split into two independent workspaces:
- **user-libs/**: Requires ROS 2 environment (`rclrs`, `rosidl-runtime-rs`)
- **packages/**: No ROS required (build infrastructure)

Benefits:
- Can develop build tools without ROS installed
- Faster CI for build tools
- Clear separation of concerns

---

### Workspace-Level Shared Bindings (2025-11-07)

Bindings generated once at `build/ros2_bindings/`, shared by all packages:
```
build/ros2_bindings/std_msgs/          # Generated once, shared by all packages
build/ros2_bindings/geometry_msgs/     # Shared by all packages
build/ros2_bindings/sensor_msgs/       # Shared by all packages
```

**How it works**:
- `colcon-cargo-ros2` discovers ROS dependencies from `package.xml` **ONLY**
- Users must declare ROS interface dependencies in `<depend>` tags in package.xml
- Generates bindings to `build/ros2_bindings/<package>/` (workspace-level)
- Bindings are made available to Cargo through environment variables during build
- **No `.cargo/config.toml` patches generated** - colcon-cargo-ros2 handles everything

**Important**: ROS dependencies MUST be declared in package.xml, not just Cargo.toml:
```xml
<!-- In package.xml -->
<depend>std_msgs</depend>
<depend>geometry_msgs</depend>
<depend>autoware_adapi_v1_msgs</depend>
```
```toml
# In Cargo.toml (users maintain this separately)
[dependencies]
std_msgs = "*"
geometry_msgs = "*"
autoware_adapi_v1_msgs = "*"
```

If bindings fail to generate, check that the interface package is declared in package.xml. There is a validation step that will report missing interface packages.

Benefits:
- No duplication (std_msgs generated once, not per-package)
- Faster builds - no Cargo patch overhead
- Smaller workspace - no per-package .cargo directories
- Cleaner build process - colcon-cargo-ros2 manages all binding discovery

## Testing

### Unit Tests

```bash
# All tests
just test

# Workspace-specific
just test-build-tools  # No ROS required
just test-user-libs    # Requires ROS

# Specific package
cd packages/rosidl-codegen && cargo test
```

### Integration Testing Workspaces

**IMPORTANT**: Always use `just` commands for testing workspaces, not direct `colcon build` commands. Each testing workspace has a justfile that handles ROS environment setup and proper build sequencing.

**testing_workspaces/my_robot_node** - Simple single-package workspace:

Demonstrates basic ROS 2 node with standard messages:

```bash
cd testing_workspaces/my_robot_node
just clean && just build  # Build workspace
just run                   # Execute test binary
```

**Coverage**:
- Single Cargo package with package.xml dependencies
- Standard messages: std_msgs, geometry_msgs, sensor_msgs
- Demonstrates dependency-aware binding generation (3-4 packages vs 437)

**testing_workspaces/complex_workspace** - Comprehensive message type testing:

Demonstrates 50+ message/action types from 17 different ROS 2 packages:

```bash
cd testing_workspaces/complex_workspace
just clean && just build  # Build workspace
just run                   # Execute test binary
```

**Coverage includes**:
- **Standard messages** (18 types): std_msgs, builtin_interfaces, geometry_msgs, sensor_msgs
- **Navigation** (5 types): nav_msgs (Odometry, Path, OccupancyGrid), trajectory_msgs
- **Control & Diagnostics** (4 types): control_msgs, diagnostic_msgs
- **Nav2 Actions** (6 types): NavigateToPose, DockRobot (tests capitalized booleans!)
- **Motion Planning** (3 types): moveit_msgs (RobotState, MotionPlanRequest, PlanningScene)
- **Action System** (3 types): action_msgs (GoalInfo, GoalStatus, GoalStatusArray)
- **Custom Interfaces** (6 types): robot_interfaces messages, services, actions

**Key Features Tested**:
- ✅ Capitalized boolean literals in action files
- ✅ Action constants in separate namespaces
- ✅ Complex nested message dependencies
- ✅ Custom interface packages
- ✅ Service and action type generation
- ✅ Workspace-level binding sharing

## CI/CD

GitHub Actions workflows:
- **wheels.yml**: Production builds (triggered by tags, publishes to PyPI)
- **test-build.yml**: Quick validation on PRs

Builds 31 artifacts (30 wheels + sdist) for Linux/macOS/Windows × Python 3.8-3.13.

## Status

**Version**: v0.3.3 (2025-11-23)
**Progress**: 15/20 subphases (75%) | 180 tests passing (177 Rust + 3 Python) | Zero warnings
**Latest**: Documentation clarification ✅, Dependency discovery from package.xml only ✅
**Testing**: Validated with:
- autoware_carla_bridge (118 packages) ✅
- splat-drive workspace (6 packages + custom interfaces) ✅
- complex_workspace (50+ message types from 17 packages) ✅

**Versions**:
- Rust workspace: v0.2.0 (rosidl-parser, rosidl-codegen, rosidl-bindgen, cargo-ros2)
- Python package: v0.3.3 (colcon-cargo-ros2)
- Author: Lin Hsiang-Jui <jerry73204@gmail.com>

**PyPI**: 31 artifacts for Linux/macOS/Windows × Python 3.8-3.13

**Architecture**:
- Two independent workspaces (user-libs + packages)
- Workspace-level binding generation via colcon's package discovery
- Rustflags-based linker search paths for workspace libraries
- Complete ament layout installation
- Package versions extracted from package.xml

**Testing Coverage**:
- **Standard ROS 2 packages**: std_msgs, geometry_msgs, sensor_msgs, nav_msgs, trajectory_msgs, builtin_interfaces
- **Navigation**: nav2_msgs with complex actions (NavigateToPose, DockRobot with capitalized booleans)
- **Motion planning**: moveit_msgs with nested dependencies
- **Control**: control_msgs, diagnostic_msgs, action_msgs
- **Custom interfaces**: Messages, services, actions with full type support

**Key Features**:
- ✅ Capitalized boolean literals (`True`/`False`)
- ✅ Action constants in separate namespaces
- ✅ Correct package versions in generated crates
- ✅ Workspace-local library linking
- ✅ `[package.metadata.ros]` installation
- ✅ `--cargo-args` pass-through
- ✅ Version management with `just bump-version`

**Next**: Phase 3.4 - Enhanced Testing & Documentation

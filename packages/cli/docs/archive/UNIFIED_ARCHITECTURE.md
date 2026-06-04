# cargo-ros2: Unified Architecture (Absorbing cargo-ament-build)

**Version**: 2.0
**Date**: 2025-01-30
**Status**: Design Phase - **NEW UNIFIED APPROACH**

---

## Table of Contents

1. [Overview](#overview)
2. [Architectural Decision](#architectural-decision)
3. [Unified Workflow](#unified-workflow)
4. [Implementation Details](#implementation-details)
5. [colcon-ros-cargo Integration](#colcon-ros-cargo-integration)
6. [Code to Extract](#code-to-extract)
7. [Migration Path](#migration-path)

---

## 1. Overview

### The Unified Vision

**cargo-ros2 will be a complete, all-in-one solution for ROS 2 Rust development**, replacing the need for cargo-ament-build entirely.

```
BEFORE (3 tools):
  colcon-ros-cargo → cargo-ament-build → cargo build
                      (installation)      (compilation)

AFTER (2 tools):
  colcon-ros-cargo → cargo-ros2 → cargo build
                      ├─ Pre-build:  Generate ROS bindings
                      ├─ Build:      Invoke cargo build
                      └─ Post-build: Install to ament layout
```

### Key Benefits

1. **Simpler installation**: Users install `colcon-ros-cargo` + `cargo-ros2` (not 3 tools)
2. **Unified codebase**: All Rust ROS tooling in one place
3. **Better integration**: cargo-ros2 controls the entire build pipeline
4. **Easier maintenance**: One tool to maintain instead of coordinating two

---

## 2. Architectural Decision

### Previous Approach (Rejected)

```
cargo-ros2 (binding generation)
    ↓
cargo-ament-build (installation)
    ↓
Done
```

**Problem**: Two separate tools with coordination overhead

### New Approach (Accepted)

```
cargo-ros2 (all-in-one)
    ├─ Phase 1: Pre-build (binding generation)
    ├─ Phase 2: Build (cargo build/check)
    └─ Phase 3: Post-build (ament installation)
```

**Benefit**: Single unified tool with clear phases

---

## 3. Unified Workflow

### 3.1 Command Interface

```bash
# Primary command (used by colcon-ros-cargo)
cargo ros2 ament-build --install-base <path> -- <cargo-args>

# Phases:
# 1. Generate bindings (if needed)
# 2. Run cargo build (or check for pure libs)
# 3. Install to ament layout
```

### 3.2 Detailed Flow

```
┌─────────────────────────────────────────────────────────────┐
│  User runs: colcon build (or cargo ros2 ament-build)        │
└────────────────────────────┬────────────────────────────────┘
                             │
                             ▼
┌─────────────────────────────────────────────────────────────┐
│  Phase 1: Pre-Build (Binding Generation)                    │
│  ────────────────────────────────────────                   │
│  1. Parse Cargo.toml dependencies                           │
│  2. Discover ROS packages via ament_index                   │
│     - System packages (/opt/ros/*/share/)                   │
│     - Workspace packages (install/*/share/)                 │
│  3. Check cache (.ros2_bindgen_cache)                       │
│  4. Generate bindings → target/ros2_bindings/<pkg>/         │
│     - Cargo.toml (generated package manifest)               │
│     - src/lib.rs, src/msg/, src/srv/, src/action/          │
│     - build.rs (FFI linking)                                │
│  5. Patch .cargo/config.toml                                │
│     [patch.crates-io]                                       │
│     vision_msgs = { path = "target/ros2_bindings/..." }     │
└────────────────────────────┬────────────────────────────────┘
                             │
                             ▼
┌─────────────────────────────────────────────────────────────┐
│  Phase 2: Build                                             │
│  ───────────                                                │
│  1. Determine build strategy:                               │
│     - Pure library (no binaries, rlib only) → cargo check   │
│     - Has binaries or exported libs → cargo build           │
│  2. Execute cargo with forwarded args                       │
│  3. Check exit code                                         │
└────────────────────────────┬────────────────────────────────┘
                             │
                             ▼
┌─────────────────────────────────────────────────────────────┐
│  Phase 3: Post-Build (Ament Installation)                   │
│  ──────────────────────────────────────────                 │
│  [ABSORBED FROM cargo-ament-build]                          │
│                                                             │
│  3.1: Create Package Markers                                │
│      - share/ament_index/resource_index/packages/<pkg>      │
│      - share/ament_index/resource_index/rust_packages/<pkg> │
│                                                             │
│  3.2: Install Source Code                                   │
│      → install/<pkg>/share/<pkg>/rust/                      │
│      - Cargo.toml                                           │
│      - Cargo.lock (if exists)                               │
│      - src/                                                 │
│      - build.rs (if build script exists)                    │
│      - package.xml → install/<pkg>/share/<pkg>/             │
│                                                             │
│  3.3: Install Binaries                                      │
│      → install/<pkg>/lib/<pkg>/                             │
│      - Executables (my_node, my_tool)                       │
│      - Shared libraries (libmy_pkg.so, .dylib, .dll)        │
│      - Static libraries (libmy_pkg.a, .lib)                 │
│      - Respects --target <arch> and --profile <profile>     │
│      - Skips binaries with missing required features        │
│                                                             │
│  3.4: Install Metadata Files                                │
│      (from [package.metadata.ros] in Cargo.toml)            │
│      - install_to_share → install/<pkg>/share/<pkg>/        │
│      - install_to_include → install/<pkg>/include/<pkg>/    │
│      - install_to_lib → install/<pkg>/lib/<pkg>/            │
│                                                             │
│      Example Cargo.toml:                                    │
│      [package.metadata.ros]                                 │
│      install_to_share = ["launch", "config"]                │
│      install_to_include = ["include"]                       │
└─────────────────────────────────────────────────────────────┘
                             │
                             ▼
                         SUCCESS!
```

---

## 4. Implementation Details

### 4.1 Project Structure

```
cargo-ros2/
├── cargo-ros2-cli/          # CLI binary
│   └── src/
│       └── main.rs          # Entry point
├── cargo-ros2-core/         # Core library
│   └── src/
│       ├── lib.rs
│       ├── discovery.rs     # ament_index integration
│       ├── generator.rs     # Binding generation
│       ├── cache.rs         # Cache management
│       ├── patcher.rs       # .cargo/config.toml management
│       ├── parser.rs        # ROS IDL parsing
│       │
│       │ NEW: Installation modules (from cargo-ament-build)
│       ├── installer.rs     # Main installation orchestration
│       ├── marker.rs        # ament_index marker creation
│       ├── source_install.rs # Source code installation
│       ├── binary_install.rs # Binary installation
│       └── metadata_install.rs # Metadata-based file installation
└── cargo-ros2-codegen/      # Code generation
    └── src/
        ├── lib.rs
        ├── msg.rs
        ├── srv.rs
        └── action.rs
```

### 4.2 Command Structure

```rust
// In cargo-ros2-cli/src/main.rs

pub enum Command {
    /// Generate bindings and build (standard usage)
    Build {
        release: bool,
        target: Option<String>,
        package: Option<String>,
        bindings_only: bool,
    },

    /// Generate bindings, build, and install (for colcon)
    AmentBuild {
        install_base: PathBuf,
        build_base: PathBuf,
        forwarded_args: Vec<OsString>,
    },

    /// Cache management
    Cache {
        list: bool,
        rebuild: bool,
        clean: bool,
    },

    // ... other commands
}

fn main() {
    let cmd = parse_args();
    match cmd {
        Command::AmentBuild { install_base, build_base, forwarded_args } => {
            // NEW: Unified workflow
            let builder = AmentBuilder::new(install_base, build_base)?;
            builder.run(forwarded_args)?;
        }
        // ... other commands
    }
}
```

### 4.3 AmentBuilder (New Core Type)

```rust
// In cargo-ros2-core/src/installer.rs

pub struct AmentBuilder {
    install_base: PathBuf,
    build_base: PathBuf,
    manifest_path: PathBuf,
    config: BuildConfig,
}

impl AmentBuilder {
    pub fn new(install_base: PathBuf, build_base: PathBuf) -> Result<Self>;

    /// Complete workflow: generate + build + install
    pub fn run(&self, cargo_args: Vec<OsString>) -> Result<()> {
        // Phase 1: Pre-build
        self.generate_bindings()?;

        // Phase 2: Build
        let exitcode = self.run_cargo_build(cargo_args)?;
        if exitcode != 0 {
            return Err(Error::BuildFailed);
        }

        // Phase 3: Post-build
        self.install()?;

        Ok(())
    }

    /// Phase 1: Generate ROS bindings
    fn generate_bindings(&self) -> Result<()> {
        let discovery = PackageDiscovery::new()?;
        let generator = BindingGenerator::new("target/ros2_bindings")?;
        let cache = BindingCache::load(&self.manifest_path.parent().unwrap())?;

        // Extract ROS dependencies from Cargo.toml
        let manifest = Manifest::from_path(&self.manifest_path)?;
        let ros_deps = extract_ros_dependencies(&manifest, &discovery)?;

        // Generate bindings for each ROS package
        for pkg_name in ros_deps {
            if !cache.is_fresh(&pkg_name)? {
                let pkg_info = discovery.find_package(&pkg_name)?;
                generator.generate(&pkg_info)?;
            }
        }

        // Update .cargo/config.toml patches
        let patcher = CargoPatcher::new(&self.manifest_path.parent().unwrap());
        patcher.add_patches(&ros_deps, Path::new("target/ros2_bindings"))?;

        Ok(())
    }

    /// Phase 2: Run cargo build or check
    fn run_cargo_build(&self, cargo_args: Vec<OsString>) -> Result<i32> {
        let manifest = Manifest::from_path(&self.manifest_path)?;

        // Determine build strategy (from cargo-ament-build logic)
        let is_pure_library = {
            let no_binaries = manifest.bin.is_empty();
            let no_exported_libraries = if let Some(crate_types) = manifest
                .lib
                .as_ref()
                .and_then(|lib| lib.crate_type.as_ref())
            {
                crate_types.as_slice() == [String::from("rlib")]
            } else {
                true
            };
            no_binaries && no_exported_libraries
        };

        let verb = if is_pure_library { "check" } else { "build" };

        let mut cmd = Command::new("cargo");
        cmd.arg(verb);
        cmd.args(&cargo_args);

        let status = cmd.status()?;
        Ok(status.code().unwrap_or(1))
    }

    /// Phase 3: Install to ament layout
    fn install(&self) -> Result<()> {
        let manifest = Manifest::from_path(&self.manifest_path)?;
        let package = manifest.package.as_ref()
            .ok_or(Error::NoPackageSection)?;
        let package_name = &package.name;
        let package_path = self.manifest_path.parent().unwrap();

        // 3.1: Create markers (from cargo-ament-build)
        create_package_marker(&self.install_base, "packages", package_name)?;
        create_package_marker(&self.install_base, "rust_packages", package_name)?;

        // 3.2: Install source code (from cargo-ament-build)
        install_package(
            &self.install_base,
            package_path,
            &self.manifest_path,
            package_name,
            &manifest,
        )?;

        // 3.3: Install binaries (from cargo-ament-build)
        install_binaries(
            &self.install_base,
            &self.build_base,
            package_name,
            &self.config.profile,
            self.config.arch.as_deref(),
            &self.config.features,
            &manifest.bin,
        )?;

        // 3.4: Install metadata files (from cargo-ament-build)
        install_files_from_metadata(
            &self.install_base,
            package_path,
            package_name,
            package.metadata.as_ref(),
        )?;

        Ok(())
    }
}
```

---

## 5. colcon-ros-cargo Integration

### 5.1 Current Implementation (calls cargo-ament-build)

```python
# colcon-ros-cargo/colcon_ros_cargo/task/ament_cargo/build.py

class AmentCargoBuildTask(CargoBuildTask):
    def _build_cmd(self, cargo_args):
        return [
            CARGO_EXECUTABLE, 'ament-build',  # ← calls cargo-ament-build
            '--install-base', args.install_base,
            '--',
            '--manifest-path', manifest_path,
            '--target-dir', args.build_base,
        ] + cargo_args
```

### 5.2 New Implementation (calls cargo-ros2)

```python
# Modified colcon-ros-cargo/colcon_ros_cargo/task/ament_cargo/build.py

class AmentCargoBuildTask(CargoBuildTask):
    def _prepare(self, env, additional_hooks):
        # Check for cargo-ros2 instead of cargo-ament-build
        ros2_check = 'cargo ros2 --version'.split()
        if subprocess.run(ros2_check, capture_output=True).returncode != 0:
            logger.error(
                '\n\nament_cargo package found but cargo-ros2 was not detected.'
                '\n\nPlease install it by running:'
                '\n $ cargo install cargo-ros2\n')
            return 1

        # ... rest of preparation (write .cargo/config.toml patches)

    def _build_cmd(self, cargo_args):
        return [
            CARGO_EXECUTABLE, 'ros2', 'ament-build',  # ← calls cargo-ros2
            '--install-base', args.install_base,
            '--',
            '--manifest-path', manifest_path,
            '--target-dir', args.build_base,
        ] + cargo_args

    # Installation is handled by cargo-ros2
    def _install_cmd(self, cargo_args):
        pass
```

### 5.3 Migration Strategy

1. **Phase 1**: Implement `cargo ros2 ament-build` with all cargo-ament-build features
2. **Phase 2**: Test thoroughly with existing ROS 2 Rust projects
3. **Phase 3**: Fork colcon-ros-cargo, modify to call cargo-ros2
4. **Phase 4**: Submit PR to colcon-ros-cargo (with deprecation path for cargo-ament-build)
5. **Phase 5**: Announce on ROS Discourse, provide migration guide

---

## 6. Code to Extract

### 6.1 From cargo-ament-build/src/lib.rs

**Functions to integrate into cargo-ros2-core:**

```rust
// ✅ EXTRACT: Marker creation
pub fn create_package_marker(
    install_base: impl AsRef<Path>,
    marker_dir: &str,
    package_name: &str,
) -> Result<()>

// ✅ EXTRACT: Source installation
pub fn install_package(
    install_base: impl AsRef<Path>,
    package_path: impl AsRef<Path>,
    manifest_path: impl AsRef<Path>,
    package_name: &str,
    manifest: &Manifest,
) -> Result<()>

// ✅ EXTRACT: Binary installation
pub fn install_binaries(
    install_base: impl AsRef<Path>,
    build_base: impl AsRef<Path>,
    package_name: &str,
    profile: &str,
    arch: Option<&str>,
    features: &HashSet<String>,
    binaries: &[Product],
) -> Result<()>

// ✅ EXTRACT: Metadata-based installation
pub fn install_files_from_metadata(
    install_base: impl AsRef<Path>,
    package_path: impl AsRef<Path>,
    package_name: &str,
    metadata: Option<&Value>,
) -> Result<()>

// ✅ EXTRACT: Utility function
fn copy(src: impl AsRef<Path>, dest_dir: impl AsRef<Path>) -> Result<()>
```

**Logic to integrate:**

```rust
// From main.rs: Pure library detection
let is_pure_library = {
    let no_binaries = manifest.bin.is_empty();
    let no_exported_libraries = if let Some(crate_types) = manifest
        .lib
        .as_ref()
        .and_then(|lib| lib.crate_type.as_ref())
    {
        crate_types.as_slice() == [String::from("rlib")]
    } else {
        true
    };
    no_binaries && no_exported_libraries
};
let verb = if is_pure_library { "check" } else { "build" };
```

### 6.2 Dependencies to Add

```toml
# In cargo-ros2-core/Cargo.toml

[dependencies]
# Existing
clap = "4.0"
serde = { version = "1.0", features = ["derive"] }
serde_json = "1.0"
toml = "0.8"
sha2 = "0.10"

# NEW: From cargo-ament-build
cargo-manifest = "0.17"  # For parsing Cargo.toml
anyhow = "1.0"           # For error handling
```

---

## 7. Migration Path

### 7.1 For Users

**Before** (current):
```bash
# Install 3 tools
pip install colcon-cargo colcon-ros-cargo
cargo install cargo-ament-build

# Use
colcon build
```

**After** (unified):
```bash
# Install 2 tools
pip install colcon-cargo colcon-ros-cargo-v2  # (our fork)
cargo install cargo-ros2

# Use (same command!)
colcon build
```

### 7.2 For Developers

**Transition period**:
1. cargo-ros2 supports both workflows:
   - `cargo ros2 build` (standalone, binding generation only)
   - `cargo ros2 ament-build` (full workflow, replaces cargo-ament-build)
2. colcon-ros-cargo detects which tool is available:
   - If cargo-ros2 → use `cargo ros2 ament-build`
   - Else if cargo-ament-build → use `cargo ament-build` (legacy)
   - Else → error

**End state** (6-12 months):
- cargo-ament-build marked as deprecated
- All users migrated to cargo-ros2
- colcon-ros-cargo upstreamed with cargo-ros2 support

---

## 8. Benefits of Unified Architecture

### 8.1 User Benefits

1. **Simpler installation**: One less tool to install
2. **Consistent experience**: One command for everything
3. **Better error messages**: Single tool knows full context
4. **Faster builds**: No subprocess overhead between tools

### 8.2 Developer Benefits

1. **Single codebase**: Easier to maintain and extend
2. **Unified configuration**: One config file, one cache system
3. **Better integration**: Full control over build pipeline
4. **Easier debugging**: No cross-tool issues

### 8.3 Ecosystem Benefits

1. **Cleaner dependency tree**: Fewer tools in the chain
2. **Easier onboarding**: Less to learn for new users
3. **Better documentation**: One tool, one comprehensive guide
4. **Faster iteration**: Changes don't require coordinating multiple repos

---

## 9. Implementation Timeline

### Phase 1.1-1.6 (Weeks 1-4): MVP with binding generation
- Focus on binding generation only
- No installation features yet

### Phase 2 (Weeks 5-8): Add ament installation
- **NEW**: Extract code from cargo-ament-build
- **NEW**: Implement `cargo ros2 ament-build` command
- **NEW**: Test with real ROS 2 projects

### Phase 3.1 (Weeks 9-12): colcon integration
- Fork colcon-ros-cargo
- Modify to call cargo-ros2
- Test with existing colcon workspaces
- Submit PR upstream

### Phase 4 (Weeks 13+): Deprecation and migration
- Announce on ROS Discourse
- Mark cargo-ament-build as deprecated
- Provide migration guide
- Support transition period

---

## 10. Success Criteria

### Technical

- ✅ All cargo-ament-build functionality preserved
- ✅ Passes all cargo-ament-build tests
- ✅ Works with existing ROS 2 Rust projects
- ✅ No performance regression

### Ecosystem

- ✅ PR accepted to colcon-ros-cargo
- ✅ Positive feedback from ros2-rust community
- ✅ cargo-ament-build maintainers approve deprecation
- ✅ Adoption by major ROS 2 Rust projects

---

**Status**: Design Phase - Ready for Implementation
**Next**: Begin Phase 1 MVP
**Last Updated**: 2025-01-30

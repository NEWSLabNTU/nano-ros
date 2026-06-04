# Architecture

## Overview

cargo-ros2 is split into two tools:

1. **`cargo-ros2-bindgen`** - Generates Rust bindings for a single ROS interface package
2. **`cargo-ros2`** - Main build tool orchestrating the full workflow

## Tool 1: cargo-ros2-bindgen

### Purpose
Generate Rust bindings for a single ROS interface package (messages, services, actions).

### Usage
```bash
cargo-ros2-bindgen --package std_msgs --output target/ros2_bindings/std_msgs
```

### Input
- ROS package name (discovers via ament_index)
- Or: Direct path to package share directory

### Output
```
target/ros2_bindings/std_msgs/
├── Cargo.toml               # Generated manifest with dependencies
├── src/
│   ├── lib.rs              # pub mod msg; pub mod srv; pub mod action;
│   ├── msg/
│   │   ├── mod.rs
│   │   ├── header.rs
│   │   └── ...
│   ├── srv/
│   └── action/
└── build.rs                # Links ROS C typesupport libs
```

### Dependencies Discovery
- Parse package.xml → extract `<depend>` tags
- Filter for interface packages
- Add to generated Cargo.toml as `dep = "*"`

### Implementation Strategy

**Phase 1 (MVP)**: Shell out to Python `rosidl_generator_rs`
```rust
fn generate_bindings(pkg: &str, output: &Path) -> Result<()> {
    // Invoke: python3 -m rosidl_generator_rs ...
    Command::new("python3")
        .arg("-m").arg("rosidl_generator_rs")
        .arg("--package-name").arg(pkg)
        .arg("--output-dir").arg(output)
        .status()?;
}
```

**Phase 2+**: Native Rust implementation
- Parse .msg/.srv/.action files natively
- Generate code with templates (no EmPy dependency)

---

## Tool 2: cargo-ros2

### Purpose
All-in-one build tool for ROS 2 Rust projects.

### Three-Phase Workflow

#### Phase 1: Pre-build (Binding Generation)
```rust
fn prebuild() -> Result<()> {
    let manifest = parse_cargo_toml()?;
    let ros_deps = discover_ros_deps(&manifest)?;  // Key function!

    for pkg in ros_deps {
        if !cache.is_fresh(pkg)? {
            cargo_ros2_bindgen(pkg, "target/ros2_bindings")?;
        }
    }

    write_cargo_config_patches(&ros_deps)?;
}
```

**Key Challenge**: Discover ROS interface packages from Cargo dependency tree.

#### Phase 2: Build
```rust
fn build(args: &[String]) -> Result<()> {
    let is_pure_lib = check_if_pure_library()?;
    let verb = if is_pure_lib { "check" } else { "build" };

    Command::new("cargo")
        .arg(verb)
        .args(args)
        .status()?;
}
```

#### Phase 3: Post-build (Ament Installation)
```rust
fn install(install_base: &Path) -> Result<()> {
    create_ament_markers(install_base)?;
    install_source(install_base)?;
    install_binaries(install_base)?;
    install_metadata_files(install_base)?;
}
```

---

## ROS Dependency Discovery

**Problem**: How to identify which Cargo dependencies are ROS interface packages?

### Strategy 1: Heuristic (Package Name)
```rust
fn is_ros_package(name: &str) -> bool {
    ament_index::get_package_share_directory(name).is_ok()
}
```

**Pros**: Simple
**Cons**: False positives (non-ROS crates with matching names)

### Strategy 2: Marker File
```toml
# In user's Cargo.toml
[package.metadata.ros2]
interface_packages = ["std_msgs", "sensor_msgs", "vision_msgs"]
```

**Pros**: Explicit, no ambiguity
**Cons**: User must manually list dependencies

### Strategy 3: Dependency Tree Analysis
```rust
fn discover_ros_deps(manifest: &Manifest) -> Result<Vec<String>> {
    let mut ros_deps = Vec::new();

    for (name, _) in &manifest.dependencies {
        // Try to resolve via ament_index
        if ament_index::get_package_share_directory(name).is_ok() {
            ros_deps.push(name.clone());

            // Recurse into transitive deps
            let pkg_xml = parse_package_xml(name)?;
            for dep in pkg_xml.depends {
                ros_deps.push(dep);
            }
        }
    }

    Ok(ros_deps)
}
```

**Pros**: Automatic, handles transitive deps
**Cons**: More complex

**MVP Choice**: Use Strategy 1 + 3 hybrid:
- Check Cargo dependencies against ament_index
- Recursively discover transitive ROS dependencies from package.xml

---

## Project Structure

```
cargo-ros2/
├── cargo-ros2-bindgen/       # Tool 1: Binding generator
│   └── src/
│       ├── main.rs           # CLI entry point
│       ├── discover.rs       # ament_index integration
│       ├── generate.rs       # Invoke rosidl_generator_rs (Phase 1)
│       └── cache.rs          # Checksum-based caching
│
├── cargo-ros2/               # Tool 2: Main build tool
│   └── src/
│       ├── main.rs           # CLI entry point
│       ├── prebuild.rs       # Phase 1: Generate bindings
│       ├── build.rs          # Phase 2: Cargo build
│       ├── install.rs        # Phase 3: Ament install
│       ├── discover.rs       # ROS dependency discovery
│       └── patch.rs          # .cargo/config.toml management
│
└── rosidl-runtime-rs/        # Runtime library (fork)
    └── src/
        ├── lib.rs
        ├── sequence.rs
        ├── string.rs
        └── traits.rs
```

---

## Data Flow

```
User runs: cargo ros2 ament-build --install-base install/my_pkg

    ↓

1. Discover ROS dependencies
   - Parse Cargo.toml
   - Check each dep against ament_index
   - Recursively get transitive deps from package.xml

    ↓

2. Generate bindings (for each missing/stale)
   cargo-ros2-bindgen --package std_msgs --output target/ros2_bindings/std_msgs
   cargo-ros2-bindgen --package sensor_msgs --output target/ros2_bindings/sensor_msgs

    ↓

3. Patch .cargo/config.toml
   [patch.crates-io]
   std_msgs = { path = "target/ros2_bindings/std_msgs" }
   sensor_msgs = { path = "target/ros2_bindings/sensor_msgs" }

    ↓

4. Build
   cargo build (or check for pure libs)

    ↓

5. Install
   - Binaries → install/my_pkg/lib/my_pkg/
   - Source → install/my_pkg/share/my_pkg/rust/
   - Markers → install/my_pkg/share/ament_index/
```

---

## Key Design Decisions

### 1. Two-Tool Split
**Why**: Separation of concerns
- `cargo-ros2-bindgen` is a low-level tool (can be used standalone)
- `cargo-ros2` orchestrates the full workflow

### 2. Project-Local Bindings
**Why**: Isolation, no global state
- All artifacts in `target/`
- `cargo clean` removes everything
- Different projects can use different ROS versions

### 3. Cargo Patches
**Why**: Standard Cargo mechanism
- No custom registry needed
- Works with existing tooling (cargo, clippy, rust-analyzer)
- Transparent to user

### 4. MVP Uses Python Generator
**Why**: Pragmatism
- rosidl_generator_rs already works
- Reusing it saves development time
- Can replace with native Rust later

### 5. Absorb cargo-ament-build
**Why**: Unified tool
- Simpler for users (one tool, not two)
- Better error messages (full context)
- Easier maintenance

---

## Dependencies

### cargo-ros2-bindgen
```toml
[dependencies]
anyhow = "1.0"
clap = "4.0"
serde_json = "1.0"  # For parsing ament_index data
```

### cargo-ros2
```toml
[dependencies]
anyhow = "1.0"
clap = "4.0"
cargo-manifest = "0.17"  # Parse Cargo.toml
toml = "0.8"             # Write .cargo/config.toml
serde = "1.0"
serde_json = "1.0"
sha2 = "0.10"            # Caching
```

### Runtime (rosidl-runtime-rs)
```toml
[dependencies]
serde = { version = "1", optional = true }
```

---

## External Dependencies

- **Python 3** (Phase 1 MVP only): For rosidl_generator_rs
- **ROS 2**: For ament_index, typesupport libraries
- **ament_index**: Package discovery

---

**Status**: Architecture design complete
**Next**: Implement MVP

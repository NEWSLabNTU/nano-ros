# Design Decisions

## 1. Tool Architecture

### Decision: Two-Tool Split

**Problem**: How to structure the codebase to balance simplicity with modularity?

**Alternatives Considered**:

**A. Single Monolithic Tool**
- All functionality in one binary: `cargo-ros2`
- Pros:
  - Simpler to install (one tool)
  - Easier to coordinate between phases
  - Single CLI to learn
- Cons:
  - Harder to test individual components
  - Can't use binding generator standalone
  - Larger binary, slower compile times
  - Tight coupling between concerns

**B. Two-Tool Split** ⭐ **CHOSEN**
- `cargo-ros2-bindgen`: Low-level binding generator
- `cargo-ros2`: High-level build orchestrator
- Pros:
  - Clear separation of concerns
  - Bindgen can be used standalone (useful for debugging)
  - Easier to test in isolation
  - Can parallelize development
  - Bindgen is reusable by other tools
- Cons:
  - Two binaries to install
  - Need to coordinate versions
  - More complex release process

**C. Three-Tool Split**
- Add separate `cargo-ros2-install` for ament installation
- Pros:
  - Even clearer separation
  - Could make installer optional
- Cons:
  - Too much fragmentation
  - Complicates user experience
  - Ament installation is core to ROS workflow

**Decision Rationale**:
Two tools strike the right balance. The binding generator is a distinct, reusable component that deserves its own tool. Installation is tightly coupled with the build process and doesn't benefit from being separate.

**Trade-offs Accepted**:
- Slightly more complex installation (two binaries)
- Need to maintain version compatibility between tools
- Worth it for modularity and testability

---

## 2. Binding Generation Strategy

### Decision: Shell Out to Python (MVP)

**Problem**: How to generate Rust bindings from ROS IDL files?

**Alternatives Considered**:

**A. Shell Out to `rosidl_generator_rs` (Python)** ⭐ **CHOSEN (MVP)**
- Invoke existing Python tool
- Pros:
  - Fastest path to MVP (reuse working code)
  - No need to understand EmPy templates
  - Proven to work with all ROS distros
  - Can focus on orchestration first
  - Lower risk
- Cons:
  - Python dependency (but ROS already requires it)
  - Process spawning overhead (~100ms per package)
  - Can't customize generation easily
  - Debugging is harder (crosses language boundary)

**B. Native Rust Implementation (from scratch)**
- Parse .msg/.srv/.action files in Rust
- Generate code with Rust templates
- Pros:
  - No Python dependency
  - Faster (no process spawning)
  - Full control over generation
  - Can optimize for our use case
  - Pure Rust solution
- Cons:
  - 4-6 weeks additional development time
  - Need to parse ROS IDL format (complex)
  - Need to replicate rosidl_generator_rs logic
  - Need to match output format exactly
  - Higher risk of bugs
  - Delays MVP

**C. FFI to C++ `rosidl_typesupport_introspection`**
- Use ROS C++ introspection to generate bindings
- Pros:
  - No Python dependency
  - Reuses official ROS code
- Cons:
  - C++ FFI is complex
  - Still process spawning or linking overhead
  - Introspection may not provide enough info
  - Unclear if faster than Python approach

**Decision Rationale**:
**MVP uses Python (Option A), Phase 4 switches to native Rust (Option B)**.

This is pragmatic: get working tool fast, then optimize. Python dependency is acceptable because ROS already requires it. The ~100ms overhead per package is mitigated by caching.

**Trade-offs Accepted**:
- Python dependency in MVP
- Some performance overhead
- Technical debt (need to replace later)
- Worth it to ship MVP 8+ weeks sooner

**Open Questions**:
- Can we optimize the Python invocation? (batch multiple packages?)
- Should we vendor rosidl_generator_rs to avoid version issues?

---

## 3. ROS Dependency Discovery

### Decision: Hybrid Approach

**Problem**: How to identify which Cargo dependencies are ROS interface packages?

**Alternatives Considered**:

**A. Heuristic: Check ament_index**
- For each Cargo dependency, check if it exists in ament_index
- Pros:
  - Fully automatic
  - No user configuration needed
  - Works with any naming
- Cons:
  - False positives (crate name collides with ROS package)
  - False negatives (renamed packages)
  - Slower (need to check many packages)

**B. Explicit Metadata in Cargo.toml**
```toml
[package.metadata.ros2]
interface_packages = ["std_msgs", "sensor_msgs"]
```
- User lists ROS packages explicitly
- Pros:
  - No ambiguity
  - Fast (no discovery needed)
  - User controls exactly what's generated
- Cons:
  - Manual work for user
  - Easy to forget to update
  - Duplicates dependency list
  - Goes against "zero configuration" goal

**C. Hybrid: ament_index + package.xml** ⭐ **CHOSEN**
- Check Cargo deps against ament_index (heuristic)
- Parse package.xml for transitive deps (explicit)
- Filter for interface packages (has msg/srv/action)
- Pros:
  - Automatic for user
  - Accurate (transitive deps from package.xml)
  - Handles ROS package dependencies correctly
  - Can detect false positives (check for msg/srv/action dirs)
- Cons:
  - More complex implementation
  - Slower than explicit metadata
  - Edge cases: workspace packages, dev packages

**Decision Rationale**:
Hybrid approach provides automation with accuracy. The heuristic catches direct dependencies, package.xml provides transitives, and filtering prevents false positives.

**Trade-offs Accepted**:
- More complex than either pure approach
- Slower than explicit metadata (but cached)
- Edge cases will need handling (workspace packages being developed)

**Open Questions**:
- How to handle packages in development (not yet in ament_index)?
- Should we support an optional override in Cargo.toml metadata?
- How to handle renamed packages (std_msgs vs std-msgs)?

---

## 4. Caching Strategy

### Decision: Project-Local Cache

**Problem**: How to avoid regenerating bindings unnecessarily?

**Alternatives Considered**:

**A. No Cache (Always Regenerate)**
- Regenerate all bindings on every build
- Pros:
  - Simple implementation
  - No stale cache issues
  - Always up-to-date
- Cons:
  - Slow (60+ seconds for typical project)
  - Wasteful (ROS packages rarely change)
  - Poor developer experience

**B. Global Cache (`~/.cache/cargo-ros2/`)**
- Share bindings across all projects
- Pros:
  - Faster for multi-project workflows
  - Disk space efficient
  - Similar to cargo's global cache
- Cons:
  - Not project-isolated (violates design principle)
  - Different projects may use different ROS distros
  - Cache conflicts between versions
  - Harder to clean (`cargo clean` doesn't work)
  - Needs global state management

**C. Project-Local Cache (`.ros2_bindgen_cache`)** ⭐ **CHOSEN**
- Cache metadata in project root
- Bindings in `target/ros2_bindings/`
- Pros:
  - Project isolation (key design principle)
  - `cargo clean` removes everything
  - No conflicts between projects
  - Survives git operations (in .gitignore)
  - Simple to reason about
- Cons:
  - Duplicates bindings across projects
  - More disk space (acceptable trade-off)
  - Cache lost when deleting project

**Decision Rationale**:
Project isolation is a core design principle. Global cache violates this and creates hard-to-debug issues. Disk space is cheap, developer time is not.

**Cache Invalidation**:
- SHA256 checksum of all .msg/.srv/.action files
- ROS_DISTRO change
- Package version change (from package.xml)
- cargo-ros2-bindgen version change

**Trade-offs Accepted**:
- More disk usage (~10MB per project for typical dependencies)
- Bindings regenerated when switching between projects
- Worth it for isolation and simplicity

**Open Questions**:
- Should we support optional global cache for power users?
- How to handle cache corruption?
- Should we cache compiled .rlib files too? (probably not - leave to cargo)

---

## 5. Cargo Integration Method

### Decision: Patch Mechanism

**Problem**: How to redirect Cargo dependency resolution to our generated bindings?

**Alternatives Considered**:

**A. Custom Cargo Registry**
- Create a registry index pointing to target/ros2_bindings/
- User specifies: `registry = "ros2_rust"`
- Pros:
  - Most "proper" Cargo solution
  - Clear intent in Cargo.toml
- Cons:
  - Complex implementation (need registry index format)
  - Requires maintaining index metadata
  - Checksum management overhead
  - Overkill for local redirects
  - Performance overhead (index parsing)

**B. Path Dependencies**
```toml
std_msgs = { path = "target/ros2_bindings/std_msgs" }
```
- Directly specify paths in Cargo.toml
- Pros:
  - Simple, obvious
- Cons:
  - User must manually edit Cargo.toml
  - Against "zero configuration" goal
  - Hard to manage many packages
  - Conflicts with publishing to crates.io

**C. [patch.crates-io] in .cargo/config.toml** ⭐ **CHOSEN**
```toml
[patch.crates-io]
std_msgs = { path = "target/ros2_bindings/std_msgs" }
```
- Cargo's built-in patching mechanism
- Pros:
  - Standard Cargo feature (designed for this)
  - Transparent to user's Cargo.toml
  - Works with existing tooling (clippy, rust-analyzer)
  - Automatic for user (we generate .cargo/config.toml)
  - Used by major projects (tokio, async-std dev workflow)
  - Fast (no overhead)
- Cons:
  - User can't easily see what's being patched (in .cargo/config.toml, not Cargo.toml)
  - Patches can be accidentally committed
  - Less familiar than regular dependencies

**Decision Rationale**:
Patches are exactly designed for this use case: temporary local redirects during development. It's the simplest standard mechanism that doesn't require custom infrastructure.

**Trade-offs Accepted**:
- .cargo/config.toml is "hidden" configuration
- Need to add comments/header to generated file
- Need to document that .cargo/ should be gitignored (or at least config.toml)

**Open Questions**:
- Should we warn if .cargo/config.toml is not in .gitignore?
- How to handle existing user patches in .cargo/config.toml?
- Should we support reading patches from Cargo.toml too? (in case user wants them tracked)

---

## 6. Installation Strategy

### Decision: Absorb cargo-ament-build

**Problem**: How to install built artifacts to ament layout?

**Alternatives Considered**:

**A. Shell Out to cargo-ament-build**
- Keep cargo-ament-build as separate tool
- Invoke it from cargo-ros2
- Pros:
  - Reuse existing, proven tool
  - Don't need to maintain installation code
  - Clear separation: we do bindings, they do installation
- Cons:
  - Two tools for user to install
  - Coordination overhead (version compatibility)
  - Extra process spawning
  - Can't provide unified error messages
  - Fragmented ecosystem

**B. Absorb cargo-ament-build Functionality** ⭐ **CHOSEN**
- Extract installation code into cargo-ros2
- Deprecate cargo-ament-build
- Pros:
  - One tool for users (simpler)
  - Unified error handling and logging
  - No version coordination issues
  - Can optimize full workflow
  - Cleaner ecosystem (fewer tools)
  - Better user experience
- Cons:
  - More code to maintain (~350 lines)
  - Need to keep parity with cargo-ament-build
  - Longer development time (but not much)
  - Need coordination with ros2-rust maintainers

**C. Separate Installer Tool**
- Create new cargo-ros2-install
- Pros:
  - Optional installation (for non-colcon users)
  - Modularity
- Cons:
  - Three tools (too fragmented)
  - Installation is core to ROS workflow
  - Adds complexity without clear benefit

**Decision Rationale**:
Absorbing installation creates a true all-in-one tool. The code is well-contained (~350 lines), easily extracted, and improves UX significantly.

**Trade-offs Accepted**:
- More code in cargo-ros2 (but modular)
- Need to maintain installation logic
- Need buy-in from ros2-rust community
- Worth it for unified user experience

**Migration Path**:
1. Implement in cargo-ros2 (Phase 2)
2. Test for parity with cargo-ament-build
3. Update colcon-ros-cargo to use cargo-ros2
4. Deprecate cargo-ament-build (with grace period)
5. Announce on ROS Discourse with migration guide

---

## 7. Transitive Dependency Handling

### Decision: Recursive Discovery via package.xml

**Problem**: How to discover ROS packages that are dependencies of dependencies?

**Example**: User depends on `vision_msgs`, which depends on `sensor_msgs`, which depends on `std_msgs`. How do we find `sensor_msgs` and `std_msgs`?

**Alternatives Considered**:

**A. User Lists Everything**
- User must specify all transitive deps in Cargo.toml
- Pros:
  - Explicit, no magic
  - Simple implementation
- Cons:
  - Error-prone (easy to forget deps)
  - Tedious for users
  - Against zero-configuration goal

**B. Parse Cargo.lock**
- Run `cargo metadata` to get resolved dependency tree
- Find ROS packages in the tree
- Pros:
  - Accurate (Cargo's resolution)
  - Handles complex scenarios
- Cons:
  - Chicken-and-egg: Cargo.lock doesn't exist until first build
  - Doesn't help with circular dependency problem
  - May include dev-dependencies unnecessarily

**C. Recursive package.xml Traversal** ⭐ **CHOSEN**
- Parse package.xml for each discovered ROS package
- Extract `<depend>`, `<build_depend>`, `<exec_depend>`
- Recursively discover deps of deps
- Filter for interface packages
- Pros:
  - Matches ROS dependency model
  - Works before Cargo resolution
  - Solves circular dependency problem
  - Accurate for ROS ecosystem
- Cons:
  - Need XML parsing
  - Slower (filesystem ops)
  - Must handle cycles in dependency graph

**Decision Rationale**:
ROS package dependencies are explicitly declared in package.xml. This is the authoritative source for ROS deps, and using it ensures we discover everything needed.

**Trade-offs Accepted**:
- Need XML parsing (add dependency: quick-xml)
- Filesystem overhead (mitigated by caching)
- Must detect and handle cycles

**Open Questions**:
- Should we cache the transitive dep graph?
- How to handle build vs exec dependencies? (probably generate both)
- What about test dependencies? (probably skip for MVP)

---

## 8. Comparison with Alternatives

### vs. ros2_rust (Official Bindings)

**Their Approach**:
- Workspace-required, colcon-driven build
- Three-stage: build ros2_rust → build interfaces → build packages
- Bindings in install/*/share/*/rust/
- Patches point to install directories

**Our Approach**:
- Project-local, Cargo-native
- Generate bindings to target/ before Cargo runs
- Works with system-installed ROS packages
- No workspace requirement

**Why Different?**
- ros2_rust optimizes for colcon ecosystem integration
- We optimize for Cargo ecosystem integration
- Their circular dependency still exists (patches point to future locations)
- We break it (generate before Cargo resolution)

**When to use ros2_rust**: When working in large colcon workspace with many ROS packages
**When to use cargo-ros2**: When working in Cargo-centric workflow, or with system ROS packages

---

### vs. r2r

**Their Approach**:
- Generate bindings in build.rs
- All generation happens during Cargo build
- No dependency on rosidl_generator_rs
- Uses C++ introspection

**Our Approach**:
- Generate bindings before Cargo build
- Cache bindings between builds
- Reuse rosidl_generator_rs (MVP)

**Why Different?**:
- build.rs generation happens every `cargo build` (slower)
- build.rs can't easily cache across builds
- build.rs runs after dependency resolution (can't solve circular dep cleanly)

**Trade-off**:
- r2r is simpler (one tool, standard build.rs)
- We're faster (caching) and more flexible (pre-build generation)

---

### vs. cargo-ament-build

**Their Approach**:
- Post-build only (installation)
- Assumes bindings already exist
- Focused, single-purpose tool

**Our Approach**:
- Full workflow (generate → build → install)
- Absorb their installation functionality
- All-in-one solution

**Why Different?**:
- They solved one problem (installation)
- We solve end-to-end workflow
- Better UX with unified tool

---

## 9. Open Design Questions

Questions to resolve during implementation:

### Workspace vs Single Package
- How to detect if we're in a Cargo workspace?
- Should we generate bindings at workspace root or per-package?
- How to share bindings across workspace members?

**Leaning toward**: Generate at workspace root if detected, else project root

### Custom Message Packages in Development
- How to handle local interface packages not yet in ament_index?
- Should we support path-based discovery?

**Leaning toward**: Add optional `--local-package <path>` flag

### ROS Distro Conflicts
- What if AMENT_PREFIX_PATH has multiple distros?
- Should we error, warn, or auto-detect?

**Leaning toward**: Detect ROS_DISTRO env var, warn on conflicts

### Generated Code Customization
- Should we support customizing generated code? (e.g., add derives, change visibility)
- How to make it extensible?

**Leaning toward**: Not in MVP, consider plugin system in Phase 4

### Performance Targets
- What's acceptable for cold build? 60s? 120s?
- What's acceptable for hot build (cache hit)? 5s? 10s?

**Leaning toward**: <60s cold, <5s hot (matches cargo-ament-build)

---

## 10. Future Evolution

### Phase 4: Native Rust Generator
When we replace Python generation:
- Will we match rosidl_generator_rs output exactly?
- Or take opportunity to improve generated code?
- How to handle migration? (both generators supported for a period?)

**Leaning toward**: Exact match initially, improvements later with feature flag

### Cross-Compilation
Not in MVP, but considerations:
- Need cross-compiled typesupport libraries
- May need separate AMENT_PREFIX_PATH for target
- Bindgen would need to know target triple

### IDE Integration
rust-analyzer needs to see generated code:
- Should we generate rust-project.json?
- Or rely on .cargo/config.toml patches being enough?

**Leaning toward**: Patches should be sufficient, test and document

---

**Status**: Design decisions documented
**Last Updated**: 2025-01-30

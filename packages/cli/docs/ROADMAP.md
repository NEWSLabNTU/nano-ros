# cargo-ros2: Project Roadmap

> **Note**: This is the main roadmap index. For detailed phase information, see the linked documents below.

## Quick Navigation

- [Progress Summary](#progress-summary)
- [Current Status](#current-status)
- [Phase Details](#phases)
- [Additional Resources](#additional-resources)

---

## Progress Summary

**Overall Progress**: 22 of 30 subphases complete (73%) + Phase 1 & Phase 4 In Progress! 🚀

| Phase                                 | Status           | Progress             | Details |
|---------------------------------------|------------------|----------------------|---------|
| Phase 0: Project Preparation          | ✅ Complete      | 3/3 subphases        | [View](ROADMAP/phase-0-preparation.md) |
| Phase 1: Native Rust IDL Generator    | 🔄 In Progress   | 6/7 subphases        | [View](ROADMAP/phase-1-idl-generator.md) |
| Phase 2: cargo-ros2 Tools             | ✅ Complete      | 2/2 subphases        | [View](ROADMAP/phase-2-tools.md) |
| Phase 3: Production Features          | ✅ Complete      | 4/4 subphases        | [View](ROADMAP/phase-3-production.md) |
| Phase 4: colcon Integration & Release | 🔄 In Progress   | 3/6 subphases        | [View](ROADMAP/phase-4-integration.md) |
| Phase 5: OMG IDL 4.2 Support          | ✅ Complete      | 4/4 subphases        | [View](ROADMAP/phase-5-idl-support.md) |

**Latest Achievement**: Phase 5 complete! 🎉 Full OMG IDL 4.2 support with lexer, parser, code generation, constant modules, @default annotations, and enums. Fixed constant module parsing order and RMW type path resolution. All 194 tests passing (100%)!

---

## Current Status

**Phase**: Phase 1 & Phase 4 In Progress (22/30 subphases complete - 73%) 🚀

### Completed ✅

- ✅ **Phase 0** Complete (all 3 subphases) - Project setup, tooling, dependencies
- ✅ **Phase 1** Subphases 1.1-1.6 Complete - Native Rust IDL Generator (parser, codegen, FFI bindings)
- ✅ **Phase 2** Complete (all 2 subphases) - cargo-ros2 Tools (bindgen CLI, core workflow)
- ✅ **Phase 3** Complete (all 4 subphases) - Production Features (services, ament, performance, docs)
- ✅ **Phase 4.1** Complete - colcon-ros-cargo Integration (rewrote to use cargo-ros2 exclusively)
- ✅ **Phase 4.1.1** Complete - config.toml Management Refactoring (centralized in cargo-ros2, no conflicts)
- ✅ **Phase 4.1.2** Complete - Code Generation Bug Fixes (Clone trait bounds, snake_case module paths)
- ✅ **Phase 5** Complete (all 4 subphases) - OMG IDL 4.2 Support (lexer, parser, codegen, integration - 100% tests)

### In Progress 🔄

- 🔧 **Phase 1, Subphase 1.7** - Code Generation Fixes (remaining issues)
  - Path resolution fix completed (2025-11-04)
  - Module path format updated to pkg::ffi::msg::module::Type (2025-11-15)
  - Discovered 3 remaining code generation issues during complex_workspace testing:
    1. Missing cross-package dependencies in generated Cargo.toml
    2. Missing module imports in generated code
    3. Trait definition stubs don't match actual rosidl_runtime_rs
  - Fix locations identified in cargo-ros2-bindgen and rosidl-codegen templates
  - [See detailed work items](ROADMAP/phase-1-idl-generator.md#subphase-17-code-generation-fixes-1-week)

### Clarified ✅

- ✅ **Phase 4.1.3** - Workspace Interface Package Discovery
  - **Initial concern was INCORRECT**: We thought discovering from install/ was broken
  - **Reality**: Current implementation is CORRECT and aligns with colcon's design!
  - **Key insight**: Colcon builds packages in topological dependency order, so dependencies are ALWAYS in install/ before dependents build
  - Only needs documentation, no code changes required
  - [See detailed analysis](ROADMAP/phase-4-integration.md#subphase-413-workspace-interface-package-discovery-1-week)

### Next Tasks 📋

1. **Complete Subphase 1.7** (Code Generation Fixes) - blocking for complex_workspace
2. **Document Subphase 4.1.3** (Add comments explaining colcon ordering) - 1 day
3. **Implement Subphase 4.1.4** (Transitive Dependency Discovery) - future enhancement
4. **Phase 4, Subphase 4.2** (Multi-Distro Support) - testing on Humble, Iron, Jazzy

**Date**: 2025-11-15

---

## Phases

### Phase 0: Project Preparation ✅ [Complete]

**Goal**: Set up project structure, tooling, and development infrastructure.

**Duration**: 1 week | **Status**: ✅ Complete (3/3 subphases)

**Subphases**:
- ✅ 0.1: Workspace Setup (Cargo workspace, profiles, justfile)
- ✅ 0.2: Documentation Setup (README, CONTRIBUTING, templates)
- ✅ 0.3: Dependencies & Tooling (clap, eyre, cargo-nextest)

**[View Full Phase Details →](ROADMAP/phase-0-preparation.md)**

---

### Phase 1: Native Rust IDL Generator 🔄 [In Progress]

**Goal**: Implement pure Rust parser and code generator for ROS IDL files (.msg, .srv, .action).

**Duration**: 5 weeks | **Status**: 🔄 In Progress (6/7 subphases)

**Subphases**:
- ✅ 1.1: IDL Parser - Messages (lexer, parser, AST)
- ✅ 1.2: Code Generator - Messages (templates, type mapping, Cargo.toml generation)
- ✅ 1.3: Services & Actions Support (full .srv and .action support)
- ✅ 1.4: Parity Testing (comparison with rosidl_generator_rs)
- ✅ 1.5: Parser Enhancements (default values, negative constants)
- ✅ 1.6: FFI Bindings & Runtime Traits (complete C interop, all traits)
- 🔧 1.7: Code Generation Fixes (cross-package deps, imports, trait stubs) - **IN PROGRESS**

**Key Achievement**: Pure Rust implementation with no Python dependency, 100% message parsing success rate, complete FFI bindings.

**[View Full Phase Details →](ROADMAP/phase-1-idl-generator.md)**

---

### Phase 2: cargo-ros2 Tools ✅ [Complete]

**Goal**: Build cargo-ros2-bindgen and cargo-ros2 tools using native generator.

**Duration**: 4 weeks | **Status**: ✅ Complete (2/2 subphases)

**Subphases**:
- ✅ 2.1: cargo-ros2-bindgen (ament index integration, CLI, 13 tests)
- ✅ 2.2: cargo-ros2 Core (cache system, config patcher, workflow, 26 tests)

**Key Achievement**: Complete CLI tools with intelligent caching, 181 tests passing, production-ready.

**[View Full Phase Details →](ROADMAP/phase-2-tools.md)**

---

### Phase 3: Production Features ✅ [Complete]

**Goal**: Add services, actions, ament installation, performance optimizations.

**Duration**: 5 weeks | **Status**: ✅ Complete (4/4 subphases)

**Subphases**:
- ✅ 3.1: Services & Actions Integration (already complete from Phase 1)
- ✅ 3.2: Ament Installation (ament_index markers, source/binary install)
- ✅ 3.3: Performance & CLI Polish (parallel generation, progress indicators, cache commands)
- ✅ 3.4: Testing & Documentation (190 tests, comprehensive docs)

**Key Achievement**: Production-ready with parallel builds, beautiful CLI, comprehensive testing.

**[View Full Phase Details →](ROADMAP/phase-3-production.md)**

---

### Phase 4: colcon Integration & Release 🔄 [In Progress]

**Goal**: Seamless colcon integration and public release.

**Duration**: 4 weeks | **Status**: 🔄 In Progress (3/6 subphases)

**Subphases**:
- ✅ 4.1: colcon-ros-cargo Integration (rewrote to use cargo-ros2 exclusively)
- ✅ 4.1.1: config.toml Management Refactoring (centralized, no race conditions)
- ✅ 4.1.2: Code Generation Bug Fixes (Clone bounds, snake_case modules)
- ✅ 4.1.3: Workspace Interface Package Discovery (clarified - current design is correct!)
- 📋 4.1.4: Transitive Dependency Discovery (future enhancement)
- 📋 4.2: Multi-Distro Support (Humble, Iron, Jazzy testing)
- 📋 4.3: Release Preparation (security audit, crates.io publish, v0.1.0)

**Key Achievement**: Eliminated circular dependencies, centralized config.toml management, clarified workspace discovery design.

**[View Full Phase Details →](ROADMAP/phase-4-integration.md)**

---

### Phase 5: OMG IDL 4.2 Support 🔄 [In Progress]

**Goal**: Add native support for `.idl` files to enable advanced ROS 2 features and DDS interoperability.

**Duration**: 6 weeks | **Status**: 🔄 In Progress (3/4 subphases)

**Subphases**:
- ✅ 5.1: IDL Lexer and Parser (OMG IDL 4.2 subset, module hierarchy, annotations)
- ✅ 5.2: IDL Code Generation (constant modules, @default values, wide strings, enums)
- ✅ 5.3: Integration and Testing (cargo-ros2-bindgen integration, real-world validation)
- 🔧 5.4: Documentation and Polish (usage guide, examples, performance testing) - **IN PROGRESS**

**Motivation**:
- `.idl` is the primary ROS 2 format (`.msg/.srv/.action` are legacy, converted to IDL internally)
- Supports features unavailable in `.msg`: `@key` annotations, `@default` values, wide strings
- Required for full ROS 2 compatibility and DDS interoperability
- Real-world usage: packages like `rclrs_example_msgs` use `.idl` files directly

**Key Achievement**: Complete IDL lexer/parser with 189/194 tests passing (97.4%)! Module path format updated to pkg::ffi::msg::module::Type for correct FFI hierarchy.

**[View Full Phase Details →](ROADMAP/phase-5-idl-support.md)**

---

## Additional Resources

### Planning & Strategy
- **[Milestones & Success Criteria](ROADMAP/milestones.md)** - M0-M4 milestones, technical/quality/community goals
- **[Testing Strategy](ROADMAP/testing-strategy.md)** - Unit, integration, end-to-end, regression testing approach
- **[Timeline & Schedule](ROADMAP/timeline.md)** - Phase durations, cumulative timeline, 19-week total estimate

### Design Documentation
- **[DESIGN.md](DESIGN.md)** - Technical design and architecture
- **[CLAUDE.md](../CLAUDE.md)** - Project instructions and overview

### Progress Tracking
- **[Current Status](#current-status)** - What's done, what's in progress, what's next
- **[Phase Details](#phases)** - Links to detailed phase documentation

---

## How to Use This Roadmap

1. **Quick Overview**: Check the [Progress Summary](#progress-summary) table for phase status
2. **Current Work**: See [Current Status](#current-status) for active tasks
3. **Detailed Planning**: Click phase links to see subphase details, work items, and acceptance criteria
4. **Context**: Read [Additional Resources](#additional-resources) for strategy and design docs

---

**Last Updated**: 2025-11-15

**Project Status**: Active development, 70% complete (21/30 subphases), Phase 1, Phase 4 & Phase 5 in progress, IDL support nearly complete!

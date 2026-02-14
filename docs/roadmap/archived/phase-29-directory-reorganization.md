# Phase 29: Directory Reorganization (`crates/` → `packages/`)

## Summary

Reorganize the repository directory structure to improve navigability:
1. Rename `crates/` → `packages/` with categorical subdirectories
2. Move `packages/codegen/` → `packages/codegen/`
3. Move `packages/reference/` → `packages/reference/`
4. Keep `examples/` exclusively for ROS API usage examples

## Motivation

The `crates/` directory contains 19 packages spanning core libs, transport, BSPs, drivers, tests, and interfaces — all flat with no categorization. Grouping by purpose makes the repository easier to navigate and understand.

## Target Structure

```
packages/
├── core/                          # The nano-ros library stack
│   ├── nano-ros/
│   ├── nano-ros-core/
│   ├── nano-ros-serdes/
│   ├── nano-ros-macros/
│   ├── nano-ros-params/
│   ├── nano-ros-transport/
│   ├── nano-ros-node/
│   └── nano-ros-c/
├── transport/                     # Zenoh transport backend
│   ├── nano-ros-transport-zenoh/
│   └── nano-ros-transport-zenoh-sys/
├── bsp/                           # Board Support Packages
│   ├── nano-ros-bsp-qemu/
│   ├── nano-ros-bsp-esp32/
│   ├── nano-ros-bsp-esp32-qemu/
│   ├── nano-ros-bsp-stm32f4/
│   └── nano-ros-bsp-zephyr/
├── drivers/                       # Hardware drivers
│   ├── lan9118-smoltcp/
│   └── openeth-smoltcp/
├── interfaces/                    # Generated ROS 2 types
│   └── rcl-interfaces/
├── testing/                       # Test infrastructure
│   └── nano-ros-tests/
├── reference/                     # Low-level platform reference impls
│   ├── qemu-smoltcp-bridge/
│   ├── qemu-lan9118/
│   ├── stm32f4-embassy/
│   ├── stm32f4-polling/
│   ├── stm32f4-rtic/
│   ├── stm32f4-smoltcp/
│   ├── embedded-cpp-listener/
│   └── embedded-cpp-talker/
└── codegen/                       # Message binding generator
    ├── packages/
    ├── interfaces/
    └── ...
```

## Work Items

### 29.1: Directory moves (`git mv`)
- [x] `git mv crates packages` (flat rename)
- [x] `git mv colcon-nano-ros packages/codegen`
- [x] Create subdirs and move packages into categories
- [x] Move `packages/reference/*` → `packages/reference/`

### 29.2: Root `Cargo.toml`
- [x] Update `members` paths
- [x] Update `exclude` paths
- [x] Update `workspace.dependencies` paths
- [x] Update `patch.crates-io` paths

### 29.3: Internal `Cargo.toml` cross-references
- [x] Fix path deps that cross category boundaries

### 29.4: Example `.cargo/config.toml` files
- [x] Update `crates/` → `packages/{cat}/` in patch paths

### 29.5: justfile
- [x] Update all path references

### 29.6: cmake files
- [x] `FindNanoRos.cmake`, `FindNanoRosCodegen.cmake`, `nano_ros_generate_interfaces.cmake`

### 29.7: Shell scripts
- [x] Build scripts, test scripts, debug scripts

### 29.8: Documentation
- [x] `CLAUDE.md`, `README.md`, roadmap docs, guides, etc.

### 29.9: Memory files
- [x] Update MEMORY.md file locations

## Verification

```bash
just quality          # format + clippy + nextest + miri + QEMU examples
```

---
id: 2087
title: "A standalone CMake leaf's declared `ros_edition` reached nothing — codegen and the runtime built humble with no error"
status: resolved
type: bug
area: [build, cmake, examples]
severity: high
found: 2026-10-26
resolved: 2026-10-26
related: [issue-2045, rfc-0056, rfc-0031, phase-452]
---

# A standalone CMake leaf ignored its declared ROS edition

**Home:** [phase-452](../../roadmap/phase-452-copy-out-front-door.md) W7. Found
while fixing [#2045](2045-example-manifests-hand-set-the-ros-edition-feature.md),
whose cargo-road twin was LOUD; this one was silent.

## Problem

RFC-0056 puts the edition in `system.toml` (`[image.<id>]` > `[image_defaults]`
> `[system] ros_edition`). On the CMake road every generator and the runtime
image crate resolve it through `_nros_resolve_ros_edition`, which reads
`NROS_ROS_EDITION`, and only a WORKSPACE configure ever set that variable
(`NanoRosWorkspace.cmake`). `nros ws leaf-system`, the one reader a standalone
leaf's configure asks, printed the image's board, RMW, domain and locator, and
no edition.

So a copied-out `examples/linux/c/talker` with `ros_edition = "jazzy"` under
`[system]` configured, built and ran with humble type hashes:

```
cmake -S . -B build -Dnano_ros_ROOT=<root>
grep -rho '"ros_edition": "[a-z]*"' build      # → "humble" (2 codegen arg files)
```

A wire mismatch against a jazzy peer, found only at runtime.

Two more defects stood behind it, each found by measuring the fix:

- The root `CMakeLists.txt` cached `NROS_ROS_EDITION` with the default
  (`humble`) before anything read the leaf, so on the `add_subdirectory` road a
  default was indistinguishable from a user's `-D`.
- On that road the leaf was read AFTER `packages/api/nros-c` and `nros-cpp`
  were added, and both generate their image crate at configure time. With only
  the read fixed, a jazzy `rv-virt-threadx/rust/talker` built its app crate
  with `ros-jazzy` and its C runtime with `ros-humble`, in one image, without
  an error.

## Resolution (2026-10-26)

- `LeafSystem` carries `ros_edition` (same overlay as `rmw`) and `nros ws
  leaf-system` prints `NROS_LEAF_ROS_EDITION`.
- `nano_ros_read_leaf_system` refuses a `-DNROS_ROS_EDITION` that disagrees with
  the declaration (the domain rule, issue 1946), and `nros_apply_leaf_identity`
  sets `NROS_ROS_EDITION` from it on both roads.
- The root cache default is EMPTY ("the leaf's `system.toml`, else humble");
  the resolver supplies the default.
- The root reads the leaf before the core libraries.
- `nros_threadx_rv64_rust_app` passes `ros-<edition>`, which the six
  `rv-virt-threadx/rust` leaves now define as forwards (#2045).

**Verified** on copies outside the checkout:

| leaf | declared | codegen | runtime image crate | app crate | build |
| --- | --- | --- | --- | --- | --- |
| `linux/c/talker` | — | humble | humble | — | — |
| `linux/c/talker` | jazzy | jazzy | jazzy | — | links |
| `linux/c/talker` | jazzy, `-DNROS_ROS_EDITION=humble` | configure refused, naming both | | | |
| `rv-virt-threadx/rust/talker` (zenoh) | — | humble | `ros-humble` | `ros-humble` | links |
| `rv-virt-threadx/rust/talker` (zenoh) | jazzy | jazzy | `ros-jazzy` | `ros-jazzy` | links |

No tracked CMake leaf declares an edition and no lane passes
`-DNROS_ROS_EDITION`, so every in-tree build resolves what it did before.

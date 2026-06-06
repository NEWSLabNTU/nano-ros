# `multi-node-workspace-cpp` — canonical 3-role C++ template

C++ parallel to
[`multi-node-workspace/`](../multi-node-workspace/) (Rust). Demonstrates
the §11 three-role shape with `nano_ros_workspace(...)` (Phase 219.I)
at the root and `nano_ros_workspace_pkg_guard()` per subdir.

```
multi-node-workspace-cpp/
├── CMakeLists.txt                # nano_ros_workspace(SYSTEM demo_bringup SUBDIRS ...)
└── src/
    ├── talker_pkg/               # Node pkg (C++) — publishes /chatter
    │   ├── package.xml
    │   ├── CMakeLists.txt        # nano_ros_node_register(NAME talker CLASS talker_pkg::Talker ...)
    │   └── src/{Talker.hpp,Talker.cpp}
    ├── listener_pkg/             # Node pkg (C++) — subscribes /chatter
    │   ├── package.xml
    │   ├── CMakeLists.txt        # nano_ros_node_register(NAME listener CLASS listener_pkg::Listener ...)
    │   └── src/{Listener.hpp,Listener.cpp}
    ├── demo_bringup/             # Bringup pkg (language-agnostic)
    │   ├── package.xml
    │   ├── system.toml
    │   └── launch/system.launch.xml
    └── robot_entry/              # Entry pkg (C++) — NROS_MAIN + nano_ros_entry(LAUNCH ...)
```

## Coverage vs the Rust template

| Role | Rust template | This (C++) template |
|---|---|---|
| Node pkg | `nros::node!(Talker)` rlib | `nano_ros_node_register()` + `NROS_NODE_REGISTER(...)` static lib |
| Bringup pkg | identical (language-agnostic) | identical (language-agnostic) |
| Entry pkg | `nros::main!(launch = "demo_bringup:system.launch.xml")` | `NROS_MAIN(...)` + `nano_ros_entry(LAUNCH ...)` |

The Entry pkg composes the C++ Node pkg static libraries into one native
binary. The Bringup's `system.toml` + `launch/system.launch.xml` are also
consumable by `nros plan demo_bringup` from outside the tree.

## Build

```bash
# Workspace path (recommended):
cmake -S . -B build -DNANO_ROS_ROOT=/path/to/nano-ros
cmake --build build

# Or any single Node pkg standalone:
cmake -S src/talker_pkg -B build/talker_pkg \
      -DNANO_ROS_ROOT=/path/to/nano-ros
cmake --build build/talker_pkg
```

In-tree builds (this checkout's nano-ros) auto-walk for
`nros-sdk-index.toml`, so `-DNANO_ROS_ROOT=` is optional.

See [`docs/roadmap/archived/phase-219-cpp-entry-pkg.md`](../../../docs/roadmap/archived/phase-219-cpp-entry-pkg.md)
for the landing history.

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
    └── demo_bringup/             # Bringup pkg (language-agnostic)
        ├── package.xml
        ├── system.toml
        └── launch/system.launch.xml
```

## Coverage vs the Rust template

| Role | Rust template | This (C++) template |
|---|---|---|
| Node pkg | `nros::node!(Talker)` rlib | `nano_ros_node_register()` + `NROS_NODE_REGISTER(...)` static lib |
| Bringup pkg | identical (language-agnostic) | identical (language-agnostic) |
| Entry pkg | `nros::main!(launch = "demo_bringup:system.launch.xml")` | **deferred** — pending Phase 219.D (`nano_ros_entry(LAUNCH ...)` cmake arg) + 219.A-E (codegen verb + generated TU) |

The Entry pkg lands once 219.D + 219.A/B/E ship. Until then, this
template validates the **Node + Bringup** side of the C++ pure-workspace
path; the Bringup's `system.toml` + `launch/system.launch.xml` are still
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

## What lands when

| 219.X item | Status | Surface added |
|---|---|---|
| 219.H | ✅ landed | sibling Node pkgs sharing `<depend>std_msgs</depend>` co-exist (interface codegen idempotency) |
| 219.I | ✅ landed | workspace root + `nano_ros_workspace_pkg_guard()` |
| 219.M | ✅ landed | `nros new --component --lang cpp` scaffolds match this shape verbatim |
| 219.J | pending | drops `target_link_libraries(<pkg>_<node>_component PUBLIC std_msgs__nano_ros_cpp)` (auto-link from launch metadata) |
| 219.D | pending | adds Entry pkg `nano_ros_entry(LAUNCH "demo_bringup:system.launch.xml")` |
| 219.A/B/C/E | pending | emits the generated `main()` TU + `NROS_MAIN(...)` macro |

See [`docs/roadmap/phase-219-cpp-entry-pkg.md`](../../../docs/roadmap/phase-219-cpp-entry-pkg.md)
§4 for full landing order.

# Mixed-language workspace

nano-ros Node pkgs are linked through a C ABI register trampoline, so
one Entry pkg can host Node pkgs written in different languages. The
native reference shape is:

- C Node pkg for C code you want to keep in C.
- C++ Entry pkg for the boot harness and generated launch wiring.
- Optional C++ Node pkgs in the same workspace.
- One Bringup pkg with the normal ROS 2 launch XML.

The mixed workspace is in
[`examples/workspaces/mixed/`](https://github.com/NEWSLabNTU/nano-ros/tree/main/examples/workspaces/mixed).
For a pure-C Entry host, use
[`examples/workspaces/c/`](https://github.com/NEWSLabNTU/nano-ros/tree/main/examples/workspaces/c).

## Layout

```text
my_ws/
├── CMakeLists.txt
└── src/
    ├── c_talker_pkg/             # C Node pkg
    ├── cpp_listener_pkg/         # C++ Node pkg
    ├── demo_bringup/             # launch XML + system.toml
    └── native_entry/             # C++ Entry pkg
```

The root uses the same CMake workspace helper as the C++ track:

```cmake
include(<nano-ros>/cmake/NanoRosWorkspace.cmake)

nano_ros_workspace(
    BACKEND  zenoh
    PLATFORM posix
    SUBDIRS  src/c_talker_pkg
             src/cpp_listener_pkg
             src/native_entry)
```

## C Node pkg

A C Node pkg is declarative: it has no `main()`. It exports a package-
mangled register symbol via `NROS_NODE_REGISTER(register_fn)`.

```cmake
nros_find_interfaces(LANGUAGE C SKIP_INSTALL)

nano_ros_node_register(
    NAME     talker
    CLASS    c_talker_pkg::Talker
    LANGUAGE C
    SOURCES  src/Talker.c
    DEPLOY   native)
```

```c
#include <nros/node_pkg.h>

static nros_ret_t register_talker(nros_node_context_t* ctx) {
    /* declare node entities here */
    return NROS_RET_OK;
}

NROS_NODE_REGISTER(register_talker);
```

`nano_ros_node_register()` injects `NROS_PKG_NAME` and
`NROS_NODE_CLASS_NAME`, so the C object exports the same register,
presence, and class-name symbols as the C++ Node-pkg macro.

## Entry pkg

Use a C++ or Rust Entry pkg as the usual host for migration work. In the
C++ template:

```cmake
nano_ros_entry(
    NAME    native_entry
    SOURCES src/main.cpp
    BOARD   native
    LAUNCH  "demo_bringup:system.launch.xml"
    DEPLOY  native)
```

The launch file names both Node pkgs:

```xml
<launch>
  <node pkg="c_talker_pkg" exec="talker" name="talker"/>
  <node pkg="cpp_listener_pkg" exec="listener" name="listener"/>
</launch>
```

The generated Entry translation unit calls each package's
`__nros_component_<pkg>_register` symbol and the CMake sidecar links
the matching static libraries. The symbol keeps the legacy
`component` spelling for ABI compatibility; the user-facing package
role is still Node pkg.

For a pure-C workspace, the Entry pkg uses the same launch-driven shape
with `LANG c`:

```cmake
nano_ros_entry(
    NAME    native_entry
    SOURCES src/main.c
    BOARD   native
    LAUNCH  "demo_bringup:system.launch.xml"
    LANG    c
    DEPLOY  native)
```

## Scaffolding

```sh
# Current compatibility scaffold for Node pkgs:
nros new --component c_talker_pkg --lang c --use-case talker
nros new --component cpp_listener_pkg --lang cpp --use-case listener
nros new system demo_bringup --components c_talker_pkg,cpp_listener_pkg
```

For a complete working tree, copy the template instead of creating
each package separately.

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

A C Node pkg is a **typed component** (RFC-0043): no `main()`. It defines a
state struct + a `configure(node, executor, self)` function and exports the
C-ABI factory/configure seam via `NROS_C_COMPONENT(StateT, configure_fn)` — the
typed Entry creates the node and runs `configure` on the real executor.

```cmake
# Raw `/chatter` publisher carries the type name as a string → no generated C
# bindings needed (so no nros_find_interfaces for this pkg).
nano_ros_node_register(
    NAME     talker
    CLASS    c_talker_pkg::Talker
    LANGUAGE C
    TYPED
    SOURCES  src/Talker.c
    DEPLOY   native)
```

```c
#include <stdint.h>
#include <nros/component.h>

typedef struct {
    _Alignas(8) uint8_t pub[NROS_C_PUBLISHER_STORAGE_SIZE];
    int32_t count;
} c_talker_pkg_t;

static void on_tick(void* ctx) {
    c_talker_pkg_t* self = (c_talker_pkg_t*)ctx;
    uint8_t buf[8] = {0x00, 0x01, 0x00, 0x00};  /* CDR_LE header + LE int32 */
    buf[4] = (uint8_t)self->count;
    (void)nros_cpp_publish_raw(self->pub, buf, sizeof(buf));
    self->count++;
}

static nros_ret_t talker_configure(const nros_cpp_node_t* node, void* executor,
                                   c_talker_pkg_t* self) {
    self->count = 0;
    int32_t rc = nros_cpp_publisher_create(node, "/chatter",
        "std_msgs::msg::dds_::Int32_", "", nros_c_qos_default(), self->pub);
    if (rc != 0) return rc;
    size_t timer;
    return nros_cpp_timer_create(executor, /*period_ms=*/1000, on_tick, self, &timer);
}

NROS_C_COMPONENT(c_talker_pkg_t, talker_configure)
```

`nano_ros_node_register(... LANGUAGE C TYPED ...)` injects `NROS_PKG_NAME`, so
`NROS_C_COMPONENT` exports the `__nros_c_component_<pkg>_{create,configure}`
seam the typed Entry calls — interoperable with C++ `configure(Node&)` and Rust
`nros::node!` components in one launch graph.

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

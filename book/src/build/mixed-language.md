# Mixed-language workspace

Languages mix per package, not per file. A bringup's `[[component]]` row
names a package and a class whatever language implements it, and node
packages are linked through a C ABI register trampoline, so one image can
compose node packages written in C, C++ and Rust. The
[workspace shape](growing-your-workspace.md) does not change with the
language. The native reference shape is:

- C Node pkgs for C code you want to keep in C.
- C++ and Rust Node pkgs beside them, in the same workspace.
- One Bringup pkg with the normal ROS 2 launch XML and the `[image.*]` rows.

The mixed workspace is in
[`examples/workspaces/mixed/`](https://github.com/NEWSLabNTU/nano-ros/tree/main/examples/workspaces/mixed).
For a pure-C workspace, use
[`examples/workspaces/c/`](https://github.com/NEWSLabNTU/nano-ros/tree/main/examples/workspaces/c).

## Layout

```text
my_ws/
├── .colcon_workspace             # tracked marker; there is no root build file
└── src/
    ├── c_talker_pkg/             # C Node pkg
    ├── cpp_listener_pkg/         # C++ Node pkg
    ├── rust_heartbeat_pkg/       # Rust Node pkg
    └── demo_bringup/             # package.xml + system.toml (+ launch/)
```

Mixed is not a special case. The driver is chosen by the **board**, and cmake
wins whenever the package graph crosses languages — corrosion makes cargo
consumable from cmake, and nothing makes cmake consumable from cargo. So a mixed workspace builds exactly like the C++ one:

```console
$ cd my_mixed_ws              # e.g. a copy of examples/workspaces/mixed
$ source /opt/ros/humble/setup.bash   # the first build generates messages from ROS 2
$ nros build linux
$ nros run linux
```

The first build records the ROS 2 prefix in the workspace, so later shells
need not source ROS 2.

`nros build` writes a `CMakeLists.txt` — a `nano_ros_workspace(…)` call listing
the discovered packages, plus one `nano_ros_add_executable(…)` per image on
that coordinate — into `build/<coordinate>/`, configures it into
`build/<coordinate>/cmake/`, and builds. That generated file is the cmake root;
the workspace itself has no build file at all, and neither the
root nor the entry is yours to write.

## C Node pkg

A C Node pkg is a **typed component**: no `main()`. It defines a
state struct + a `configure(node, executor, self)` function and exports the
C-ABI factory/configure seam via `RCLC_C_COMPONENT(StateT, configure_fn)` — the
typed Entry creates the node and runs `configure` on the real executor. The
component is written against the **C API**: `configure` receives an
`rcl_node_t *` and an `rclc_executor_t *` over the entry's own node and
executor, so a C and a C++ component in one image share both.

```cmake
# std_msgs is declared in package.xml (`<depend>std_msgs</depend>`), which
# generates its C bindings for this package.
find_package(std_msgs REQUIRED)
nano_ros_auto_add_library(talker_lib STATIC src/Talker.c)

nros_components_register_node(talker_lib
    PLUGIN c_talker_pkg::Talker
    EXECUTABLE talker
    SHAPE configure
    TYPED)
```

```c
#include <stdint.h>
#include <rclc/rclc.h>
#include <std_msgs/msg/int32.h>

typedef struct {
    rcl_publisher_t pub;
    rcl_timer_t timer;
    int32_t count;
} c_talker_pkg_t;

static void on_tick(rcl_timer_t* timer, int64_t last_call_time) {
    c_talker_pkg_t* self = RCLC_CONTAINER_OF(timer, c_talker_pkg_t, timer);
    (void)last_call_time;
    std_msgs__msg__Int32 msg = {.data = self->count};
    (void)rcl_publish(&self->pub, &msg, NULL);
    self->count++;
}

static rcl_ret_t talker_configure(rcl_node_t* node, rclc_executor_t* executor,
                                  c_talker_pkg_t* self) {
    self->count = 0;
    self->pub = rcl_get_zero_initialized_publisher();
    rcl_ret_t rc = rclc_publisher_init_default(
        &self->pub, node, ROSIDL_GET_MSG_TYPE_SUPPORT(std_msgs, msg, Int32), "/chatter");
    if (rc != RCL_RET_OK) return rc;
    self->timer = rcl_get_zero_initialized_timer();
    rc = rclc_timer_init_default(&self->timer, NULL, 1000000000ULL, on_tick);
    if (rc != RCL_RET_OK) return rc;
    return rclc_executor_add_timer(executor, &self->timer);
}

RCLC_C_COMPONENT(c_talker_pkg_t, talker_configure)
```

`nano_ros_auto_add_library` injects `NROS_PKG_NAME` (language inferred from the sources), so
`RCLC_C_COMPONENT` exports the `__nros_c_component_<pkg>_{create,configure}`
seam the typed Entry calls — interoperable with C++ `configure(Node&)` and Rust
`nros::node!` components in one launch graph.

## The image

Nothing about the language mix reaches the image declaration. One row per
program, in `src/demo_bringup/system.toml`:

```toml
[image_defaults]
rmw = "zenoh"

[image.linux]
board = "linux"
```

The launch file (here an authored `launch/system.launch.xml`; without one, the
file generated from the `[[component]]` rows) names the Node pkgs, whatever
they are written in:

```xml
<launch>
  <node pkg="c_talker_pkg" exec="talker" name="talker"/>
  <node pkg="cpp_listener_pkg" exec="listener" name="listener"/>
</launch>
```

From that pair, `nros build` emits this into the generated root. `LANG` follows
the workspace's own sources; `BOARD` and `DEPLOY` come from the resolved image,
so a generated entry cannot disagree with the board it was generated for. There
is no `SOURCES`, because the verb generates the translation unit carrying
`main`:

```cmake
nano_ros_add_executable(linux_entry
    BOARD   native
    BRINGUP "${CMAKE_CURRENT_SOURCE_DIR}/../../src/demo_bringup"
    LAUNCH  default
    LANG    cpp
    TYPED
    DEPLOY  native)
```

The generated entry translation unit calls each package's
`__nros_component_<pkg>_register` symbol and the CMake sidecar links the
matching static libraries. The symbol keeps the legacy `component` spelling for
ABI compatibility; the user-facing package role is still Node pkg.

Note there is no `MODEL` here. The resolved system is a build artifact,
resolved from the launch file under `build/`. Naming it in a build file is a
deprecated expert override, and committing one is refused.

## Per-language notes

**Rust.** A Rust node package is a library crate ending in
`nros::node!(YourNode)`. Its message crates and the settings cargo needs (the
board's target triple, link flags, environment) are generated under `build/`,
never beside your packages, and `nros build` prepares them before it hands
off. In a mixed workspace the Rust packages are built through CMake (by
corrosion), so their compiler errors are still cargo's.

**C.** A C node package is the `configure` component shape above, built by
the same per-package CMake as C++. Its message bindings are generated during
the CMake configure.

**The RMW is spelled once**, whatever the languages: `[system] rmw` in the
bringup's `system.toml`, or `rmw =` on one `[image.*]` block when one image
differs. The features each package needs follow from it, so switching backend
is a one-word edit: [Switching RMW in Config](../communicate/switching-rmw.md).

## Scaffolding

Inside a workspace, `nros new` adds a node package in the language you name,
and `--to` adds its component row to a bringup:

```sh
nros new c_talker_pkg --lang c --template talker --to demo_bringup
nros new cpp_listener_pkg --lang cpp --template listener --to demo_bringup
nros new rust_heartbeat_pkg --lang rust --to demo_bringup
```

To start from a complete mixed tree, copy
[`examples/workspaces/mixed/`](https://github.com/NEWSLabNTU/nano-ros/tree/main/examples/workspaces/mixed)
out instead. It runs a C talker, a C++ listener, a Rust heartbeat, and C and
C++ service and action pairs, composed into one program per image.

## Design background

- [RFC-0121: workspace-first project layout](https://github.com/NEWSLabNTU/nano-ros/blob/main/docs/design/0121-workspace-first-project-layout.md)

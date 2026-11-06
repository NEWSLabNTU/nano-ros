# Mix languages

<!-- type: how-to · level: 2 -->

**Needs first:** [Organize a larger workspace](organize-a-larger-workspace.md)

Mix languages when one image should run node packages written in C, C++ and
Rust: C code you want to keep in C, beside new C++ or Rust nodes.

Languages mix per package, not per file. A bringup's `[[component]]` row names
a package and a class whatever language implements it, and each node package
registers itself through a C ABI the generated entry calls. The workspace shape
does not change with the language, and neither does `system.toml`.

## Before you start

- A workspace with a bringup, as in
  [Organize a larger workspace](organize-a-larger-workspace.md).
- ROS 2 sourced for the first build. It generates the message bindings for
  every language.

## Steps

1. Add a node package per language. From the workspace root, `--lang` picks
   the language and `--to` adds the package's row to the bringup:

   ```bash
   nros new c_talker_pkg --lang c --template talker --to host_node_bringup
   nros new listener_pkg --lang cpp --template listener --to host_node_bringup
   ```

   ```text
   nros new: added the node package src/c_talker_pkg/ (talker, c) to …/host_ws
     declared it in …/host_ws/src/host_node_bringup: a [[component]] row and an <exec_depend>
   ```

2. Write the C node as a typed component. It has no `main()`: it defines a
   state struct and a `configure` function, and exports them with
   `RCLC_C_COMPONENT`. `configure` receives the entry's own node and
   executor, so a C and a C++ component in one image share both:

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

   Its `CMakeLists.txt` registers the component the same way a C++ package
   does, with `TYPED`:

   ```cmake
   find_package(std_msgs REQUIRED)   # declared as <depend>std_msgs</depend> in package.xml
   nano_ros_auto_add_library(talker_lib STATIC src/Talker.c)
   nros_components_register_node(talker_lib
       PLUGIN c_talker_pkg::Talker
       EXECUTABLE talker
       SHAPE configure
       TYPED)
   ```

3. Name the packages in the launch file, if the bringup has one. Without
   `launch/`, the build generates the launch file from the `[[component]]`
   rows. Either way, nothing names a language. The mixed example workspace's
   launch file:

   ```xml
   <launch>
     <node pkg="c_talker_pkg" exec="talker" name="talker"/>
     <node pkg="cpp_listener_pkg" exec="listener" name="listener"/>
     <node pkg="rust_heartbeat_pkg" exec="heartbeat" name="heartbeat"/>
   </launch>
   ```

4. Build. Once the image holds a C or C++ package, the build runs CMake, and
   the Rust packages build inside it through Corrosion. `--dry-run` shows the
   handoff:

   ```bash
   nros build linux --dry-run
   ```

   ```text
   nros build: host_node_bringup:linux -> board linux (platform posix), driver cmake
   cmake -S build/posix-zenoh-linux -B build/posix-zenoh-linux/cmake …
   cmake --build build/posix-zenoh-linux/cmake
   ```

   The generated `build/posix-zenoh-linux/CMakeLists.txt` is the CMake root.
   You do not write it, and the workspace has no root build file.

5. Run it:

   ```bash
   nros run host_node_bringup:linux
   ```

## Example

[`examples/workspaces/mixed/`](https://github.com/NEWSLabNTU/nano-ros/tree/main/examples/workspaces/mixed)
runs a C talker, a C++ listener and a Rust heartbeat in one image, with C and
C++ service and action pairs beside them. Its bringup states the image the
same way a one-language workspace does:

```toml
# src/demo_bringup/system.toml (excerpt)
[system]
name = "mixed_demo"
rmw = "zenoh"
domain_id = 0
default_launch = "system.launch.xml"

[[component]]
pkg = "c_talker_pkg"
class = "c_talker_pkg::Talker"
name = "talker"

[[component]]
pkg = "cpp_listener_pkg"
class = "cpp_listener_pkg::Listener"
name = "listener"

[image_defaults]
rmw = "zenoh"

[image.linux]
board = "linux"

[image.freertos]
board = "mps2-an385-freertos"
```

```bash
source /opt/ros/humble/setup.bash
nros build linux
nros run demo_bringup:linux
```

The first build records the ROS 2 prefix in the workspace, so later shells
need not source ROS 2.

## Per-language notes

- **Rust.** A node package is a library crate ending in
  `nros::node!(YourNode)`. Its message crates and cargo settings are
  generated under `build/`. In a mixed image its compiler errors are still
  cargo's.
- **C.** Its message bindings are generated during the CMake configure.
- **C++.** The component has a `configure(::rclcpp::Node&)` method, as in
  [Organize a larger workspace](organize-a-larger-workspace.md).
- **The RMW is stated once**, whatever the languages: `[system] rmw`, or
  `rmw =` on one image block. See
  [Switch the RMW per image](../rmw/switch-the-rmw-per-image.md).

## If it doesn't work

- **A node is missing at run time.** Its `[[component]]` row or launch
  `<node>` names a package or class that does not match the package's own
  registration (`PLUGIN` in CMake, the class in `nros::node!`).
- **The first build stops on `AMENT_PREFIX_PATH`.** ROS 2 was not sourced and
  the workspace has no recorded prefix yet. Source ROS 2 and build again.

## See also

- [Build outputs](../../reference/build-outputs.md#c-c-and-mixed-workspaces-cmake):
  where a CMake-driven image lands.
- [C API](../../reference/c-api.md) and [C++ API](../../reference/cpp-api.md).

## Design background

- [RFC-0121: workspace-first project layout](https://github.com/NEWSLabNTU/nano-ros/blob/main/docs/design/0121-workspace-first-project-layout.md)

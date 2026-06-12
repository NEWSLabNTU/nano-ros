# Zephyr C++ TYPED talker (RFC-0043 / phase-240.8)

The typed-component counterpart to `examples/zephyr/cpp/talker`. Instead of a
hand-written imperative `main.cpp`, the node is a **stateful component**
(`Talker::configure(node)` binds a `Publisher<Int32>` + a `bind_timer` member
callback that publishes a counter), and the **Zephyr typed carrier** generates
the entry:

- `nano_ros_node_register(TYPED … DEPLOY zephyr)` (with `NANO_ROS_PLATFORM=zephyr`)
  `configure_file`s `cmake/templates/zephyr_entry_main_typed.cpp.in` — a plain
  `int main(void)` that constructs the component, calls `configure(node)`, and
  runs `::nros::board::ZephyrBoard::run_components(&setup)` (locator-less; Zephyr
  `CONFIG_NET_CONFIG_AUTO_INIT` peer discovery).
- The branch composes the generated entry + the component lib into the global
  `find_package(Zephyr)` `app` target (`target_sources`/`target_link_libraries`)
  — no second executable.

## Status — build-gated draft (2026-06-13)

The Zephyr typed **carrier** (template + `NanoRosNodeRegister.cmake` branch,
phase-240.8) is implemented and render/review-verified. This example is the
**consumer**, but it is **not yet built** — there is no Zephyr SDK / west
workspace in the dev/CI environment that produced it (the carrier was validated
on the **NuttX** tier in QEMU instead; same `component.hpp` API + the proven
typed-entry shape).

**Open question to verify when the SDK is available:** the component static lib
(`<pkg>_<name>_component`) that `nano_ros_node_register` builds must find the
nros-cpp + `std_msgs` C++ headers on Zephyr. On native/NuttX it links
`NanoRos::NanoRosCpp` + `NROS_GENERATED_INTERFACE_LIBS`; on Zephyr those come via
the `find_package(Zephyr)` nros module (`zephyr_library_named(nros)`) +
`find_package(std_msgs)`, so the component-lib branch's interface provisioning
may need a Zephyr-specific path. Build on `native_sim/native/64` first
(`west build -b native_sim/native/64 -- -DSNIPPET=nros-zenoh`).

# C API and CMake Integration

## Overview

Phase 140 — nano-ros is consumed exclusively via
`add_subdirectory(<repo-root>)` from the user's CMakeLists. There is
no install step, no install prefix, no `find_package(NanoRos)`.

The nros C API is built via CMake + [Corrosion](https://github.com/corrosion-rs/corrosion) (v0.6.1), which integrates Cargo into CMake. Each `add_subdirectory(nano-ros)` invocation builds the staticlib(s) for the selected `(NANO_ROS_PLATFORM, NANO_ROS_RMW)` tuple in-tree.

## Usage

```cmake
set(NANO_ROS_PLATFORM posix)
set(NANO_ROS_RMW     zenoh)
add_subdirectory(third_party/nano-ros nano_ros)

add_executable(my_app src/main.c)
target_link_libraries(my_app PRIVATE NanoRos::NanoRos)
nros_platform_link_app(my_app)
```

This wires include dirs, RMW staticlib, platform shim, and per-build
config header automatically. See
[build-as-subdirectory.md](../../book/src/getting-started/build-as-subdirectory.md)
for the full walkthrough.

**RMW backend selection:** Pass `-DNANO_ROS_RMW=<zenoh|dds|xrce|cyclonedds>` to cmake.

## Code Generation

C code generation uses `nano_ros_generate_interfaces()` (from `cmake/NanoRosGenerateInterfaces.cmake`, included automatically by the root `CMakeLists.txt` once nano-ros is `add_subdirectory`'d). The codegen tool (`nros-codegen`) is a Corrosion-built target reachable via `$<TARGET_FILE:nros-codegen>`.

Always use `nano_ros_generate_interfaces()` for message/service/action types — never hand-write CDR serialization or struct definitions. The API mirrors `rosidl_generate_interfaces()` from standard ROS 2: interface files are positional arguments resolved first locally, then via ament index, then from bundled interfaces.

```cmake
# Standard ROS 2 package — resolved via AMENT_PREFIX_PATH or bundled
nano_ros_generate_interfaces(std_msgs
    "msg/Int32.msg"
    SKIP_INSTALL
)

# Another standard package
nano_ros_generate_interfaces(example_interfaces
    "srv/AddTwoInts.srv"
    "action/Fibonacci.action"
    SKIP_INSTALL
)

# Custom project-local interfaces
nano_ros_generate_interfaces(${PROJECT_NAME}
    "msg/Temperature.msg"
    SKIP_INSTALL
)
```

Resolution order for each file: `${CMAKE_CURRENT_SOURCE_DIR}/<file>` → `${AMENT_PREFIX_PATH}/share/<target>/<file>` → `<install_prefix>/share/nano-ros/interfaces/<target>/<file>`.

Type info structs (`nros_message_type_t`, `nros_service_type_t`, `nros_action_type_t`) are all defined in `nros/types.h`.

## Async Action Client (Executor-Driven)

Action and service client entrypoints split into two families:

- `nros_action_send_goal_async()`, `nros_action_get_result_async()`,
  `nros_action_cancel_goal()`, `nros_client_send_request_async()` — non-blocking,
  return immediately. Replies arrive via callbacks invoked from
  `nros_executor_spin_some()`.
- `nros_action_send_goal()`, `nros_action_get_result()`, `nros_client_call()` —
  blocking convenience wrappers. They call the async variant and then drive the
  executor (`nros_executor_spin_some` internally in a wall-clock budgeted loop)
  until the reply lands or timeout. They never call `zpico_get` directly — all
  I/O still flows through the registered executor. Calling any of them from
  inside a dispatch callback returns `NROS_RET_REENTRANT`.

The action-client blocking helpers (`nros_action_send_goal`,
`nros_action_get_result`) take the executor as an explicit `executor`
parameter because the action client stores its arena handle as an opaque
pointer into the executor's `_opaque` storage (set up by
`nros_executor_register_action_client`) rather than a wrapper pointer.
`nros_action_client_wait_for_action_server()` and
`nros_action_client_action_server_is_ready()` take the same explicit
executor argument for the same reason. `nros_client_call()` does not —
the service client stashes the executor pointer on
`nros_executor_register_client()` and recovers it internally.

Canonical call pattern (C):

```c
nros_support_init(&support, locator, domain_id);
nros_node_init(&node, &support, "c_action_client", "/");
nros_action_client_init(&client, &node, "/fibonacci", &type_info);
nros_action_client_set_feedback_callback(&client, on_feedback, NULL);
nros_action_client_set_result_callback(&client, on_result, NULL);

nros_executor_init(&executor, &support, /*pool=*/4);
nros_executor_register_action_client(&executor, &client);

/* Warm-up: let zenoh discover the action server. */
for (int i = 0; i < 300; ++i) {
    nros_executor_spin_some(&executor, 10000000ULL); /* 10 ms */
}

/* Async path: returns immediately; goal_response_callback fires during spin. */
nros_goal_uuid_t goal_uuid;
nros_action_send_goal_async(&client, goal_buf, goal_len, &goal_uuid);
while (!state.goal_responded) {
    nros_executor_spin_some(&executor, 10000000ULL);
}

/* Blocking convenience: same async-then-spin pattern, packaged. */
nros_goal_status_t status;
uint8_t result_buf[512];
size_t result_len = 0;
nros_action_get_result(&client, &executor, &goal_uuid,
                       &status, result_buf, sizeof(result_buf), &result_len);
```

Phase 122.3 added a separate **L1 polling** family
(`nros_action_client_init_polling` + `nros_action_client_send_goal_raw` /
`_try_recv_goal_response_raw` / `_send_get_result_request_raw` /
`_try_recv_result_raw` / `_send_cancel_request_raw` /
`_try_recv_cancel_response_raw` / `_try_recv_feedback_raw`) for callers
that drive the action lifecycle without an executor arena. The L1 family
stores its `ActionClientCore` inline in the `nros_action_client_t._opaque`
slot and does not require `nros_executor_register_action_client`.

## System Install

There is no system install. Phase 140 deleted every `install(...)`
rule. nano-ros ships in source form — the user project owns whatever
install layout it needs for *its* binaries.

## FreeRTOS / NuttX Cross-Compilation

Cross-compiled examples consume nano-ros the same way as POSIX:
`add_subdirectory(<repo>)` with `NANO_ROS_PLATFORM` set to the
target RTOS. Pass `CMAKE_TOOLCHAIN_FILE` for the cross-compiler.

**FreeRTOS (ARM Cortex-M3, MPS2-AN385):**

```bash
cmake -S examples/qemu-arm-freertos/c/zenoh/talker -B build/talker \
    -DCMAKE_TOOLCHAIN_FILE=$(pwd)/cmake/toolchain/arm-freertos-armcm3.cmake \
    -DNANO_ROS_PLATFORM=freertos \
    -DNANO_ROS_BOARD=mps2-an385-freertos \
    -DNANO_ROS_RMW=zenoh
cmake --build build/talker
```

**NuttX (ARM Cortex-A7):**

```bash
cmake -S examples/qemu-arm-nuttx/cpp/zenoh/talker -B build/talker \
    -DNANO_ROS_PLATFORM=nuttx \
    -DNANO_ROS_BOARD=nuttx-qemu-arm
cmake --build build/talker
```

The example's own `CMakeLists.txt` add_subdirectory's nano-ros; the
Corrosion target tree under `build/talker/cargo/` holds the
per-build staticlib (`libnros_c.a`, `libnros_rmw_zenoh_staticlib.a`, …).

## Zephyr Integration

**RMW backend selection** in `prj.conf`:
```ini
# Zenoh (default — connects to zenohd router)
CONFIG_NROS=y
CONFIG_NROS_RMW_ZENOH=y       # (default, can be omitted)

# XRCE-DDS (connects to Micro-XRCE-DDS Agent)
CONFIG_NROS=y
CONFIG_NROS_RMW_XRCE=y
CONFIG_NROS_XRCE_AGENT_ADDR="192.0.2.2"
CONFIG_NROS_XRCE_AGENT_PORT=2018
```

**API selection** in `prj.conf`:
```ini
CONFIG_NROS_RUST_API=y         # Rust API (default) — uses rust_cargo_application()
CONFIG_NROS_C_API=y            # C API — links libnros_c.a, uses nros_generate_interfaces()
```

Zenoh requires `CONFIG_POSIX_API=y` and elevated mutex counts. XRCE requires `CONFIG_NET_SOCKETS=y`. See existing examples in `examples/zephyr/` for complete `prj.conf` templates.

# C API and CMake Integration

## Overview

C examples use a config-mode CMake package. The nros C API is built via CMake + [Corrosion](https://github.com/corrosion-rs/corrosion) (v0.6.1), which integrates Cargo into CMake. The top-level `CMakeLists.txt` builds one RMW variant per invocation.

## Usage

```cmake
find_package(NanoRos REQUIRED CONFIG)
target_link_libraries(my_app PRIVATE NanoRos::NanoRos)
```

This provides include dirs, static library, and platform link libs (pthread, dl, m) automatically.

**RMW backend selection:** The `NANO_ROS_RMW` CMake variable selects which library variant to link (default: `zenoh`). Pass `-DNANO_ROS_RMW=xrce` for XRCE examples.

## Code Generation

C code generation uses `nano_ros_generate_interfaces()` (from `NanoRosGenerateInterfaces.cmake`, included automatically by `find_package(NanoRos CONFIG)`).

Always use `nano_ros_generate_interfaces()` for message/service/action types -- never hand-write CDR serialization or struct definitions. The API mirrors `rosidl_generate_interfaces()` from standard ROS 2: interface files are positional arguments resolved first locally, then via ament index, then from bundled interfaces.

```cmake
# Standard ROS 2 package -- resolved via AMENT_PREFIX_PATH or bundled
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

Resolution order for each file: `${CMAKE_CURRENT_SOURCE_DIR}/<file>` -> `${AMENT_PREFIX_PATH}/share/<target>/<file>` -> `<install_prefix>/share/nano-ros/interfaces/<target>/<file>`.

Type info structs (`nros_message_type_t`, `nros_service_type_t`, `nros_action_type_t`) are all defined in `nros/types.h`.

## C API by Header

### Core lifecycle (`nros/init.h`, `nros/executor.h`, `nros/node.h`)

- `nros_init(locator)` / `nros_shutdown()` — session open/close
- `nros_executor_init()` / `nros_spin_once(timeout_ms)` — executor lifecycle
- `nros_create_node(name)` — create a node handle

### Pub/Sub (`nros/publisher.h`, `nros/subscription.h`)

- `nros_publisher_init()` / `nros_publish(msg, size)`
- `nros_subscription_init()` / `nros_executor_add_subscription(callback)`

### Services (`nros/service.h`, `nros/client.h`)

- `nros_service_init()` / `nros_executor_add_service(callback)`
- `nros_client_init()` / `nros_call_service(req, req_size, reply, reply_size, timeout_ms)`

### Actions (`nros/action.h`)

**Action client:**

| Function | Description |
|----------|-------------|
| `nros_action_client_init()` | Create action client for a given action type |
| `nros_action_send_goal(goal, size, timeout_ms)` | Blocking: send goal, wait for acceptance |
| `nros_action_send_goal_async(goal, size)` | Non-blocking: send goal, poll via callbacks |
| `nros_action_get_result(goal_id, result, size, timeout_ms)` | Blocking: wait for result |
| `nros_action_get_result_async(goal_id)` | Non-blocking: poll via result callback |
| `nros_action_cancel_goal(goal_id, timeout_ms)` | Cancel an active goal |
| `nros_executor_add_action_client(executor, client)` | Register for callback-driven processing |

**Action server:**

| Function | Description |
|----------|-------------|
| `nros_action_server_init()` | Create action server |
| `nros_action_publish_feedback(goal, feedback, size)` | Publish feedback to the client |
| `nros_action_succeed(goal, result, size)` | Complete goal with success |
| `nros_action_abort(goal, result, size)` | Abort goal |
| `nros_action_canceled(goal, result, size)` | Mark goal as canceled |

**Callback registration:**

```c
nros_action_client_set_goal_response_callback(client, on_goal_accepted);
nros_action_client_set_feedback_callback(client, on_feedback);
nros_action_client_set_result_callback(client, on_result);
```

**Non-blocking pattern:** Use `_async` variants to avoid blocking the executor. The async call returns immediately; results arrive via the registered callback on subsequent `nros_spin_once()` calls.

### Timers (`nros/timer.h`)

- `nros_timer_init(period_ms)` / `nros_executor_add_timer(callback)`

### Parameters, Lifecycle, Guard Condition

- `nros/parameter.h` — declare/get/set parameters
- `nros/lifecycle.h` — lifecycle state machine
- `nros/guard_condition.h` — thread-safe trigger

## System Install

For package maintainers:

```bash
cmake -S . -B build -DNANO_ROS_RMW=zenoh -DCMAKE_BUILD_TYPE=Release
cmake --build build
cmake --install build --prefix /usr/local

# Multi-RMW: run cmake twice to same prefix (library names don't collide)
cmake -S . -B build-xrce -DNANO_ROS_RMW=xrce -DCMAKE_BUILD_TYPE=Release
cmake --build build-xrce
cmake --install build-xrce --prefix /usr/local
```

## Zephyr Integration

**RMW backend selection** in `prj.conf`:
```ini
# Zenoh (default -- connects to zenohd router)
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
CONFIG_NROS_RUST_API=y         # Rust API (default) -- uses rust_cargo_application()
CONFIG_NROS_C_API=y            # C API -- links libnros_c.a, uses nros_generate_interfaces()
```

Zenoh requires `CONFIG_POSIX_API=y` and elevated mutex counts. XRCE requires `CONFIG_NET_SOCKETS=y`. See existing examples in `examples/zephyr/` for complete `prj.conf` templates.

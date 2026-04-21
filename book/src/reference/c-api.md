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

All objects use **inline opaque storage** — no heap allocation. Declare
each as a `static` in `.bss` (the examples use an `app` struct) and pass
pointers to the `_init` functions. Pair every `_init` with a matching
`_fini` at shutdown.

### Core lifecycle (`nros/init.h`, `nros/executor.h`, `nros/node.h`)

```c
nros_support_t support = nros_support_get_zero_initialized();
nros_support_init(&support, "tcp/127.0.0.1:7447", /*domain_id=*/0);

nros_node_t node = nros_node_get_zero_initialized();
nros_node_init(&node, &support, "my_node", /*namespace=*/"");

nros_executor_t exec = nros_executor_get_zero_initialized();
nros_executor_init(&exec, &support, /*max_handles=*/8);

while (running) {
    nros_executor_spin_some(&exec, /*timeout_ms=*/100);
}

nros_executor_fini(&exec);
nros_node_fini(&node);
nros_support_fini(&support);
```

Related helpers:

- `nros_support_init_named()` — for XRCE sessions that need a distinct session name
- `nros_executor_spin(&exec)` — blocking loop (no return)
- `nros_executor_spin_period(&exec, period_ns)` — fixed-rate blocking loop
- `nros_executor_stop(&exec)` — signal the blocking spin to return

### Pub/Sub (`nros/publisher.h`, `nros/subscription.h`)

```c
// Publisher
nros_publisher_t pub = nros_publisher_get_zero_initialized();
nros_publisher_init(&pub, &node, &my_msg_type, "/counter");

uint8_t buf[64];
size_t len;
my_msg_serialize(&msg, buf, sizeof(buf), &len);
nros_publish_raw(&pub, buf, len);

// Subscription (callback-based via executor)
nros_subscription_t sub = nros_subscription_get_zero_initialized();
nros_subscription_init(&sub, &node, &my_msg_type, "/counter",
                       on_message_callback, &ctx);
nros_executor_add_subscription(&exec, &sub, NROS_EXECUTOR_ON_READY);
```

QoS customization: call `nros_publisher_init_with_qos(..., &qos_profile)` /
`nros_subscription_init_with_qos(..., &qos_profile)` with a QoS struct
(e.g. `NROS_QOS_SENSOR_DATA` for best-effort sensor data). The bare
`_init` forms use RELIABLE / KEEP_LAST(10).

### Services (`nros/service.h`, `nros/client.h`)

```c
// Service server (callback-based via executor)
nros_service_t srv = nros_service_get_zero_initialized();
nros_service_init(&srv, &node, &add_two_ints_type, "/add",
                  on_request_callback, &ctx);
nros_executor_add_service(&exec, &srv, NROS_EXECUTOR_ON_READY);

// Service client (blocking call)
nros_client_t cli = nros_client_get_zero_initialized();
nros_client_init(&cli, &node, &add_two_ints_type, "/add");
nros_executor_add_client(&exec, &cli, /*timeout_ms=*/5000);

uint8_t req_buf[64], resp_buf[64];
size_t req_len = /* serialize request into req_buf */, resp_len = 0;
nros_ret_t ret = nros_client_call(&cli,
                                   req_buf, req_len,
                                   resp_buf, sizeof(resp_buf), &resp_len);
```

`nros_client_call` spins the executor internally until the reply arrives
or the registered timeout expires. Returns `NROS_RET_REENTRANT` (`-15`)
if called from inside a dispatch callback — use a standalone non-blocking
client path from within callbacks.

> **Type-argument note** (Phase 84.B1): `nros_service_init` and
> `nros_client_init` currently take `nros_message_type_t*`. A future fix
> changes this to `nros_service_type_t*` to match the type struct in
> `nros/types.h` — the existing signature is a known bug.

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
| `nros_action_server_init()` | Create action server (registers `goal`, `cancel`, `accepted` callbacks) |
| `nros_action_execute(server, goal)` | Transition an accepted goal to `EXECUTING` |
| `nros_action_publish_feedback(server, goal, feedback, size)` | Publish feedback to the client |
| `nros_action_succeed(server, goal, result, size)` | Complete goal with success |
| `nros_action_abort(server, goal, result, size)` | Abort goal |
| `nros_action_canceled(server, goal, result, size)` | Mark goal as canceled |
| `nros_action_get_goal_status(server, goal, &status)` | Read arena-authoritative status; `NROS_RET_NOT_FOUND` for retired goals |
| `nros_action_server_get_active_goal_count(server)` | Count of active goals from the arena |

**Goal handle (`nros_goal_handle_t`):** a pure identity card containing only the
16-byte `uuid`. Per-goal user state (parsed goal data, progress, etc.) belongs
in caller-managed `{uuid → state}` storage. The handle pointer passed to
callbacks is valid only for the duration of that callback — copy it by value if
you need to reference the goal later.

**Server callback signatures** all receive the owning `server`, a
`const nros_goal_handle_t *`, and a user `context`:

```c
nros_goal_response_t goal_callback(
    nros_action_server_t* server,
    const nros_goal_handle_t* goal,
    const uint8_t* goal_request, size_t goal_len, void* context);

nros_cancel_response_t cancel_callback(
    nros_action_server_t* server,
    const nros_goal_handle_t* goal, void* context);

void accepted_callback(
    nros_action_server_t* server,
    const nros_goal_handle_t* goal, void* context);
```

Goal lifecycle (status, active-goal count) is owned by the arena in
`nros-node`. The C struct does not duplicate it: there are no `status` /
`active` fields on `nros_goal_handle_t`, and no `goals[]` / `active_goal_count`
fields on `nros_action_server_t`. Always read status via
`nros_action_get_goal_status`.

**Client callback registration:**

```c
nros_action_client_set_goal_response_callback(client, on_goal_accepted);
nros_action_client_set_feedback_callback(client, on_feedback);
nros_action_client_set_result_callback(client, on_result);
```

**Non-blocking pattern:** Use `_async` variants to avoid blocking the executor. The async call returns immediately; results arrive via the registered callback on subsequent `nros_spin_once()` calls.

### Timers (`nros/timer.h`)

```c
nros_timer_t tmr = nros_timer_get_zero_initialized();
nros_timer_init(&tmr, &support,
                /*period_ns=*/1000UL * 1000 * 1000,  // 1 s
                timer_callback, &ctx);
nros_executor_add_timer(&exec, &tmr, NROS_EXECUTOR_ALWAYS);
```

The callback signature is `void (*)(struct nros_timer_t* timer, void* context)`.
Period is expressed in **nanoseconds**.

### Parameters, Lifecycle, Guard Condition

- `nros/parameter.h` — declare/get/set parameters
- `nros/lifecycle.h` — lifecycle state machine
- `nros/guard_condition.h` — thread-safe trigger

#### ROS 2 lifecycle services (REP-2002)

The standalone `nros_lifecycle_state_machine_t` runs without any ROS 2
tooling hooks. To expose a node's lifecycle to `ros2 lifecycle
set|get|list|nodes`, register the five REP-2002 services on the
executor instead of initialising a standalone state machine. Enable the
`lifecycle-services` feature on `nros-c` (forwarded to `nros-node`) and
call:

```c
nros_ret_t r = nros_executor_register_lifecycle_services(exec);
nros_executor_lifecycle_register_on_configure(exec, on_configure, ctx);
nros_executor_lifecycle_register_on_activate (exec, on_activate,  ctx);
nros_executor_lifecycle_register_on_deactivate(exec, on_deactivate, ctx);
nros_executor_lifecycle_register_on_cleanup  (exec, on_cleanup,   ctx);
nros_executor_lifecycle_register_on_shutdown (exec, on_shutdown,  ctx);

uint8_t state = nros_executor_lifecycle_get_state(exec);
nros_executor_lifecycle_change_state(exec, NROS_LIFECYCLE_TRANSITION_CONFIGURE);
```

The executor owns the state machine; each `spin_once` drains the five
service servers (`~/change_state`, `~/get_state`,
`~/get_available_states`, `~/get_available_transitions`,
`~/get_transition_graph`) so `ros2 lifecycle` queries round-trip
without dedicated threads.

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

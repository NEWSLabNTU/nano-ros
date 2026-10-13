# Retired names

nano-ros's cmake variables use one prefix, `NROS_`. Older releases used
`NANO_ROS_` for some of them, and the release before this one still accepted
those spellings as deprecated aliases. They are now **retired**: a configure
that sets one — in a `CMakeLists.txt`, with `-D` on the command line, or left
behind in a build directory's `CMakeCache.txt` — stops with an error that names
the replacement. Nothing is adopted under the new name any more, so rename the
old spelling where you set it.

| Retired | Use instead |
|---|---|
| `NANO_ROS_PLATFORM` | `NROS_PLATFORM` |
| `NANO_ROS_FEATURES` | `NROS_FEATURES` |
| `NANO_ROS_SAFETY_E2E` | `NROS_SAFETY_E2E` |
| `NANO_ROS_ROS_EDITION` | `NROS_ROS_EDITION` |
| `NANO_ROS_GEN_CACHE_DIR` | `NROS_GEN_CACHE_DIR` |
| `NANO_ROS_SKIP_BOOTSTRAP` | `NROS_SKIP_BOOTSTRAP` |
| `NANO_ROS_BUILD_CODEGEN` | `NROS_BUILD_CODEGEN` |
| `NANO_ROS_CMAKE_DIR` | `NROS_CMAKE_DIR` |
| `NANO_ROS_RMW` | `NROS_RMW` |
| `NANO_ROS_BOARD` | `NROS_BOARD` |
| `NANO_ROS_ROOT` (cmake variable) | `NROS_REPO_DIR` |
| `NANO_ROS_ROOT <path>` (keyword of `nano_ros_workspace()` and friends) | `NROS_REPO_DIR <path>` |

## Retired knob spellings

Build-time knobs have one env spelling, `NROS_<ID>` (RFC-0103 D2b), held in
the knob registry `packages/tooling/nros-zephyr-build/knobs.toml`. The older
spellings below were read with a deprecation warning for one release. They are
now **retired**: a build that finds one set — exported in the environment, in a
cargo `[env]` table, or as an `[image.<id>] env` row in `system.toml` — stops
with an error that names the replacement. Nothing is read under the old name
any more, so rename it where you set it.

| Retired | Use instead |
|---|---|
| `ZPICO_BATCH_MULTICAST_SIZE` | `NROS_ZENOH_BATCH_MULTICAST_SIZE` |
| `ZPICO_BATCH_UNICAST_SIZE` | `NROS_ZENOH_BATCH_UNICAST_SIZE` |
| `ZPICO_FRAG_MAX_SIZE` | `NROS_ZENOH_FRAG_MAX_SIZE` |
| `ZPICO_GET_POLL_INTERVAL_MS` | `NROS_ZENOH_GET_POLL_INTERVAL_MS` |
| `ZPICO_GET_REPLY_BUF_SIZE` | `NROS_ZENOH_GET_REPLY_BUF_SIZE` |
| `ZPICO_GRAPH_CACHE_SIZE` | `NROS_ZENOH_GRAPH_CACHE_SIZE` |
| `ZPICO_GRAPH_DISCOVERY` | `NROS_ZENOH_GRAPH_DISCOVERY` |
| `ZPICO_LEASE_TASK_PRIORITY` | `NROS_ZENOH_LEASE_PRIORITY` |
| `ZPICO_MAX_LARGE_SUBSCRIBERS` | `NROS_ZENOH_MAX_LARGE_SUBSCRIBERS` |
| `ZPICO_MAX_LIVELINESS` | `NROS_ZENOH_MAX_LIVELINESS` |
| `ZPICO_MAX_PENDING_GETS` | `NROS_ZENOH_MAX_PENDING_GETS` |
| `ZPICO_MAX_PUBLISHERS` | `NROS_ZENOH_MAX_PUBLISHERS` |
| `ZPICO_MAX_QUERYABLES` | `NROS_ZENOH_MAX_QUERYABLES` |
| `ZPICO_MAX_SESSIONS` | `NROS_ZENOH_MAX_SESSIONS` |
| `ZPICO_MAX_SUBSCRIBERS` | `NROS_ZENOH_MAX_SUBSCRIBERS` |
| `ZPICO_MAX_TL_PUBLISHERS` | `NROS_ZENOH_MAX_TL_PUBLISHERS` |
| `ZPICO_PUBLISHER_TX_BUFFER_SIZE` | `NROS_ZENOH_PUBLISHER_TX_BUFFER_SIZE` |
| `ZPICO_READ_TASK_PRIORITY` | `NROS_ZENOH_READ_PRIORITY` |
| `ZPICO_SERVICE_BUFFER_SIZE` | `NROS_ZENOH_SERVICE_BUFFER_SIZE` |
| `ZPICO_SMOLTCP_BUFFER_SIZE` | `NROS_SMOLTCP_BUFFER_SIZE` |
| `ZPICO_SMOLTCP_CONNECT_TIMEOUT_MS` | `NROS_SMOLTCP_CONNECT_TIMEOUT_MS` |
| `ZPICO_SMOLTCP_MAX_SOCKETS` | `NROS_SMOLTCP_MAX_SOCKETS` |
| `ZPICO_SMOLTCP_MAX_UDP_SOCKETS` | `NROS_SMOLTCP_MAX_UDP_SOCKETS` |
| `ZPICO_SMOLTCP_SOCKET_TIMEOUT_MS` | `NROS_SMOLTCP_SOCKET_TIMEOUT_MS` |
| `ZPICO_SUBSCRIBER_LARGE_SIZE` | `NROS_ZENOH_SUBSCRIBER_LARGE_SIZE` |
| `ZPICO_SUBSCRIBER_RING_DEPTH` | `NROS_ZENOH_SUBSCRIBER_RING_DEPTH` |
| `ZPICO_SUBSCRIBER_SIZE_THRESHOLD` | `NROS_ZENOH_SUBSCRIBER_SIZE_THRESHOLD` |
| `ZPICO_TL_RETAIN_BYTES` | `NROS_ZENOH_TL_RETAIN_BYTES` |
| `ZPICO_TL_RETAIN_DEPTH` | `NROS_ZENOH_TL_RETAIN_DEPTH` |
| `ZPICO_TX_BATCH` | `NROS_ZENOH_TX_BATCH` |
| `ZPICO_TX_BATCH_FLUSH_MS` | `NROS_ZENOH_TX_BATCH_FLUSH_MS` |
| `ZPICO_TX_SPLIT_LOCK` | `NROS_ZENOH_TX_SPLIT_LOCK` |

The same `ZPICO_*` words remain the zenoh-pico shim's own C macro names (a
`-DZPICO_MAX_QUERYABLES=…` the build passes to the C compiler). Those are
generated from the knob, never set by hand, and are not on this list.

A board descriptor's knobs are a top-level `[knobs.<group>]` table. The
per-board `[board.knobs.<group>]` spelling is retired the same way: a
descriptor that still states it fails to load, naming `[knobs]`.

## Retired runtime aliases and macro forms

Deprecated in release `0.5.0-nros2`, retired after it. Each is now an ERROR
naming its replacement — never silently ignored, because an ignored locator
dials the default router and reads as "no delivery".

| Retired | Use instead | What happens now |
|---|---|---|
| `ZENOH_LOCATOR` (env) | `NROS_LOCATOR` | startup error (Rust panic / `InitError::RetiredEnvVar`; C/C++ init returns invalid-argument and logs it) |
| `ZENOH_MODE` (env) | `NROS_SESSION_MODE` | same |
| `nros::main!(board = X)` | `nros::main!()` + `[image.<id>] board` in `system.toml`; an out-of-tree board crate states `[package.metadata.nros.board] type = "::my_board::MyBoard"` | `compile_error!` |
| `--features rmw-xrce` on the PX4 companions | nothing — the xrce backend is unconditional | the feature no longer exists (cargo: unknown feature) |

## A build directory from an older release

An older nano-ros wrote some of these names into the cache itself, so a build
directory it first configured carries them even though nothing sets them now.
The error lists those entries and prints the command that drops exactly them:

```sh
cmake -U NANO_ROS_PLATFORM -U NANO_ROS_RMW <build-dir>
```

Deleting the build directory's `CMakeCache.txt` works too.

## What is not on this list

`nano_ros_DIR` and `nano_ros_ROOT` are CMake's own `find_package(nano_ros)`
hints and keep their spelling.

`NANO_ROS_ROOT` as an **environment** variable is retired by its own rule: a
configure that finds it set stops, naming `NROS_REPO_DIR`.

The error table is kept for one more release, so a project that skips a
release still hears what to type. After that the old names are simply unknown
variables.

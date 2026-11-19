# Phase-581 W0 - measurements for RFC-0140

Measured 2026-11-19 on branch `phase-581-w0` (based on the commit that accepted RFC-0140), inside the
`ros2` distrobox (Ubuntu 22.04, ROS 2 Humble), per
[phase-581 W0](../phase-581-ros-env-vars.md) and
[RFC-0140](../../design/0140-configuration-registry.md). Scratch files (drivers,
probe apps, raw logs) lived under `/mnt/wd/nt/p581w0/`, outside the checkout;
the trimmed configure logs are committed beside this file:
[M5](phase-581-w0-m5-configure.log), [M6](phase-581-w0-m6-configure.log).
Everything below comes from a command that was run; where nothing was run the
row says NOT MEASURED and why.

## 1. Run-time reads on Linux (RFC-0140 Appendix B)

### Method

Three images built from the in-tree examples with `nros build` (the CLI built
by `just setup-cli` in this worktree):

| Road | Leaf and image | Binary |
| --- | --- | --- |
| Rust | `examples/linux/rust/talker`, `native_talker_bringup:linux` | `build/posix-zenoh/linux_entry/target/debug/linux_entry` (a `nros::main!` entry, `linux` board) |
| C | `examples/linux/c/talker`, `native_c_talker_bringup:linux` | `build/posix-zenoh-linux-native_c_talker_bringup/cmake/pkg/native_c_talker/c_talker` (the example's own `main.c`) |
| C++ | `examples/linux/cpp/talker`, `cpp_talker_bringup:linux` | `build/posix-zenoh-linux-cpp_talker_bringup/cmake/pkg/cpp_talker/cpp_talker` (the example's own `main.cpp`) |

Each was run against a router started with the repo's own
`scripts/dev/zenohd.sh` `nros_router_exec tcp/127.0.0.1:17447`, under an
`LD_PRELOAD` shim that wraps `getenv` and `secure_getenv` and logs every name
asked for (Rust's `std::env::var` reaches libc `getenv`, so all three roads are
seen). "Read" below means the process called `getenv` for that name while it
was set; "acted" is a separate observation from the process's own output.

Every run sets `NROS_LOCATOR=tcp/127.0.0.1:17447 NROS_ENTRY_SPIN_MS=3000`, plus:

| Run | Extra variables |
| --- | --- |
| A | `ROS_DOMAIN_ID=7 ROS_LOCALHOST_ONLY=1 ROS_AUTOMATIC_DISCOVERY_RANGE=LOCALHOST ROS_STATIC_PEERS=127.0.0.1 RMW_IMPLEMENTATION=rmw_cyclonedds_cpp ROS_NAMESPACE=/w0ns ROS_LOG_DIR=... ROS_HOME=... RCUTILS_CONSOLE_OUTPUT_FORMAT=W0FMT_{severity}_{message} RCUTILS_COLORIZED_OUTPUT=1 RCUTILS_LOGGING_USE_STDOUT=1 RCUTILS_LOGGING_BUFFERED_STREAM=1 ROS_DISABLE_LOANED_MESSAGES=1 ROS_SECURITY_ENABLE=true ROS_SECURITY_STRATEGY=Enforce ROS_SECURITY_KEYSTORE=/nonexistent ROS_SECURITY_ENCLAVE_OVERRIDE=/w0 RUST_LOG=debug ZENOH_MULTICAST_SCOUTING=false ZENOH_SCOUTING_TIMEOUT=1000 ZENOH_CONFIG_OVERRIDE=scouting/multicast/enabled=false ZENOH_ROUTER_CONFIG_URI=/nonexistent RMW_ZENOH_CONFIG_URI=/nonexistent CYCLONEDDS_URI=file:///nonexistent NROS_SESSION_MODE=client NROS_RMW=zenoh NROS_NODE_NAME=w0node NROS_NODE_NAMESPACE=/w0ns2 NROS_ENTRY_SPIN_STEP_MS=100 NROS_ENTRY_EXPECT_MESSAGE_CALLBACKS=0 NROS_RMW_TRACE_OPEN=1 NROS_RMW_CYCLONEDDS_SKIP_PREFIX=w0 NROS_CYCLONE_MATCH_TIMEOUT_MS=1 XRCE_SERIAL_BAUD=115200 NROS_PROFILE_INPUTS=1 NROS_DOMAIN_ID=9` |
| A2 | `ZENOH_TLS_ROOT_CA_CERTIFICATE=/nonexistent ZENOH_TLS_ROOT_CA_CERTIFICATE_BASE64=AAAA ZENOH_TLS_VERIFY_NAME_ON_CONNECT=false` |
| B1 | `ZENOH_SESSION_CONFIG_URI=<file connecting to tcp/127.0.0.1:17447>` |
| B2 | `ZENOH_LISTEN=tcp/127.0.0.1:17999` |
| B3 | `ZENOH_SESSION_CONFIG_URI=<file connecting to tcp/127.0.0.1:1>` (the `NROS_LOCATOR` above still set) |
| B4 | `NROS_LOCATOR` unset; once with no file, once with the B1 file |
| C1 | `ZENOH_LOCATOR=tcp/127.0.0.1:17447` |
| C2 | `ZENOH_MODE=peer` |
| D1 | `NROS_CENSUS_OUT=<file>` |
| D2 | `NROS_PROFILE_OUT=<file>` |
| D3 | `NROS_GRAPH_DUMP=1` |
| D4 | `NROS_BLESS_PROBE_SIDECAR=<file>` |
| E | `ROS_DOMAIN_ID=999` |
| F | `NROS_LOCATOR=tcp/127.0.0.1:1` |

### Results

"R" read, "-" not read on this boot path. Acted: what the output showed.

| Variable | Rust | C | C++ | Acted (observed) |
| --- | --- | --- | --- | --- |
| `ROS_DOMAIN_ID` | R | R | R | Yes, all three: boot line `domain 7 (env)`; Rust also printed `ROS_DOMAIN_ID=7 overrides system.toml's domain_id=0` and its liveliness keyexpr carried domain 7. Run E (999): Rust panicked `domain id exceeds DOMAIN_ID_MAX (232)`, C `rclc_support_init -> -3`, C++ `rclcpp::init_in() -> -3` |
| `NROS_LOCATOR` | R | R | R | Yes, all three: run F failed to connect to `tcp/127.0.0.1:1` |
| `NROS_SESSION_MODE` | R | R | R | Read; `client` equals the default, so no change was observable |
| `NROS_RMW` | R | R | R | Yes: boot line `rmw zenoh (env)` on all three |
| `NROS_NODE_NAME`, `NROS_NODE_NAMESPACE` | R | R | R | Read, but the node kept its own name: Rust liveliness `.../NN/%/%/talker`, C++ `Node created: talker`. The component (Rust) and application (C, C++) name outranks the env rung on these images |
| `RMW_IMPLEMENTATION` | - | - | - | Not read on any road (its reader, `Context::default_from_env`, is not on these boot paths) |
| `ROS_NAMESPACE`, `ROS_LOCALHOST_ONLY`, `ROS_AUTOMATIC_DISCOVERY_RANGE`, `ROS_STATIC_PEERS`, `ROS_LOG_DIR`, `ROS_HOME`, the four `RCUTILS_*`, `ROS_DISABLE_LOANED_MESSAGES`, the four `ROS_SECURITY_*` | - | - | - | Not read |
| `RUST_LOG` | R | - | - | Rust: `debug` produced `[DEBUG]` keyexpr lines |
| `ZENOH_MULTICAST_SCOUTING`, `ZENOH_SCOUTING_TIMEOUT` | R | - | - | Rust read; no observable change on a client session |
| `ZENOH_TLS_ROOT_CA_CERTIFICATE`, `_BASE64`, `ZENOH_TLS_VERIFY_NAME_ON_CONNECT` | R | - | - | Rust, on a `tcp/` locator: `session config: key 'root_ca_certificate' needs a zenoh-pico built with Z_FEATURE_LINK_TLS`, session open failed (`InvalidArgument`) |
| `ZENOH_LISTEN` | R | - | - | Rust: session open failed (`ConnectionFailed`) with the router up |
| `ZENOH_SESSION_CONFIG_URI` | R | R | R | Yes, all three: run B4 with `NROS_LOCATOR` unset opened a session only when the file was given. Run B3: with `NROS_LOCATOR` set, the file's `connect` (port 1) did not apply, the env locator did |
| `ZENOH_CONFIG_OVERRIDE`, `ZENOH_ROUTER_CONFIG_URI`, `RMW_ZENOH_CONFIG_URI` | - | - | - | Not read |
| `ZENOH_LOCATOR`, `ZENOH_MODE` | R | R | R | Refused on all three: `ZENOH_LOCATOR is retired and no longer read; set NROS_LOCATOR instead`; Rust panics (exit 134), C and C++ init returns `-3` |
| `CYCLONEDDS_URI` | - | - | - | Not read by these zenoh images. Cyclone images: NOT MEASURED (none built) |
| `NROS_ENTRY_SPIN_MS` | R | - | - | Rust: exited after 3.0 s (`hosted spin complete`). The C and C++ examples are hand-written `main`s with their own spin loop and ran until killed |
| `NROS_ENTRY_SPIN_STEP_MS`, `NROS_ENTRY_EXPECT_MESSAGE_CALLBACKS` | R | - | - | Rust entry only |
| `NROS_RMW_TRACE_OPEN` | R | R | R | Rust printed the `open: locator=... ret=...` trace line in run A, C and C++ too |
| `NROS_CENSUS_OUT` | R | R | R | Yes: `rmw metadata (env)`, census file written, exit 0 on all three |
| `NROS_PROFILE_OUT` | R | - | R | Profile file written on Rust and C++; the C image did not read it |
| `NROS_PROFILE_INPUTS` | R* | - | R* | *Read only in run D2 (with `NROS_PROFILE_OUT` set), where it was unset |
| `NROS_GRAPH_DUMP`, `NROS_BLESS_PROBE_SIDECAR` | - | - | - | Not read by a talker image |
| `NROS_RMW_CYCLONEDDS_SKIP_PREFIX`, `NROS_CYCLONE_MATCH_TIMEOUT_MS`, `XRCE_SERIAL_BAUD` | - | - | - | Not read by zenoh images; their own backends: NOT MEASURED (no Cyclone or XRCE image built) |
| `NROS_DOMAIN_ID` (at run time) | - | - | - | Not read at run time (a build-time name) |
| `NROS_BRIDGE_<N>_*` | | | | NOT MEASURED (the bridge binary was not built) |

Other ROS- or nano-ros-shaped names the shim saw being read: `RUST_BACKTRACE`
(Rust panic path) only.

## 2. Zephyr `native_sim` (Decision 8)

A plain Zephyr app (`main()` prints `getenv()` of five names) built for
`native_sim/native/64` on 3.7 and 4.4, once with `CONFIG_PICOLIBC=y` and once
with `CONFIG_EXTERNAL_LIBC=y`, run as
`P581W0_PROBE=hello ROS_DOMAIN_ID=7 NROS_LOCATOR=tcp/192.0.2.1:7447 CYCLONEDDS_URI=file:///x zephyr.exe -stop_at=1`:

| Line | libc | Result |
| --- | --- | --- |
| 3.7 | PICOLIBC | every `getenv` returned `(null)`, `PATH` included |
| 3.7 | EXTERNAL_LIBC | every value seen (`hello`, `7`, ...) |
| 4.4 | PICOLIBC | every `getenv` returned `(null)` |
| 4.4 | EXTERNAL_LIBC | every value seen |

nano-ros `native_sim` images are PICOLIBC: the existing 3.7 and 4.4
`build-c-talker-zenoh` builds in the Zephyr workspaces have `CONFIG_PICOLIBC=y`
and `CONFIG_NATIVE_LIBC_INCOMPATIBLE=y` (selected by Zephyr's POSIX options,
`lib/posix/options/Kconfig.profile`), so EXTERNAL_LIBC is not available to
them. Running those two `zephyr.exe` (built 2026-10-07) with
`ROS_DOMAIN_ID=7 NROS_LOCATOR=... CYCLONEDDS_URI=...` under the shim: the host
libc saw one `getenv`, `NATIVE_SIM_RESET_CAUSE`, and no nano-ros name.

Result: a nano-ros `native_sim` image has no process environment. RFC-0109 D6a
(which lists `native_sim` among hosted builds whose environment overrides the
baked wiring) is wrong; the comment in
`packages/rmw/cyclonedds/nros-rmw-cyclonedds/src/session.cpp` (line 594,
"native_sim's picolibc getenv sees no host environment") is right.

## 3. `freertos-posix` Rust road (Decision 11)

Static, by reading: `nros-board-freertos-posix/src/lib.rs:117` calls
`nros_board_freertos::run_entry`, whose `entry.rs:376` resolves with
`::nros::ExecutorConfig::resolve`, documented at
`packages/core/nros-node/src/executor/types.rs:661` as "with no environment
rung". `threadx-linux` instead calls `resolve_boot`
(`nros-board-threadx/src/entry.rs:1580`), which uses `nros::env::resolve_hosted`.

Run time: NOT MEASURED. No in-tree image is a Rust `freertos-posix` image
(`git grep -l 'board = "freertos-posix"' -- examples` finds only the C and C++
workspaces). A scratch copy of `examples/linux/rust/talker` with an added
`[image.fp] board = "freertos-posix"` did not build: `error[E0152]: found
duplicate lang item panic_impl`, because that node crate depends on
`env_logger` (std) and the `freertos-posix` entry is `#![no_std]`. Making a
node crate for the probe was out of this item's scope. The static reading
above is the evidence that the Rust road on `freertos-posix` has no
environment rung today. The C and C++ `freertos-posix` images were not run
either; section 1 measured the `nros-c`/`nros-cpp` resolver acting on
`ROS_DOMAIN_ID` on the `linux` board only.

## 4. `ROS_NAMESPACE` against stock Humble (Decision 3)

`ROS_DOMAIN_ID=117 ROS_LOCALHOST_ONLY=1`, default RMW (`rmw_fastrtps_cpp`):

| Command | `ros2 node list --no-daemon` | `ros2 topic list --no-daemon` |
| --- | --- | --- |
| `ROS_NAMESPACE=/w0ns ros2 run demo_nodes_cpp talker` | `/talker` | `/chatter` |
| `ROS_NAMESPACE=/w0ns ros2 run demo_nodes_py talker` | `/talker` | `/chatter` |
| `ROS_NAMESPACE=w0rel .../demo_nodes_cpp/talker` | `/talker` | |
| control: `.../demo_nodes_cpp/talker --ros-args -r __ns:=/w0ns` | `/w0ns/talker` | `/w0ns/chatter` |

`strings` of `librcl.so`, `librclcpp.so`, `librcl_yaml_param_parser.so` and
`librcutils.so`: 0 occurrences of `ROS_NAMESPACE`; 0 files under
`site-packages/rclpy` and `site-packages/launch_ros` mention it.

Result on Humble: stock rcl, rclcpp and rclpy ignore `ROS_NAMESPACE`.
Jazzy: NOT MEASURED. The box has no Jazzy install and no `nano-ros-ros:jazzy`
image exists (`docker images`); building one (`just ros_editions image jazzy`)
pulls the ROS Jazzy base image, which this item does not do.

## 5. M5 - an RMW choice member from another file (RFC-0140 D6)

Log: [phase-581-w0-m5-configure.log](phase-581-w0-m5-configure.log). A guest
app, `BOARD=native_sim`, the member

```
if NROS
choice NROS_RMW_BACKEND
config NROS_RMW_ACME
    bool "acme (phase-581 W0 M5 probe)"
    select P581W0_ACME_SELECTED_MARKER
endchoice
endif
```

placed (a) in an out-of-tree Zephyr module's `zephyr/Kconfig`
(`ZEPHYR_EXTRA_MODULES=<nano-ros>;<acme_mod>`) and (b) in
`<build>/nros/Kconfig.packages`, `osource`d by a line added for the run beside
the module Kconfig's existing `osource ".../Kconfig.from-system-toml"` (line
1404; reverted with `git checkout zephyr/Kconfig`). Same result on 3.7 and 4.4
in both shapes:

| conf | `.config` |
| --- | --- |
| `CONFIG_NROS_RMW_ACME=y` | `CONFIG_NROS_RMW_ACME=y`, `# CONFIG_NROS_RMW_ZENOH is not set`, `CONFIG_P581W0_ACME_SELECTED_MARKER=y` |
| `..._ACME=y` then `..._ZENOH=y` | `CONFIG_NROS_RMW_ZENOH=y`, `# CONFIG_NROS_RMW_ACME is not set` |
| `..._ZENOH=y` then `..._ACME=y` | `CONFIG_NROS_RMW_ACME=y`, marker `y` |
| no member assigned | `CONFIG_NROS_RMW_ZENOH=y` (the choice's default) |

No warning was printed in any case. Result: GREEN. A member from another file
joins the named choice, can be selected and its `select` applies. Kconfig does
NOT refuse a conf assignment of another member: the last assignment wins,
silently. So the refusal W8 needs is the module hook's job.

## 6. M6 - a conf assignment to an undefined symbol (RFC-0140 D4)

Log: [phase-581-w0-m6-configure.log](phase-581-w0-m6-configure.log). A plain
Zephyr app (no nano-ros module, so no hook), `BOARD=native_sim`, conf
`CONFIG_P581W0_UNDEFINED_SYMBOL=y` and `CONFIG_NROS_P581W0_UNDEFINED_INT=4`.
On 3.7 and 4.4 alike: `warning: attempt to assign the value 'y' to the
undefined symbol P581W0_UNDEFINED_SYMBOL` (and the same for the int), then
`error: Aborting due to Kconfig warnings`, configure exit status 1, no
`.config`. Result: a guest that assigns a retired (now undefined) symbol fails
configure on both lines, but the message names no replacement; the hook's own
refusal is what names it.

## 7. Census of the `NROS_MAX_*` demand carriers (for W7)

Command: `git grep -nE 'NROS_MAX_(PUBLISHERS|SUBSCRIBERS|QUERYABLES|LIVELINESS|PENDING_GETS|LARGE_SUBSCRIBERS)\b' -- ':!docs' ':!*.md' ':!third-party'`:
198 hits in 35 files (the pattern also matches `CONFIG_NROS_MAX_*`,
`NROS_RESOLVED_NROS_MAX_*` and `NROS_KNOB_SOURCE_NROS_MAX_*`). Classified by
reading each hit:

**Definitions (generated Kconfig views).** `zephyr/Kconfig:2648, 2675, 2699,
2708, 2731, 2764`; `zephyr/Kconfig.knob-prompts:187, 190, 193, 196, 199, 202`;
`cmake/NanoRosKnobRegistry.cmake:187, 190, 193, 196, 199, 202`;
`packages/tooling/nros-zephyr-build/knobs.toml:1749, 1784, 1816, 1833, 1864, 1911`
(the `kconfig` column); `knob_registry.rs:2015, 2038, 2061, 2084, 2107, 2151`
(rows), `2541-2546` (`KCONFIG_PAIRS`), `2948-2983` (`KconfigKnob` symbols).

**cmake readers of the Kconfig demand symbol (Zephyr road only).**
`zephyr/cmake/nros_cargo_build.cmake:821-822` (`SUBSCRIBERS`), `827-828`
(`PUBLISHERS`), `830-831` (`QUERYABLES`), `838-839` (`LIVELINESS`): each
`_nros_resolve_derivable_knob(NROS_MAX_<X> "${CONFIG_NROS_MAX_<X>}" NROS_DERIVED_MAX_<X> ...)`,
which sets the cmake variable `NROS_RESOLVED_NROS_MAX_<X>`; `:1177`
(`PENDING_GETS`, straight into `NROS_ZENOH_MAX_PENDING_GETS`); `:1334-1336`
(`LARGE_SUBSCRIBERS`, into `NROS_ZENOH_MAX_LARGE_SUBSCRIBERS`).

**cmake readers of the resolved demand.** `zephyr/cmake/nros_cargo_build.cmake:1073-1078`
(`_nros_c_array_pool_floor` of `NROS_RESOLVED_NROS_MAX_{PUBLISHERS,SUBSCRIBERS,QUERYABLES}`
into `NROS_ZENOH_MAX_*`), `:1174-1175` (`LIVELINESS`). The XRCE pools at
`:1561` and `:1576` read the derived `NROS_DERIVED_MAX_{SUBSCRIBERS,QUERYABLES}`,
not an `NROS_MAX_*` name. `cmake/NanoRosStackFloor.cmake:497` matches
`NROS_MAX_{PUBLISHERS,SUBSCRIBERS,QUERYABLES}=` entries of
`NROS_BOARD_FACTS_ENV` as stack-floor shape keys.

**Rust build-script readers (through the ladder's `$DOTCONFIG` rung).**
`nros_zephyr_build::kconfig_key_for` (`lib.rs:446-451`, used at `:544`)
maps `NROS_ZENOH_MAX_<X>` to `CONFIG_NROS_MAX_<X>` through the row's
`kconfig`; the knobs read are at
`packages/rmw/zenoh/nros-zpico-build/src/runner.rs:1325, 1342, 1346, 1354, 1366`
(via `env_usize`, `:1584`) and `packages/rmw/zenoh/nros-rmw-zenoh/build.rs:321, 343`.

**CLI readers.** `packages/cli/nros-cli-core/src/cmd/leaf_settings.rs:772-777`
(`registry_knob_of_env_row` accepts `NROS_MAX_<X>` as the Kconfig spelling of
the zenoh row); `leaf_kconfig.rs:537`; and the generic readers of
`KnobRow::kconfig` at `builder/build_args.rs:216, 734`,
`builder/knob_roads.rs:295-308`, `cmd/knob.rs:191, 299`,
`cmd/leaf_system.rs:1091`, `cmd/locate.rs:996`.

**Gates, tools and data.** `scripts/check-kconfig-knob-forwarding.sh:228-231`;
`scripts/check-knob-delivery.py:88-133, 515-616, 677-678`;
`scripts/check-declared-fact-carriers.py:289-339`;
`scripts/check-leaf-conf-nros-knobs.py:315-342` (self-test);
`scripts/nros-stack-floor.py:2232-2233` (`SHAPE_KNOBS`) and its self-tests at
`3336-3628`; `config/stack-floors.toml:102-104, 122-124, 140-142, 158-160` and
`config/stack-floors.json:35-37, 55-57, 75-77, 93-95` (shape keys);
`scripts/knob-delivery-fixture.py:148-176`;
`tests/fixtures/knob-delivery/island-shaped-zephyr/CMakeCache.txt:33-99`;
`tests/cmake-message-bounds-tests.sh:376`; `scripts/read-boot-report.py:901,
906` (message text).

**Not readers (comment or message text only).**
`cmake/NanoRosMessageBounds.cmake:8, 28, 36, 67, 783, 839, 853, 858, 1351, 1393`
and `cmake/NanoRosCodegenCore.cmake:612, 1470`: these files read and publish
`NROS_DERIVED_MAX_LARGE_SUBSCRIBERS` (`NanoRosMessageBounds.cmake:423, 494,
850, 1319-1321, 1388-1390, 1464`), never an `NROS_MAX_*` value. Also
`packages/rmw/zenoh/nros-rmw-zenoh/src/shim/publisher.rs:794`,
`shim/service.rs:1178`, `nros-zpico-build/src/runner.rs:450, 474`, and doc
comments in `entity_inventory.rs`, `message_bound_knobs.rs`, `bounds.rs`,
`lib.rs:96-111`.

Finding: the `NROS_MAX_<X>` cmake carriers exist only on the Zephyr road
(`nros_cargo_build.cmake`); RFC-0140 D3's "the `NROS_MAX_<X>` cmake carriers in
`cmake/NanoRosMessageBounds.cmake` and `cmake/NanoRosCodegenCore.cmake`" does
not hold. Off Zephyr the demand reaches the pools as `NROS_ZENOH_MAX_*`
(`leaf_entity_env.rs:878-879`) and the sizing descriptor
(`runner.rs:1342-1348`).

## 8. Owners, build readers and units

**`SERVICE_TIMEOUT_MS`.** One resolver,
`nros_platform_config::platform_config::resolve_service_timeout_ms`
(`packages/tooling/nros-platform-config/src/platform_config.rs:1830-1833`),
has two callers: the zenoh backend's build script
(`packages/rmw/zenoh/nros-rmw-zenoh/build.rs:248-249`, the Rust const used by
`shim/service.rs:1797`) and `nros-build-helpers` (`src/c.rs:130`), which bakes
`SERVICE_DEFAULT_TIMEOUT_MS` into `nros-c`'s config
(`templates/nros_c_config.rs.template:8`), read by `nros-c/src/service.rs:1159`
for every C service client whatever the RMW. Owner: core. New key
`service.timeout_ms`; its env spelling `NROS_SERVICE_TIMEOUT_MS` is already the
mechanical one.

**`ENTRY_SPIN_MS`.** `git grep -nE 'ENTRY_SPIN_MS|entry\.spin_ms|spin_ms'`
outside docs: no `rerun-if-env-changed=NROS_ENTRY_SPIN_MS`, no `option_env!` or
`env!` of it, no reader of `CONFIG_NROS_ENTRY_SPIN_MS` (the symbol is not in
`zephyr/Kconfig`). Every reader is run time: `nros-cpp/include/rclcpp/detail/main.hpp:160`,
`nros-board-common/c/nros_rtos_run_components.c:205`,
`nros-board-linux/src/lib.rs:1366`, `nros-board-threadx-linux/src/lib.rs:65`,
`nros-cpp/src/lib.rs:10993`, `rmw/bridge/src/config.rs:293`,
`nros-macros/src/main_macro.rs:5328, 5344, 5347`. Confirmed: no build-time
reader. (The run above shows the Rust entry acting on it.)

**Units of the seven rows whose help states none**, from each consumer:

| Row | Consumer | Unit |
| --- | --- | --- |
| `KEYEXPR_STRING_SIZE` | `nros-rmw-zenoh/build.rs:254` emits `KEYEXPR_STRING_SIZE`, used as `heapless::String<KEYEXPR_STRING_SIZE>` (`shim/publisher.rs:549`) | bytes (UTF-8 capacity) |
| `LET_BUFFER_SIZE` | `nros-build-helpers/src/c.rs:119` writes `LET_BUFFER_SIZE` into `nros_c_config.rs.template:5`; `git grep -nw LET_BUFFER_SIZE` finds no other use | none: the constant has no reader |
| `SMOLTCP_BUFFER_SIZE` | `nros-smoltcp/build.rs:54` emits `SOCKET_BUFFER_SIZE`, used as `[u8; SOCKET_BUFFER_SIZE]` (`src/bridge.rs:106`) | bytes |
| `ZENOH_BATCH_MULTICAST_SIZE` | `nros-zpico-build/src/lib.rs:393` emits `Z_BATCH_MULTICAST_SIZE`, compared with the link MTU and returned as the batch size (zenoh-pico `include/zenoh-pico/transport/multicast/transport.h:39-42`) | bytes |
| `ZENOH_PUBLISHER_TX_BUFFER_SIZE` | `nros-rmw-zenoh/build.rs:392`, used as `[u8; ZENOH_TX_BUF]` (`shim/publisher.rs:1259, 1278`) | bytes |
| `ZENOH_SUBSCRIBER_SIZE_THRESHOLD` | `nros-rmw-zenoh/build.rs:104`, compared with `SUBSCRIBER_BUFFER_SIZE` and payload lengths (`shim/subscriber.rs:247`) | bytes |
| `XRCE_BUILTIN_SERVICE_REQUEST_BUFFER_SIZE` | `nros-rmw-xrce/src/internal.h:732`, `uint8_t builtin_request_bytes[...][XRCE_BUILTIN_SERVICE_REQUEST_BUFFER_SIZE]` | bytes |

Finding: `LET_BUFFER_SIZE` (`runtime.let_buffer_size`) is resolved and baked
but nothing reads the result, so it is retired with no replacement rather than
renamed.

## 9. User-settable cmake `NROS_*` entries

A script over `git ls-files '*.cmake' '*CMakeLists.txt'` minus `third-party/`,
comments stripped, listing `set(NROS_* ... CACHE <TYPE> ...)` with a type other
than `INTERNAL` and every `option(NROS_* ...)`: **52 names** (RFC-0140's first
count, 31, used a narrower scope it did not record; this one covers every
tracked cmake file, `examples/` and `tests/` included). Classified by each
entry's docstring and use:

| Class | Count | Names |
| --- | --- | --- |
| row (a setting; becomes a registry row or statement) | 16 | `NROS_RMW` (statement `rmw`), `NROS_PLATFORM`, `NROS_ROS_EDITION`, `NROS_FEATURES` (`[system].features`), `NROS_SAFETY_E2E`, `NROS_LINK_IP`, `NROS_STACK_FLOOR`, `NROS_RUST_MAIN_STACK_FLOOR` (bytes; would need `_BYTES` under N4), `NROS_BOARD_RUNNER`, `NROS_S32DS_API`, `NROS_PLATFORM_FREERTOS_HEAP_3`, `NROS_PLATFORM_FREERTOS_WITH_NET`, `NROS_PLATFORM_FREERTOS_WITH_BAREMETAL_COMPAT`, `NROS_PLATFORM_THREADX_WITH_NET`, `NROS_RMW_CYCLONEDDS_FOREIGN_WAKE`, `NROS_RMW_CYCLONEDDS_FOREIGN_WAKE_BRIDGE` |
| resource (a path to a tool, tree or file, RFC-0103 D6) | 16 | `NROS_CYCLONE_CMAKE_DIR`, `NROS_CYCLONE_IDLC`, `NROS_CYCLONE_SCRIPTS_DIR`, `NROS_HOST_IDLC`, `NROS_NUTTX_BOARD_MAKEDEFS`, `NROS_NUTTX_CROSS`, `NROS_NUTTX_DEFCONFIG`, `NROS_NUTTX_PROVISION_SCRIPT`, `NROS_PLATFORM_CFFI_INCLUDE`, `NROS_PLATFORM_IMPL_DIR`, `NROS_PLATFORM_POSIX_C_SRC_DIR`, `NROS_REPO_DIR`, `NROS_RMW_CFFI_DIR`, `NROS_RMW_CYCLONEDDS_MSG_TO_IDL`, `NROS_THREADX_LLVM_AR`, `NROS_THREADX_STRIP_SCRIPT` |
| plumbing (exempt: build wiring, test switches, values our own cmake sets) | 20 | `NROS_BUILD_CODEGEN`, `NROS_CARGO_PROFILE`, `NROS_CODEGEN_CARGO_PROFILE`, `NROS_PLATFORM_FREERTOS_INSTALL`, `NROS_PLATFORM_THREADX_INSTALL`, `NROS_PLATFORM_LINK_FEATURES`, `NROS_RMW_COMPANION_LIBRARIES`, `NROS_RMW_CYCLONEDDS_DDSC_LIBRARY`, `NROS_RMW_CYCLONEDDS_IPO`, `NROS_RMW_CYCLONEDDS_BUILD_TESTS`, `NROS_RMW_CYCLONEDDS_RUN_SERVICE_CONCURRENT`, `NROS_RMW_UORB_BUILD_TESTS`, `NROS_RMW_UORB_LINK_PX4`, `NROS_RMW_UORB_BUILD_PX4_GLUE`, `NROS_RMW_UORB_SIZING_DESCRIPTOR`, `NROS_RMW_XRCE_C_BUILD_TESTS`, `NROS_SMOKE_FREERTOS_HEAP`, `NROS_SYSTEM_DIR`, `NROS_SYSTEM_BRINGUP_DIR`, `NROS_XRCE_CFFI_OUT_DIR` |

The classification is W9's starting list; W9 records each in a row or in
`config-exemptions.toml` with its reason.

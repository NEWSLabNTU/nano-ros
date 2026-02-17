# Phase 45 — Configurable Constants & Named Magic Numbers

## Status: Not Started

## Background

Phase 40 made zenoh-pico buffer sizes configurable via `ZPICO_*` environment
variables, and the subsequent shim slot count work extended this to all
zenoh-specific tuning parameters (`ZPICO_MAX_PUBLISHERS`, `ZPICO_SUBSCRIBER_BUFFER_SIZE`,
etc.).

However, several areas still have hardcoded constants that users cannot tune
without editing source code:

1. **XRCE-DDS backend** — slot counts, buffer sizes, timeouts, and C library
   configuration have no env var exposure
2. **zpico-sys C shim** — service reply buffer size and poll interval are
   hardcoded
3. **zpico-smoltcp** — socket count, buffer sizes, and timeouts are hardcoded
4. **nros-rmw-zenoh** — buffer size literals scattered as magic numbers
5. **nros-c** — executor limits, message buffer size, and timeouts are hardcoded
6. **nros-node** — buffer size defaults are named constants but not env-configurable
7. **nros-params** — parameter storage limits are hardcoded

This phase brings all backends and core libs to parity, following the
established prefix conventions: `XRCE_*` for XRCE-DDS, `ZPICO_*` for
zenoh-pico, and `NROS_*` for core library constants.

### Goals

1. **Expose XRCE slot counts, buffer sizes, and timeouts as `XRCE_*` env vars**
2. **Expose zpico-sys and zpico-smoltcp tuning as `ZPICO_*` env vars**
3. **Expose nros-c executor limits as `NROS_*` env vars**
4. **Replace magic numbers with named constants** across all crates
5. **Introduce `config.rs` convention** — each crate with env-configurable constants
   gets a dedicated config module for discoverability
6. **Document all new env vars in CLAUDE.md**

### Non-Goals

- Changing default values (all defaults stay the same)
- Runtime configuration (all values remain compile-time)
- Renaming existing env vars (`XRCE_TRANSPORT_MTU`, `ZPICO_*` already correct)
- Making protocol-fixed values configurable (CDR alignment, GID sizes, etc.)
- Making const-generic defaults env-configurable (already user-tunable via type params)

---

## Conventions

### Three-Tier Constant Naming

| Tier | Scope | Prefix | Example |
|------|-------|--------|---------|
| 1. Public cross-crate | Env-configurable, used by other crates | Matches env var (`ZPICO_`, `XRCE_`, `NROS_`) | `ZPICO_MAX_SUBSCRIBERS` |
| 2. Crate-internal env-configurable | Env-configurable, used only within the crate | No prefix — descriptive name | `SUBSCRIBER_BUFFER_SIZE` |
| 3. Internal-only | Not env-configurable, implementation detail | No prefix — descriptive name | `SESSION_FLUSH_TIMEOUT_MS` |

Tier 1 constants are `pub` and appear in the crate's public API. Tier 2 constants are
`pub(crate)` and only referenced within the defining crate. Tier 3 constants are `const`
(or `pub(crate)`) and stay near their usage in implementation modules.

### config.rs Convention (Option C)

Each crate with env-configurable constants (Tier 1 or Tier 2) gets a dedicated
`src/config.rs` module. This module contains **only** the `include!()` for build.rs-generated
constants — nothing else. This provides a single, predictable location to discover all
tunable parameters in each crate.

**What goes in config.rs:**
- `include!(concat!(env!("OUT_DIR"), "/generated_constants.rs"))` — env-configurable values

**What does NOT go in config.rs:**
- Internal-only constants (Tier 3) — stay near their usage in implementation modules
- Protocol-fixed public constants (`ZPICO_ZID_SIZE`, CDR alignment) — stay in `ffi.rs` / `types.rs`
- Const generic defaults — stay at the type definition site

**Crates that get config.rs:** zpico-sys, nros-rmw-zenoh, nros-rmw-xrce, zpico-smoltcp,
xrce-smoltcp, nros-c, nros-params (7 crates total — see step 45.15).

**Crates that do NOT get config.rs:** nros-node (const generics, not env-configurable),
nros-core (protocol constants only), nros-serdes (CDR protocol constants only).

---

## Part A — XRCE-DDS Backend

### 45.1 — Slot Counts and Buffer Size (nros-rmw-xrce)

**Crate:** `nros-rmw-xrce`

Create `build.rs` for `nros-rmw-xrce` that reads env vars and generates a
`xrce_config.rs` file with the configurable constants. Replace the hardcoded
`pub const` / `const` values in `lib.rs` with `include!()`.

| Env Var                    | Current Constant      | Default | Line      |
|----------------------------|-----------------------|---------|-----------|
| `XRCE_MAX_SUBSCRIBERS`     | `MAX_SUBSCRIBERS`     | `8`     | lib.rs:49 |
| `XRCE_MAX_SERVICE_SERVERS` | `MAX_SERVICE_SERVERS` | `4`     | lib.rs:52 |
| `XRCE_MAX_SERVICE_CLIENTS` | `MAX_SERVICE_CLIENTS` | `4`     | lib.rs:55 |
| `XRCE_BUFFER_SIZE`         | `BUFFER_SIZE`         | `1024`  | lib.rs:58 |
| `XRCE_STREAM_HISTORY`      | `STREAM_HISTORY`      | `4`     | lib.rs:67 |

**`XRCE_STREAM_HISTORY` validation:** The build.rs must enforce `>= 2`. XRCE
reliable streams with history=1 fail to recycle the single slot between
separate `run_session_until_all_status` calls, causing entity creation timeouts.

**Static array initialization:** The slot arrays (`SUBSCRIBER_SLOTS`,
`SERVICE_SERVER_SLOTS`, `SERVICE_CLIENT_SLOTS`) are currently initialized with
repeated `Slot::new()` calls matching the hardcoded count. After this change,
generate the array initialization in `build.rs` or use a `const` block with
the generated size.

**Files modified:**
- `packages/xrce/nros-rmw-xrce/build.rs` (new)
- `packages/xrce/nros-rmw-xrce/Cargo.toml` (no new deps needed)
- `packages/xrce/nros-rmw-xrce/src/lib.rs`

### 45.2 — Timeout and Retry Tuning (nros-rmw-xrce)

**Crate:** `nros-rmw-xrce`

Add to the same `build.rs` from 45.1:

| Env Var                           | Current Constant             | Default | Line      |
|-----------------------------------|------------------------------|---------|-----------|
| `XRCE_ENTITY_CREATION_TIMEOUT_MS` | `ENTITY_CREATION_TIMEOUT_MS` | `1000`  | lib.rs:71 |
| `XRCE_SERVICE_REPLY_TIMEOUT_MS`   | `SERVICE_REPLY_TIMEOUT_MS`   | `1000`  | lib.rs:74 |
| `XRCE_SERVICE_REPLY_RETRIES`      | `SERVICE_REPLY_RETRIES`      | `5`     | lib.rs:77 |

**Files modified:**
- `packages/xrce/nros-rmw-xrce/build.rs` (extend from 45.1)
- `packages/xrce/nros-rmw-xrce/src/lib.rs`

### 45.3 — C Library Configuration (xrce-sys)

**Crate:** `xrce-sys`

Expose the UXR C library session/heartbeat parameters as env vars in the
existing `build.rs`. These are written into the generated `config.h`.

| Env Var                                | C Define                                     | Default | build.rs line |
|----------------------------------------|----------------------------------------------|---------|---------------|
| `XRCE_MAX_SESSION_CONNECTION_ATTEMPTS` | `UXR_CONFIG_MAX_SESSION_CONNECTION_ATTEMPTS` | `10`    | 177           |
| `XRCE_MIN_SESSION_CONNECTION_INTERVAL` | `UXR_CONFIG_MIN_SESSION_CONNECTION_INTERVAL` | `25`    | 178           |
| `XRCE_MIN_HEARTBEAT_TIME_INTERVAL`     | `UXR_CONFIG_MIN_HEARTBEAT_TIME_INTERVAL`     | `100`   | 179           |

Pattern: use the existing `env_usize()` helper already in `xrce-sys/build.rs`.

**Files modified:**
- `packages/xrce/xrce-sys/build.rs`

### 45.4 — Embedded UDP Metadata Slots (xrce-smoltcp)

**Crate:** `xrce-smoltcp`

Expose `UDP_META_COUNT` as a build-time env var. This controls how many
in-flight UDP packets smoltcp can buffer per direction.

| Env Var               | Current Constant | Default | Line      |
|-----------------------|------------------|---------|-----------|
| `XRCE_UDP_META_COUNT` | `UDP_META_COUNT` | `4`     | lib.rs:44 |

Requires adding a `build.rs` to `xrce-smoltcp` (currently has none).

**Files modified:**
- `packages/xrce/xrce-smoltcp/build.rs` (new)
- `packages/xrce/xrce-smoltcp/src/lib.rs`

### 45.5 — XRCE Named Internal Constants

Replace magic numbers with named constants. No env vars — these are
implementation details that don't need user tuning.

**nros-rmw-xrce/src/lib.rs:**

| Magic Number | Lines                              | Proposed Name               | Notes                                          |
|--------------|------------------------------------|-----------------------------|------------------------------------------------|
| `100`        | 136, 765, 849, 937                 | `SESSION_FLUSH_TIMEOUT_MS`  | Timeout for `uxr_run_session_time` flush calls |
| `10`         | 531                                | `SESSION_CREATION_RETRIES`  | Retries for `uxr_create_session_retries`       |
| `128`        | 628-632, 696-701, 786-803, 874-891 | `DDS_NAME_BUF_SIZE`         | Stack buffers for DDS topic/type/service names |
| `64`         | 551                                | `PARTICIPANT_NAME_BUF_SIZE` | Stack buffer for participant name              |
| `5381`       | 278                                | `DJB2_INIT`                 | djb2 hash initial value                        |
| `33`         | 280                                | `DJB2_MULTIPLIER`           | djb2 hash multiplier                           |

**nros-rmw-xrce/src/posix_udp.rs:**

| Magic Number | Lines  | Proposed Name                                               |
|--------------|--------|-------------------------------------------------------------|
| `64` / `63`  | 15, 28 | `AGENT_ADDR_BUF_SIZE` / derive as `AGENT_ADDR_BUF_SIZE - 1` |

**nros-rmw-xrce/src/posix_serial.rs:**

| Magic Number  | Lines  | Proposed Name                                           |
|---------------|--------|---------------------------------------------------------|
| `256` / `255` | 14, 31 | `PTY_PATH_BUF_SIZE` / derive as `PTY_PATH_BUF_SIZE - 1` |
| `1000`        | 135    | `SERIAL_DEFAULT_TIMEOUT_MS`                             |

**xrce-sys/build.rs:**

| Magic Number | Line | Proposed Name            | Notes                              |
|--------------|------|--------------------------|------------------------------------|
| `512`        | 197  | `UXR_SESSION_BLOB_SIZE`  | Opaque blob for `uxrSession`       |
| `256`        | 198  | `UXR_TRANSPORT_OVERHEAD` | Transport blob overhead beyond MTU |

**Files modified:**
- `packages/xrce/nros-rmw-xrce/src/lib.rs`
- `packages/xrce/nros-rmw-xrce/src/posix_udp.rs`
- `packages/xrce/nros-rmw-xrce/src/posix_serial.rs`
- `packages/xrce/xrce-sys/build.rs`

---

## Part B — Zenoh-Pico Backend

### 45.6 — Rename ZENOH_SHIM_ Constants to ZPICO_

**Crates:** `zpico-sys`, `nros-rmw-zenoh`, `zpico-zephyr`

Rename all `ZENOH_SHIM_` prefixed constants to `ZPICO_` for consistency with
the crate and env var naming. This is a bulk rename across 9 files (~187
occurrences). Function names (`zenoh_shim_init`, `zenoh_shim_open`, etc.)
keep their current names — only constants change.

**Rust constants (zpico-sys/src/ffi.rs):**

| Old Name | New Name |
|----------|----------|
| `ZENOH_SHIM_ZID_SIZE` | `ZPICO_ZID_SIZE` |
| `ZENOH_SHIM_RMW_GID_SIZE` | `ZPICO_RMW_GID_SIZE` |
| `ZENOH_SHIM_OK` | `ZPICO_OK` |
| `ZENOH_SHIM_ERR_GENERIC` | `ZPICO_ERR_GENERIC` |
| `ZENOH_SHIM_ERR_CONFIG` | `ZPICO_ERR_CONFIG` |
| `ZENOH_SHIM_ERR_SESSION` | `ZPICO_ERR_SESSION` |
| `ZENOH_SHIM_ERR_TASK` | `ZPICO_ERR_TASK` |
| `ZENOH_SHIM_ERR_KEYEXPR` | `ZPICO_ERR_KEYEXPR` |
| `ZENOH_SHIM_ERR_FULL` | `ZPICO_ERR_FULL` |
| `ZENOH_SHIM_ERR_INVALID` | `ZPICO_ERR_INVALID` |
| `ZENOH_SHIM_ERR_PUBLISH` | `ZPICO_ERR_PUBLISH` |
| `ZENOH_SHIM_ERR_TIMEOUT` | `ZPICO_ERR_TIMEOUT` |

**Generated constants (zpico-sys/build.rs → shim_constants.rs):**

| Old Name | New Name |
|----------|----------|
| `ZENOH_SHIM_MAX_PUBLISHERS` | `ZPICO_MAX_PUBLISHERS` |
| `ZENOH_SHIM_MAX_SUBSCRIBERS` | `ZPICO_MAX_SUBSCRIBERS` |
| `ZENOH_SHIM_MAX_QUERYABLES` | `ZPICO_MAX_QUERYABLES` |
| `ZENOH_SHIM_MAX_LIVELINESS` | `ZPICO_MAX_LIVELINESS` |

**C defines (zenoh_shim.c, zenoh_shim.h, `-D` flags):**

Same rename for all `ZENOH_SHIM_*` `#define`s and `-D` compiler flags.
The C header `zenoh_shim.h` is auto-generated by cbindgen — update the
cbindgen `include` list entries to use `ZPICO_*` names.

**`ZENOH_SHIM_GET_REPLY_BUF_SIZE`** → `ZPICO_GET_REPLY_BUF_SIZE` (zenoh_shim.c:112)

**Files modified:**
- `packages/zpico/zpico-sys/build.rs` (generated const names + `-D` flag names)
- `packages/zpico/zpico-sys/src/ffi.rs` (const names)
- `packages/zpico/zpico-sys/src/lib.rs` (test references)
- `packages/zpico/zpico-sys/cbindgen.toml` (export list)
- `packages/zpico/zpico-sys/c/shim/zenoh_shim.c` (~115 occurrences)
- `packages/zpico/zpico-sys/c/include/zenoh_shim.h` (~15 occurrences)
- `packages/zpico/nros-rmw-zenoh/src/zpico.rs` (imports)
- `packages/zpico/nros-rmw-zenoh/src/shim.rs` (imports + usage)
- `packages/zpico/zpico-zephyr/src/bsp_zephyr.c` (4 occurrences)

### 45.7 — Service Reply Buffer and Poll Interval (zpico-sys)

**Crate:** `zpico-sys`

Expose the service client get-reply buffer size and poll interval as env vars.
These affect maximum service response size and service latency/CPU tradeoff.

| Env Var                      | C Define/Literal           | Default | File              | Line                                                   |
|------------------------------|----------------------------|---------|-------------------|--------------------------------------------------------|
| `ZPICO_GET_REPLY_BUF_SIZE`   | `ZPICO_GET_REPLY_BUF_SIZE` | `4096`  | zenoh_shim.c:112  | Stack buffer for service client replies                |
| `ZPICO_GET_POLL_INTERVAL_MS` | `ZPICO_GET_POLL_INTERVAL_MS` | `10`  | zenoh_shim.c:1196 | Single-threaded polling interval in `zenoh_shim_get()` |

Pass as `-D` compiler flags from `build.rs` (same pattern as shim slot counts).

**Files modified:**
- `packages/zpico/zpico-sys/build.rs` (add to `ShimConfig`)
- `packages/zpico/zpico-sys/c/shim/zenoh_shim.c` (replace `#define` and literal with compiler-provided values)

### 45.8 — Zenoh smoltcp Transport Tuning (zpico-smoltcp)

**Crate:** `zpico-smoltcp`

Expose socket/buffer/timeout constants as build-time env vars. These directly
control memory footprint on embedded platforms (4 sockets x 2048 bytes = 16 KB
base allocation).

| Env Var                            | Current Constant     | Default | File         | Line                       |
|------------------------------------|----------------------|---------|--------------|----------------------------|
| `ZPICO_SMOLTCP_MAX_SOCKETS`        | `MAX_SOCKETS`        | `4`     | bridge.rs:19 | Max concurrent TCP sockets |
| `ZPICO_SMOLTCP_BUFFER_SIZE`        | `SOCKET_BUFFER_SIZE` | `2048`  | bridge.rs:22 | Per-socket staging buffer  |
| `ZPICO_SMOLTCP_CONNECT_TIMEOUT_MS` | `CONNECT_TIMEOUT_MS` | `30000` | tcp.rs:13    | TCP connection timeout     |
| `ZPICO_SMOLTCP_SOCKET_TIMEOUT_MS`  | `SOCKET_TIMEOUT_MS`  | `10000` | tcp.rs:16    | TCP read/write timeout     |

Requires adding a `build.rs` to `zpico-smoltcp` (currently has none).

**Files modified:**
- `packages/zpico/zpico-smoltcp/build.rs` (new)
- `packages/zpico/zpico-smoltcp/src/bridge.rs`
- `packages/zpico/zpico-smoltcp/src/tcp.rs`

### 45.9 — Zenoh Shim Named Constants (nros-rmw-zenoh)

Replace magic number literals with named constants in `nros-rmw-zenoh`. No env
vars — these are internal implementation details.

**nros-rmw-zenoh/src/shim.rs:**

| Magic Number         | Lines                       | Proposed Name            | Notes                                         |
|----------------------|-----------------------------|--------------------------|-----------------------------------------------|
| `128`                | 579, 593                    | `LOCATOR_BUFFER_SIZE`    | Null-terminated locator string buffer         |
| `64`                 | 605, 616                    | `CONFIG_PROPERTY_SIZE`   | Property key/value buffer size                |
| `8`                  | 605, 607, 612               | `MAX_SESSION_PROPERTIES` | Max session config properties                 |
| `257`                | 818, 1087                   | `KEYEXPR_BUFFER_SIZE`    | Key expression buffer (256 + null)            |
| `256`                | 812, 1081, 1518, 1626, 1670 | `KEYEXPR_STRING_SIZE`    | heapless::String capacity for key expressions |
| `64`                 | 321, 352                    | `MANGLED_NAME_SIZE`      | Buffer for topic/namespace name mangling      |
| `32`                 | 318, 348, 354               | `ZID_HEX_SIZE`           | 16-byte ZID as hex (32 ASCII chars)           |
| `32`                 | 354, 390, 426, 462          | `QOS_STRING_SIZE`        | QoS encoding string buffer                    |
| `1_000_000`          | 855                         | `TIMESTAMP_INCREMENT_NS` | Placeholder timestamp increment (1ms in ns)   |
| `0x517cc1b727220a95` | 189                         | `GID_PRNG_MULTIPLIER`    | LCG multiplier for GID generation             |

**Duplicate magic `8` — use imported constants:**

The hardcoded `8` at lines 1017, 1074, 1568 is used as the subscriber/service
buffer limit but should reference the already-imported
`ZPICO_MAX_SUBSCRIBERS` and `ZPICO_MAX_QUERYABLES` from zpico-sys
(renamed from `ZENOH_SHIM_MAX_*` in 45.6).

| Magic Number | Lines      | Replace With            |
|--------------|------------|-------------------------|
| `8`          | 1017, 1074 | `ZPICO_MAX_SUBSCRIBERS` |
| `8`          | 1568       | `ZPICO_MAX_QUERYABLES`  |

**zpico-smoltcp/src/bridge.rs:**

| Magic Number | Lines        | Proposed Name          | Notes                                     |
|--------------|--------------|------------------------|-------------------------------------------|
| `49152`      | 25, 186, 187 | `EPHEMERAL_PORT_START` | RFC 6056 ephemeral port range lower bound |

**Files modified:**
- `packages/zpico/nros-rmw-zenoh/src/shim.rs`
- `packages/zpico/zpico-smoltcp/src/bridge.rs`

---

## Part C — Core Libraries

### 45.10 — Rename NANO_ROS_ Constants to NROS_

**Crates:** `nros-c`, C codegen, examples, tests

Rename all uppercase `NANO_ROS_` prefixed constants, enums, and type names to
`NROS_` for consistency with the crate naming convention. Function names
(lowercase `nano_ros_*`) stay unchanged — this phase only renames the
uppercase identifiers.

**Scope:** ~1,900 occurrences across ~50 files.

**Categories of identifiers renamed:**

| Category           | Example Old                     | Example New                 | Count |
|--------------------|---------------------------------|-----------------------------|-------|
| Return codes       | `NANO_ROS_RET_OK`               | `NROS_RET_OK`               | ~200  |
| Executor constants | `NANO_ROS_EXECUTOR_MAX_HANDLES` | `NROS_EXECUTOR_MAX_HANDLES` | ~50   |
| Executor enums     | `NANO_ROS_SEMANTICS_LET`        | `NROS_SEMANTICS_LET`        | ~30   |
| QoS profiles       | `NANO_ROS_QOS_DEFAULT`          | `NROS_QOS_DEFAULT`          | ~30   |
| Lifecycle states   | `NANO_ROS_LIFECYCLE_STATE_*`    | `NROS_LIFECYCLE_STATE_*`    | ~100  |
| Action enums       | `NANO_ROS_GOAL_STATUS_*`        | `NROS_GOAL_STATUS_*`        | ~120  |
| Parameter types    | `NANO_ROS_PARAMETER_*`          | `NROS_PARAMETER_*`          | ~130  |
| Platform defines   | `NANO_ROS_PLATFORM_*`           | `NROS_PLATFORM_*`           | ~30   |
| Visibility macros  | `NANO_ROS_PUBLIC`               | `NROS_PUBLIC`               | ~20   |
| Header guards      | `NANO_ROS_*_H`                  | `NROS_*_H`                  | ~20   |
| Misc limits        | `NANO_ROS_MAX_*`                | `NROS_MAX_*`                | ~50   |

**Note:** Lowercase identifiers (`nano_ros_executor_t`, `nano_ros_init()`,
etc.) are NOT renamed. The C API function/type naming convention was
intentionally kept as `nano_ros_*` in Phase 33.1 for user-facing stability.
Only the uppercase constant/enum/macro identifiers change.

**C codegen:** Update `rosidl-codegen` to emit `NROS_` prefixed constants
in generated C code (types.rs, lib.rs).

**Files modified:**
- `packages/core/nros-c/src/*.rs` (15 files, ~1,040 occurrences)
- `packages/core/nros-c/include/nros/*.h` (21 files, ~740 occurrences)
- `packages/codegen/packages/rosidl-codegen/src/{lib,types}.rs` (~9 occurrences)
- `examples/native/c/**/*.c` (8 files, ~80 occurrences)
- `examples/zephyr/c/**/*.c` (2 files, ~7 occurrences)
- `examples/native/c/xrce/**/*.c` (2 files, ~15 occurrences)
- `examples/zephyr/c/**/prj.conf` (2 files, ~8 occurrences)
- `packages/testing/nros-tests/src/fixtures/binaries.rs` (~4 occurrences)
- `packages/zpico/zpico-zephyr/include/nano_ros_bsp_zephyr.h` (header guard)

### 45.11 — C API Executor Limits (nros-c)

**Crate:** `nros-c`

Add a `build.rs` to `nros-c` that reads env vars and generates constants.
These control executor capacity and memory footprint — critical for embedded
systems where the C API is the primary interface.

| Env Var                     | Current Constant          | Default | File            | Line |
|-----------------------------|---------------------------|---------|-----------------|------|
| `NROS_EXECUTOR_MAX_HANDLES` | `NROS_EXECUTOR_MAX_HANDLES` | `16`    | executor.rs:17  |      |
| `NROS_MAX_SUBSCRIPTIONS`    | `NROS_MAX_SUBSCRIPTIONS`    | `8`     | executor.rs:20  |      |
| `NROS_MAX_TIMERS`           | `NROS_MAX_TIMERS`           | `8`     | executor.rs:23  |      |
| `NROS_MAX_SERVICES`         | `NROS_MAX_SERVICES`         | `4`     | executor.rs:26  |      |
| `NROS_LET_BUFFER_SIZE`      | `LET_BUFFER_SIZE`           | `512`   | executor.rs:31  |      |
| `NROS_MESSAGE_BUFFER_SIZE`  | `MESSAGE_BUFFER_SIZE`       | `4096`  | executor.rs:678 |      |
| `NROS_MAX_CONCURRENT_GOALS` | `NROS_MAX_CONCURRENT_GOALS` | `4`     | action.rs:19    |      |

**Note:** These constants are exported to C headers via cbindgen. The
`build.rs` must generate the Rust constants (for `include!()`), and cbindgen
must be updated to exclude them from source parsing (same pattern as zpico-sys
shim slots — provide values via generated file, not source literals).

**Files modified:**
- `packages/core/nros-c/build.rs` (new)
- `packages/core/nros-c/src/executor.rs`
- `packages/core/nros-c/src/action.rs`

### 45.12 — C API Named Constants (nros-c)

Replace magic number literals with named constants. No env vars.

**nros-c/src/executor.rs:**

| Magic Number  | Lines      | Proposed Name        | Notes                                        |
|---------------|------------|----------------------|----------------------------------------------|
| `100_000_000` | 182, 249   | `DEFAULT_TIMEOUT_NS` | Default executor timeout (100ms in ns)       |
| `10_000_000`  | 1009, 1097 | `MAX_SLEEP_NS`       | Maximum sleep cap to maintain responsiveness |

**Files modified:**
- `packages/core/nros-c/src/executor.rs`

### 45.13 — Parameter Storage Limits (nros-params)

**Crate:** `nros-params`

Add a `build.rs` to `nros-params` that reads env vars. These control the
static storage sizes for parameter names, values, and arrays — significant for
memory-constrained embedded targets.

| Env Var                     | Current Constant       | Default | File         | Line |
|-----------------------------|------------------------|---------|--------------|------|
| `NROS_MAX_PARAMETERS`       | `MAX_PARAMETERS`       | `32`    | server.rs:13 |      |
| `NROS_MAX_PARAM_NAME_LEN`   | `MAX_PARAM_NAME_LEN`   | `64`    | types.rs:12  |      |
| `NROS_MAX_STRING_VALUE_LEN` | `MAX_STRING_VALUE_LEN` | `256`   | types.rs:15  |      |
| `NROS_MAX_ARRAY_LEN`        | `MAX_ARRAY_LEN`        | `32`    | types.rs:18  |      |
| `NROS_MAX_BYTE_ARRAY_LEN`   | `MAX_BYTE_ARRAY_LEN`   | `256`   | types.rs:21  |      |

**Files modified:**
- `packages/core/nros-params/build.rs` (new)
- `packages/core/nros-params/src/types.rs`
- `packages/core/nros-params/src/server.rs`

---

## Part E — config.rs Migration

### 45.15 — Introduce config.rs Modules

Create `src/config.rs` in each crate that has env-configurable constants. Migrate
existing `include!()` calls from implementation files into the new module. For new
crates (from earlier steps), the `include!()` should go directly into config.rs
rather than the implementation file.

**Pattern for each config.rs:**

```rust
//! Build-time configurable constants.
//!
//! Values are set via environment variables at build time.
//! See build.rs for env var names and defaults.

include!(concat!(env!("OUT_DIR"), "/generated_constants.rs"));
```

**Pattern for lib.rs:**

```rust
pub(crate) mod config;
// or `pub mod config;` if constants are Tier 1 (cross-crate)
```

**Per-crate changes:**

| Crate | Action | Constants Moved | Visibility |
|-------|--------|-----------------|------------|
| `zpico-sys` | Move `include!()` from `ffi.rs` to new `config.rs` | `ZPICO_MAX_PUBLISHERS`, `ZPICO_MAX_SUBSCRIBERS`, `ZPICO_MAX_QUERYABLES`, `ZPICO_MAX_LIVELINESS` | `pub` (Tier 1, cross-crate) |
| `nros-rmw-zenoh` | Move `include!()` from `shim.rs` to new `config.rs` | `SUBSCRIBER_BUFFER_SIZE`, `SERVICE_BUFFER_SIZE` | `pub(crate)` (Tier 2) |
| `nros-rmw-xrce` | Place `include!()` from 45.1/45.2 in new `config.rs` | All 8 constants from 45.1 + 45.2 | `pub(crate)` (Tier 2) |
| `zpico-smoltcp` | Place `include!()` from 45.8 in new `config.rs` | `MAX_SOCKETS`, `SOCKET_BUFFER_SIZE`, `CONNECT_TIMEOUT_MS`, `SOCKET_TIMEOUT_MS` | `pub(crate)` (Tier 2) |
| `xrce-smoltcp` | Place `include!()` from 45.4 in new `config.rs` | `UDP_META_COUNT` | `pub(crate)` (Tier 2) |
| `nros-c` | Place `include!()` from 45.11 in new `config.rs` | All 7 constants from 45.11 | `pub(crate)` (Tier 2) |
| `nros-params` | Place `include!()` from 45.13 in new `config.rs` | All 5 constants from 45.13 | `pub(crate)` (Tier 2) |

**zpico-sys special case:** The `ZPICO_MAX_*` constants are Tier 1 (cross-crate) because
`nros-rmw-zenoh` imports them. The config module must be `pub mod config;` and
`nros-rmw-zenoh` references them as `zpico_sys::config::ZPICO_MAX_SUBSCRIBERS`. Protocol-fixed
constants (`ZPICO_ZID_SIZE`, `ZPICO_RMW_GID_SIZE`, error codes) stay in `ffi.rs`.

**nros-rmw-zenoh special case:** The `include!()` currently at shim.rs line 951 moves to
config.rs. Importing modules use `use crate::config::SUBSCRIBER_BUFFER_SIZE;`.

**C crate special case (nros-c):** The C header constants are provided via `-D` compiler
flags (same as zpico-sys pattern), not from config.rs. The config.rs only holds the Rust-side
constants used in the Rust implementation. cbindgen export list must be updated to exclude
the generated constants.

**Files modified:**
- `packages/zpico/zpico-sys/src/config.rs` (new)
- `packages/zpico/zpico-sys/src/lib.rs` (add `pub mod config;`)
- `packages/zpico/zpico-sys/src/ffi.rs` (remove `include!()`)
- `packages/zpico/nros-rmw-zenoh/src/config.rs` (new)
- `packages/zpico/nros-rmw-zenoh/src/lib.rs` (add `pub(crate) mod config;`)
- `packages/zpico/nros-rmw-zenoh/src/shim.rs` (remove `include!()`, add `use crate::config::*;`)
- `packages/xrce/nros-rmw-xrce/src/config.rs` (new)
- `packages/xrce/nros-rmw-xrce/src/lib.rs` (add `pub(crate) mod config;`)
- `packages/zpico/zpico-smoltcp/src/config.rs` (new)
- `packages/zpico/zpico-smoltcp/src/lib.rs` (add `pub(crate) mod config;`)
- `packages/xrce/xrce-smoltcp/src/config.rs` (new)
- `packages/xrce/xrce-smoltcp/src/lib.rs` (add `pub(crate) mod config;`)
- `packages/core/nros-c/src/config.rs` (new)
- `packages/core/nros-c/src/lib.rs` (add `pub(crate) mod config;`)
- `packages/core/nros-params/src/config.rs` (new)
- `packages/core/nros-params/src/lib.rs` (add `pub(crate) mod config;`)

---

## Part F — Documentation

### 45.16 — Documentation

Update CLAUDE.md env var table with all new variables. Group by prefix:
`XRCE_*`, `ZPICO_*`, `NROS_*`.

**Files modified:**
- `CLAUDE.md`

---

## Env Var Summary

Complete list of new env vars introduced by this phase:

### XRCE-DDS

| Env Var                                | Default | Crate         | Step |
|----------------------------------------|---------|---------------|------|
| `XRCE_MAX_SUBSCRIBERS`                 | `8`     | nros-rmw-xrce | 45.1 |
| `XRCE_MAX_SERVICE_SERVERS`             | `4`     | nros-rmw-xrce | 45.1 |
| `XRCE_MAX_SERVICE_CLIENTS`             | `4`     | nros-rmw-xrce | 45.1 |
| `XRCE_BUFFER_SIZE`                     | `1024`  | nros-rmw-xrce | 45.1 |
| `XRCE_STREAM_HISTORY`                  | `4`     | nros-rmw-xrce | 45.1 |
| `XRCE_ENTITY_CREATION_TIMEOUT_MS`      | `1000`  | nros-rmw-xrce | 45.2 |
| `XRCE_SERVICE_REPLY_TIMEOUT_MS`        | `1000`  | nros-rmw-xrce | 45.2 |
| `XRCE_SERVICE_REPLY_RETRIES`           | `5`     | nros-rmw-xrce | 45.2 |
| `XRCE_MAX_SESSION_CONNECTION_ATTEMPTS` | `10`    | xrce-sys      | 45.3 |
| `XRCE_MIN_SESSION_CONNECTION_INTERVAL` | `25`    | xrce-sys      | 45.3 |
| `XRCE_MIN_HEARTBEAT_TIME_INTERVAL`     | `100`   | xrce-sys      | 45.3 |
| `XRCE_UDP_META_COUNT`                  | `4`     | xrce-smoltcp  | 45.4 |

### Zenoh-Pico

| Env Var                            | Default | Crate         | Step |
|------------------------------------|---------|---------------|------|
| `ZPICO_GET_REPLY_BUF_SIZE`         | `4096`  | zpico-sys     | 45.7 |
| `ZPICO_GET_POLL_INTERVAL_MS`       | `10`    | zpico-sys     | 45.7 |
| `ZPICO_SMOLTCP_MAX_SOCKETS`        | `4`     | zpico-smoltcp | 45.8 |
| `ZPICO_SMOLTCP_BUFFER_SIZE`        | `2048`  | zpico-smoltcp | 45.8 |
| `ZPICO_SMOLTCP_CONNECT_TIMEOUT_MS` | `30000` | zpico-smoltcp | 45.8 |
| `ZPICO_SMOLTCP_SOCKET_TIMEOUT_MS`  | `10000` | zpico-smoltcp | 45.8 |

### Core

| Env Var                     | Default | Crate       | Step  |
|-----------------------------|---------|-------------|-------|
| `NROS_EXECUTOR_MAX_HANDLES` | `16`    | nros-c      | 45.11 |
| `NROS_MAX_SUBSCRIPTIONS`    | `8`     | nros-c      | 45.11 |
| `NROS_MAX_TIMERS`           | `8`     | nros-c      | 45.11 |
| `NROS_MAX_SERVICES`         | `4`     | nros-c      | 45.11 |
| `NROS_LET_BUFFER_SIZE`      | `512`   | nros-c      | 45.11 |
| `NROS_MESSAGE_BUFFER_SIZE`  | `4096`  | nros-c      | 45.11 |
| `NROS_MAX_CONCURRENT_GOALS` | `4`     | nros-c      | 45.11 |
| `NROS_MAX_PARAMETERS`       | `32`    | nros-params | 45.13 |
| `NROS_MAX_PARAM_NAME_LEN`   | `64`    | nros-params | 45.13 |
| `NROS_MAX_STRING_VALUE_LEN` | `256`   | nros-params | 45.13 |
| `NROS_MAX_ARRAY_LEN`        | `32`    | nros-params | 45.13 |
| `NROS_MAX_BYTE_ARRAY_LEN`   | `256`   | nros-params | 45.13 |

## Not Made Configurable (Rationale)

The following were reviewed and intentionally left as-is:

**nros-node defaults** — `DEFAULT_RX_BUFFER_SIZE` (1024), `DEFAULT_TX_BUFFER_SIZE` (1024),
`DEFAULT_MAX_SUBSCRIPTIONS` (8), `DEFAULT_MAX_SERVICES` (4), `DEFAULT_MAX_NODES` (4),
`DEFAULT_MAX_TOKENS` (16), `DEFAULT_MAX_TIMERS` (8), `DEFAULT_MAX_ACTIVE_GOALS` (8),
`PARAM_SERVICE_BUFFER_SIZE` (4096), `MAX_PARAMS_PER_REQUEST` (64). These are all const
generic defaults — users already tune them via type parameters at the call site
(e.g., `ConnectedNode<S, 32, 16>`). Adding env vars would create a confusing
dual-configuration path.

**nros-c string buffer limits** — `MAX_LOCATOR_LEN` (128), `MAX_NAME_LEN` (64),
`MAX_NAMESPACE_LEN` (128), `MAX_TOPIC_LEN` (256), `MAX_SERVICE_NAME_LEN` (256),
`MAX_TYPE_NAME_LEN` (256), `MAX_TYPE_HASH_LEN` (128), `MAX_ACTION_NAME_LEN` (256).
These are bounded by ROS 2 naming conventions and rarely need tuning.

**zenoh_generic_config.h** — `Z_CONFIG_SOCKET_TIMEOUT` (100), `Z_TRANSPORT_LEASE` (10000),
`Z_TRANSPORT_LEASE_EXPIRE_FACTOR` (3), `ZP_PERIODIC_SCHEDULER_MAX_TASKS` (8). These are
zenoh-pico protocol/scheduler constants managed upstream; overriding them risks protocol
incompatibility.

**nros-serdes** — All constants are CDR protocol-defined (alignment 2/4/8, header format).

**nros-core** — Protocol enums (lifecycle states, action status, error codes) and
physical constants (`NANOS_PER_SEC`).

## Verification

```bash
# Build XRCE with defaults and custom values
cargo build -p nros-rmw-xrce --features posix-udp
XRCE_MAX_SUBSCRIBERS=16 XRCE_BUFFER_SIZE=4096 cargo build -p nros-rmw-xrce --features posix-udp
XRCE_STREAM_HISTORY=1 cargo build -p nros-rmw-xrce --features posix-udp  # must fail

# Build zenoh with defaults and custom values
cargo build -p zpico-sys --features platform-posix
ZPICO_GET_REPLY_BUF_SIZE=8192 cargo build -p zpico-sys --features platform-posix
cargo build -p zpico-smoltcp
ZPICO_SMOLTCP_MAX_SOCKETS=2 ZPICO_SMOLTCP_BUFFER_SIZE=1024 cargo build -p zpico-smoltcp

# Build core with defaults and custom values
cargo build -p nros-c --features rmw-zenoh,platform-posix,ros-humble
NROS_EXECUTOR_MAX_HANDLES=8 NROS_MESSAGE_BUFFER_SIZE=2048 cargo build -p nros-c --features rmw-zenoh,platform-posix,ros-humble
cargo build -p nros-params
NROS_MAX_PARAMETERS=16 cargo build -p nros-params

# Integration tests
cargo nextest run -p nros-tests -E 'binary(xrce)'
cargo nextest run -p nros-tests

# Full quality gate
just quality
```

## Dependencies

- None. All changes are additive (new env vars with existing defaults).
- Steps 45.1 and 45.2 share a build.rs and should be done together.
- Step 45.6 (ZENOH_SHIM_ → ZPICO_ rename) must be done before 45.7 and 45.9,
  since those steps reference the new `ZPICO_*` constant names.
- Step 45.10 (NANO_ROS_ → NROS_ rename) must be done before 45.11, since
  that step makes the renamed constants env-configurable.
- Steps 45.5, 45.9, 45.12 (named constants) are independent of env var work.
- Part A (XRCE), Part B (zenoh), and Part C (core) are independent and can be
  done in any order (except rename dependencies above).
- Step 45.15 (config.rs migration) should be done alongside or immediately after
  the env var steps for each crate. For existing crates (zpico-sys, nros-rmw-zenoh),
  it moves existing `include!()` calls. For new crates, the `include!()` should
  go into config.rs from the start.
- Step 45.16 (docs) should be done last after all env vars are finalized.

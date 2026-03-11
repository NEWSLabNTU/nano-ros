# Phase 66: C++ API (`nros-cpp`)

**Goal**: Provide a freestanding C++ API that mirrors rclcpp naming conventions, wrapping Rust `nros-node` directly via typed `extern "C"` FFI. Includes CMake-based message codegen for embedded C++ targets.

**Status**: Complete
**Priority**: Medium
**Depends on**: Phase 49 (nros-c thin wrapper migration — complete), Phase 51 (Board crate `run()` API)

## Overview

The C++ API enables rclcpp-familiar development on embedded targets (Zephyr, FreeRTOS, NuttX, ThreadX, bare-metal) without the C++ standard library. Unlike a typical approach of wrapping the C API, nros-cpp wraps the Rust core **directly** through a dedicated `nros-cpp-ffi` Rust crate. This preserves strong type safety — each message type gets its own FFI function, preventing type confusion at the FFI boundary.

See [docs/design/cpp-api-design.md](../design/cpp-api-design.md) for the full design rationale, API surface, and architecture diagrams.

### Key Design Decisions

1. **Direct Rust FFI** (not C API wrapper): A new `nros-cpp-ffi` Rust crate exports type-specific `extern "C"` functions per message type. C++ templates dispatch to these, preserving type safety across the FFI boundary.

2. **Freestanding C++**: `-ffreestanding -fno-exceptions -fno-rtti`. No STL, no `std::function`, no `std::string`. Function pointers + `void* context` for callbacks. Optional `NROS_CPP_STD` mode for hosted environments.

3. **Result-based error handling**: `nros::Result` + `NROS_TRY` macro. Exceptions are not available on most RTOS/bare-metal targets. Optional `NROS_CPP_EXCEPTIONS` mode behind `#ifdef`.

4. **ROS 2 standard namespaces**: Generated types use `std_msgs::msg::Int32`, `example_interfaces::srv::AddTwoInts` — matching rclcpp conventions for migration compatibility.

5. **rclcpp-style Node/Executor pattern**: Entities created via `Node`, then `Executor::add_node()` for spinning.

6. **CMake message codegen**: Extends `nano_ros_generate_interfaces()` with `LANGUAGE CPP`, generating C++ headers + Rust FFI glue. Same pipeline as existing C codegen.

## Work Items

- [x] 66.1 — `nros-cpp-ffi` Rust crate (core FFI exports)
- [x] 66.2 — C++ header-only library (core types)
- [x] 66.3 — Publisher and Subscription
- [x] 66.4 — C++ message codegen (`generate-cpp`)
- [x] 66.5 — CMake integration (`LANGUAGE CPP`)
- [x] 66.6 — Service Server and Client
- [x] 66.7 — Action Server and Client
- [x] 66.8 — Timer and GuardCondition
- [x] 66.9 — Executor and Node-centric pattern
- [x] 66.10 — Examples (Linux native)
- [x] 66.11 — Examples (embedded targets)
- [x] 66.12 — Integration tests
- [x] 66.13 — Optional `std` mode conveniences
- [x] 66.14 — Documentation

### 66.1 — `nros-cpp-ffi` Rust crate (core FFI exports)

Create a new Rust crate that exports `extern "C"` functions for C++ consumption. This is the typed FFI boundary — not the existing `nros-c` API.

**Scope**:
- `#[repr(C)]` structs for opaque handles (executor, node)
- `nros_cpp_init()` / `nros_cpp_fini()` — wraps `Executor::open()` / `Executor::close()`
- `nros_cpp_node_create()` / `nros_cpp_node_destroy()` — wraps `Executor::create_node()`
- cbindgen `build.rs` to generate `nros_cpp_ffi.h`
- Error codes as `i32` return values

**Files**:
- `packages/core/nros-cpp-ffi/Cargo.toml`
- `packages/core/nros-cpp-ffi/src/lib.rs`
- `packages/core/nros-cpp-ffi/build.rs` (cbindgen)
- `packages/core/nros-cpp-ffi/cbindgen.toml`

### 66.2 — C++ header-only library (core types)

Create the header-only C++ library with foundational types.

**Scope**:
- `nros::Result` with `ErrorCode` enum and `NROS_TRY` macro
- `nros::QoS` with chainable setters and predefined profiles
- `nros::Node` class (create, destroy, get_name, get_namespace)
- `nros.hpp` umbrella header
- CMake target: `NanoRos::NanoRosCpp` (header-only, depends on FFI static lib)

**Files**:
- `packages/core/nros-cpp/include/nros/result.hpp`
- `packages/core/nros-cpp/include/nros/qos.hpp`
- `packages/core/nros-cpp/include/nros/node.hpp`
- `packages/core/nros-cpp/include/nros/nros.hpp`
- `packages/core/nros-cpp/CMakeLists.txt`

### 66.3 — Publisher and Subscription

Add pub/sub support to both the FFI crate and the C++ headers.

**Scope**:
- `nros::Publisher<M>` — `publish()` calls type-specific FFI function
- `nros::Subscription<M>` — callback with `const M&` + `void* context`
- FFI: `nros_cpp_publisher_create()`, `nros_cpp_subscription_create()`
- Publisher/Subscription own opaque `void* rust_handle_` pointers
- RAII: destructor calls FFI destroy function
- Move semantics (non-copyable)

**Files**:
- `packages/core/nros-cpp-ffi/src/publisher.rs`
- `packages/core/nros-cpp-ffi/src/subscription.rs`
- `packages/core/nros-cpp/include/nros/publisher.hpp`
- `packages/core/nros-cpp/include/nros/subscription.hpp`

### 66.4 — C++ message codegen (`generate-cpp`)

Extend the codegen tool to generate C++ message bindings with typed publish/subscribe.

**Architecture**:

```
.msg file ──→ cargo nano-ros generate-cpp ──→ ┌── message.hpp (C++ header)
                                               └── message_ffi.rs (Rust FFI glue)
```

- **C++ header** (`message.hpp`): `#[repr(C)]`-compatible struct in ROS 2 namespace with
  `TYPE_NAME`, `TYPE_HASH`, `SERIALIZED_SIZE_MAX`, `ffi_publish()`, `ffi_deserialize()`.
  Uses `FixedString<N>` and `FixedSequence<T,N>` wrapper types for ergonomic access.

- **Rust FFI glue** (`message_ffi.rs`): `#[repr(C)]` struct mirroring C++ layout + thin
  `extern "C"` functions that use `CdrWriter`/`CdrReader` for serialization. Each field
  maps to a single `writer.write_<type>()` call.

**Serialization flow**:
```
Publish:  C++ struct → void* FFI → Rust #[repr(C)] cast → per-field CdrWriter → raw CDR → nros_cpp_publish_raw()
Receive:  CDR bytes → CdrReader per-field reads → fill #[repr(C)] struct → void* FFI → C++ struct
```

**Convenience types** (freestanding C++14, header-only, no STL):
- `nros::FixedString<N>` — Fixed-capacity null-terminated string (wraps `char[N]`).
  Provides `operator=(const char*)`, `c_str()`, `length()`, `operator==`. repr(C) identical to `char[N]`.
- `nros::FixedSequence<T,N>` — Fixed-capacity sequence (wraps `uint32_t size; T data[N]`).
  Provides `push_back()`, `operator[]`, `begin()`/`end()`. repr(C) identical to `{u32, T[N]}`.

**Type mapping**:

| ROS IDL type | C++ field | Rust `#[repr(C)]` | CDR call |
|---|---|---|---|
| `int32` | `int32_t` | `i32` | `write_i32` |
| `float64` | `double` | `f64` | `write_f64` |
| `bool` | `bool` | `bool` | `write_bool` |
| `string` | `nros::FixedString<256>` | `[u8; 256]` | `write_string` |
| `string<=64` | `nros::FixedString<64>` | `[u8; 64]` | `write_string` |
| `int32[3]` | `int32_t v[3]` | `[i32; 3]` | loop × `write_i32` |
| `int32[]` | `nros::FixedSequence<int32_t, 64>` | `_seq_t { size: u32, data: [i32; 64] }` | `write_u32` + loop |
| `pkg/Msg` | `pkg_msg_Name` | `pkg_msg_name_t` | call helper fn |

**Scope**:
- `cargo nano-ros generate-cpp` command (reuses same JSON args format as `generate-c`)
- C++14 compatible (`namespace a { namespace b { } }` syntax)
- `static constexpr` type metadata: `TYPE_NAME`, `TYPE_HASH`, `SERIALIZED_SIZE_MAX`
- Messages, services (Request/Response nested structs), and actions (Goal/Result/Feedback)
- Typed `Subscription<M>::try_recv(M& msg)` using `M::ffi_deserialize()`

**Files**:
- `packages/core/nros-cpp/include/nros/fixed_string.hpp` — `FixedString<N>` template
- `packages/core/nros-cpp/include/nros/fixed_sequence.hpp` — `FixedSequence<T,N>` template
- `packages/codegen/packages/rosidl-codegen/templates/message_cpp.hpp.jinja`
- `packages/codegen/packages/rosidl-codegen/templates/message_cpp_ffi.rs.jinja`
- `packages/codegen/packages/rosidl-codegen/templates/service_cpp.hpp.jinja`
- `packages/codegen/packages/rosidl-codegen/templates/action_cpp.hpp.jinja`
- `packages/codegen/packages/rosidl-codegen/src/types.rs` (C++ type mapping)
- `packages/codegen/packages/rosidl-codegen/src/templates.rs` (CppFfiField, template structs)
- `packages/codegen/packages/rosidl-codegen/src/generator/common.rs` (build_cpp_ffi_field)
- `packages/codegen/packages/rosidl-codegen/src/generator/cpp.rs` (C++ generator)
- `packages/codegen/packages/cargo-nano-ros/src/main.rs` (GenerateCpp subcommand)
- `packages/codegen/packages/cargo-nano-ros/src/lib.rs` (generate_cpp_from_args_file)
- `packages/core/nros-cpp/include/nros/subscription.hpp` (typed try_recv)

### 66.5 — CMake integration (`LANGUAGE CPP`)

Extend `nano_ros_generate_interfaces()` to support C++ output.

**Scope**:
- `LANGUAGE CPP` option in `nano_ros_generate_interfaces()`
- Resolves `.msg`/`.srv`/`.action` files (local -> ament -> bundled)
- Runs `nros-codegen --language cpp` to produce `.hpp` + `.rs` FFI glue
- Compiles Rust FFI glue into a static library via Corrosion
- Creates `<target>__nano_ros_cpp` CMake target with include path + FFI lib link
- Parallel to existing `LANGUAGE C` pipeline

**Files**:
- `packages/codegen/packages/nros-codegen-c/cmake/NanoRosGenerateInterfaces.cmake` (extend)
- `packages/codegen/packages/nros-codegen-c/cmake/NanoRosCppFfi.cmake` (new — Corrosion glue)

### 66.6 — Service Server and Client

Add service support.

**Scope**:
- `nros::Service<S>` — server with typed request/response callback
- `nros::Client<S>` — blocking `call()` with typed request/response
- FFI: `nros_cpp_service_create()`, `nros_cpp_client_create()`, `nros_cpp_client_call()`
- Callback: `bool handler(const S::Request&, S::Response&, void*)`

**Files**:
- `packages/core/nros-cpp-ffi/src/service.rs`
- `packages/core/nros-cpp-ffi/src/client.rs`
- `packages/core/nros-cpp/include/nros/service.hpp`
- `packages/core/nros-cpp/include/nros/client.hpp`

### 66.7 — Action Server and Client

Add action support.

**Scope**:
- `nros::ActionServer<A>` with `Callbacks` struct (on_goal, on_cancel, on_accepted)
- `nros::GoalHandle<A>` — publish_feedback, succeed, abort, canceled
- `nros::ActionClient<A>` — send_goal, cancel_goal, get_result, feedback/result callbacks
- FFI: type-specific action functions per action type

**Files**:
- `packages/core/nros-cpp-ffi/src/action.rs`
- `packages/core/nros-cpp/include/nros/action_server.hpp`
- `packages/core/nros-cpp/include/nros/action_client.hpp`

### 66.8 — Timer and GuardCondition

Add timer and guard condition support.

**Scope**:
- `nros::Timer` — period_ns callback, cancel, reset
- `nros::GuardCondition` — trigger, is_triggered, clear, set_callback
- FFI: `nros_cpp_timer_create()`, `nros_cpp_guard_condition_create()`

**Files**:
- `packages/core/nros-cpp-ffi/src/timer.rs`
- `packages/core/nros-cpp-ffi/src/guard_condition.rs`
- `packages/core/nros-cpp/include/nros/timer.hpp`
- `packages/core/nros-cpp/include/nros/guard_condition.hpp`

### 66.9 — Executor and Node-centric pattern

Add executor with rclcpp-style `add_node()` pattern.

**Scope**:
- `nros::Executor` — create, add_node, spin, spin_some, spin_once, spin_period, stop
- Free functions: `nros::spin()`, `nros::spin_some()`
- FFI: `nros_cpp_executor_create()`, `nros_cpp_executor_add_node()`, `nros_cpp_executor_spin()`
- Internally maps `add_node()` to registering all node entities with the Rust executor

**Files**:
- `packages/core/nros-cpp-ffi/src/executor.rs`
- `packages/core/nros-cpp/include/nros/executor.hpp`

### 66.10 — Examples (Linux native)

Create C++ examples that build and run on Linux (native, no RTOS).

**Scope**:
- `examples/native/cpp/zenoh/talker/` — Publish `std_msgs::msg::Int32` on `/chatter`
- `examples/native/cpp/zenoh/listener/` — Subscribe to `/chatter`
- `examples/native/cpp/zenoh/service-server/` — `AddTwoInts` server
- `examples/native/cpp/zenoh/service-client/` — `AddTwoInts` client
- Each with `CMakeLists.txt`, `package.xml`, `src/main.cpp`
- Uses `nano_ros_generate_interfaces(... LANGUAGE CPP)`

**Files**: `examples/native/cpp/zenoh/{talker,listener,service-server,service-client}/`

### 66.11 — Examples (embedded targets)

Port C++ examples to Zephyr RTOS (native_sim). Zephyr handles `-fno-exceptions -fno-rtti`
automatically via `CONFIG_CPLUSPLUS=y`.

**Scope**:
- Target: Zephyr RTOS on native_sim/native/64
- `CONFIG_NROS_CPP_API=y` Kconfig choice (alongside C and Rust API)
- `nros_generate_interfaces(... LANGUAGE CPP)` in Zephyr CMake (with auto-discovery)
- Generalized `nros_cargo_build()` target naming (works for nros-c and nros-cpp-ffi)
- Talker + listener examples using nros C++ API

**Files**: `examples/zephyr/cpp/zenoh/{talker,listener}/`, `zephyr/Kconfig`,
`zephyr/CMakeLists.txt`, `zephyr/cmake/nros_cargo_build.cmake`,
`zephyr/cmake/nros_generate_interfaces.cmake`

### 66.12 — Integration tests

Automated tests for C++ API — build, startup, E2E communication, cross-language interop.

**Scope**:
- Build tests: verify all 4 native C++ examples compile
- Startup tests: talker, listener, service server initialize
- E2E tests: talker/listener exchange messages, service server/client complete RPC
- Cross-language interop: C++ talker → Rust listener, C++ service server → Rust client
- Zephyr C++ E2E tests: talker-listener, cross-platform (Zephyr ↔ native Rust)
- `just test-cpp` recipe, nextest `cpp_api` test group
- Bug fix: `node.hpp` passed `S::Request::TYPE_NAME` instead of `S::TYPE_NAME` for services/actions

**Files**:
- `packages/testing/nros-tests/tests/cpp_api.rs`
- `packages/testing/nros-tests/src/fixtures/binaries.rs` (C++ builders)
- `packages/testing/nros-tests/src/zephyr.rs` (C++ Zephyr entries + tests)
- `.config/nextest.toml` (add `cpp_api` group)
- `justfile` (add `test-cpp` recipe)
- `packages/core/nros-cpp/include/nros/node.hpp` (fix `S::TYPE_NAME` for services/actions)

### 66.13 — Optional `std` mode conveniences

Add `#ifdef NROS_CPP_STD` overloads for hosted environments.

**Scope**:
- `std::function`-based callback overloads for subscriptions, services, timers
- `std::string` overloads for topic/service names
- `std::chrono` duration overloads for timers
- All behind `#ifdef NROS_CPP_STD` (never required)

**Files**:
- `packages/core/nros-cpp/include/nros/std_compat.hpp`

### 66.14 — Documentation

**Scope**:
- Update `CLAUDE.md`: add `nros-cpp`, `nros-cpp-ffi` to workspace structure, update phase table
- Update `docs/reference/environment-variables.md` if any new env vars
- Update `docs/guides/creating-examples.md` with C++ example instructions
- Add `docs/guides/cpp-api.md` — getting started with C++ API
- Doxygen comments in all public headers

**Files**:
- `CLAUDE.md`
- `docs/guides/cpp-api.md`
- `docs/guides/creating-examples.md`

## Acceptance Criteria

- [ ] `nros-cpp-ffi` crate compiles and exports `extern "C"` functions
- [ ] C++ header-only library compiles with `-ffreestanding -fno-exceptions -fno-rtti`
- [ ] `nros::Result` + `NROS_TRY` macro works for error propagation
- [ ] `nros::Publisher<M>::publish()` sends messages via typed FFI (no type erasure)
- [ ] `nros::Subscription<M>` receives deserialized messages in typed callback
- [ ] `nros::Service<S>` and `nros::Client<S>` complete request/response cycle
- [ ] `nros::ActionServer<A>` and `nros::ActionClient<A>` complete goal lifecycle
- [ ] `nros::Timer` fires periodic callbacks
- [ ] `nros::Executor::add_node()` + `spin()` processes all registered entities
- [ ] `cargo nano-ros generate-cpp` produces valid `.hpp` headers in ROS 2 namespaces
- [ ] `nano_ros_generate_interfaces(... LANGUAGE CPP)` works in CMake
- [ ] Generated types use `namespace std_msgs { namespace msg { struct Int32 { ... }; } }` (C++14)
- [ ] Linux native C++ talker/listener exchange messages over zenohd
- [ ] Linux native C++ service server/client complete RPC cycle
- [ ] At least one embedded target compiles and runs C++ examples
- [ ] `just test-cpp` passes
- [ ] `just quality` passes with C++ crates in workspace
- [ ] No STL dependency in freestanding mode (verified by `-ffreestanding` build)

## Notes

- **C++ standard**: Target C++14 minimum. Avoid C++17 features (nested namespaces `a::b::c`, `if constexpr`, structured bindings) for maximum toolchain compatibility on embedded.
- **cbindgen for FFI header**: The `nros-cpp-ffi` crate uses cbindgen (same as `nros-c`) to generate the FFI header. The C++ headers include this generated header.
- **Sequence fields**: Variable-length sequences in messages are represented as fixed-size arrays with a length field (e.g., `int32_t data[64]; uint32_t data_len;`). Maximum sizes are determined at codegen time from message definitions.
- **No `cxx` crate**: While `cxx` provides safe Rust-C++ interop, it requires C++11 STL (`std::unique_ptr`, `rust::String`) which is unavailable in freestanding mode. Raw `extern "C"` FFI is used instead.
- **Binary size**: The typed FFI approach generates one FFI function per message type. For projects using many message types, this increases binary size. Future optimization: link-time dead code elimination (`-Wl,--gc-sections`) removes unused FFI functions.
- **Parameter API**: `nros::Node` parameter methods (declare, get, has) are deferred to after the core pub/sub/service/action API is stable.

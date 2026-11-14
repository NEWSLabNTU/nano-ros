---
rfc: 0018
title: "C++ API Design — nros-cpp"
status: Stable
area: runtime-api
since: 2026-03
last-reviewed: 2026-11-14
implements-tracked-by: [phase-287, phase-442]
supersedes: []
superseded-by: null
---

> **Review 2026-11-13, ruled 2026-11-14 (phase-555 W3).** This RFC is the
> C++ constraints RFC: freestanding, no exceptions, no heap, and a typed FFI
> that bypasses `nros-c`. Those stand, and
> [RFC-0096](0096-cpp-freestanding-core-and-porting-layer.md) restores them.
> The names are [RFC-0089](0089-ros2-api-adoption-and-the-compile-or-conform-rule.md)'s:
> it deleted the `nros::` C++ namespace, so the user surface is `rclcpp::`,
> `rclcpp_action::` and `rclcpp_lifecycle::` under upstream include paths
> (`packages/api/nros-cpp/include/rclcpp/`). The sections that described the
> `nros::` names were cut; see "Names" below.

# C++ API Design — nros-cpp

## Goal

Provide a C++ API that mirrors rclcpp naming conventions while targeting
embedded systems with no heap allocation. The C++ layer wraps the Rust
`nros-node` core **directly via `extern "C"` FFI**, bypassing the C API
(`nros-c`) to preserve strong type safety through C++ templates.

## Architecture

```
┌─────────────────────────────────────────────────────────────┐
│  User code (C++)                                            │
│  rclcpp::Node, rclcpp::Publisher<M>, rclcpp::Subscription<M>│
├─────────────────────────────────────────────────────────────┤
│  nros-cpp  (header-only, freestanding C++)                  │
│  Thin wrappers: type safety, RAII, templates                │
│  Generated message types: std_msgs::msg::Int32, etc.        │
├─────────────────────────────────────────────────────────────┤
│  nros-cpp FFI  (Rust staticlib, extern "C" exports)          │
│  Typed FFI surface for C++ — not the C API                  │
├─────────────────────────────────────────────────────────────┤
│  nros-node (Rust core)                                      │
│  Executor<S, MAX_CBS, CB_ARENA>, Node, Publisher, etc.      │
└─────────────────────────────────────────────────────────────┘
```

### Why bypass the C API?

The `nros-c` API erases all type information into opaque handles
(`rcl_publisher_t`, `rcl_subscription_t`; `nros_publisher_t` and
`nros_subscription_t` when this was written). Wrapping these in C++ templates
recovers type safety but cannot propagate it through the FFI boundary —
a `Publisher<Int32>` and a `Publisher<String>` both hold the same
`rcl_publisher_t` handle, and nothing prevents mixing them at the FFI
level.

By having a dedicated Rust FFI layer in `nros-cpp`, we can:

1. **Carry type info through FFI**: The Rust side knows the concrete
   message type at compile time and can generate type-specific FFI
   functions (e.g., `nros_cpp_publish_std_msgs_Int32()`).
2. **Inline CDR serialization in Rust**: The C++ side passes a plain
   struct; Rust serializes it via the already-generated `RosMessage`
   impl. No duplicate serialization code in C++.
3. **Validate at the FFI boundary**: Rust can validate buffer sizes,
   message invariants, and session state before touching the transport.

### nros-cpp FFI Layer

The Rust FFI staticlib in `packages/api/nros-cpp/` that:

- Depends on `nros-node` and message crates (e.g., `nros-std-msgs`)
- Exports `extern "C"` functions for each message/service/action type
- Uses cbindgen to generate a C++ header (`nros_cpp_ffi.h`)
- Statically links into the final binary alongside `nros-node`

Example FFI surface:

```rust
// packages/api/nros-cpp/src/lib.rs (generated per message type)

#[repr(C)]
pub struct StdMsgsInt32 {
    pub data: i32,
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_cpp_publish_std_msgs_Int32(
    handle: *mut c_void,
    msg: *const StdMsgsInt32,
) -> i32 {
    // Deserialize C struct → Rust RosMessage, call publisher.publish()
}
```

The codegen tool generates both the C++ header struct and the Rust FFI
function for each message type, ensuring they stay in sync.

## Constraints

### Freestanding C++ (`-ffreestanding`)

The C++ API must work without the C++ standard library. This means:

- **No STL containers**: no `std::string`, `std::vector`, `std::map`,
  `std::shared_ptr`, `std::function`
- **No exceptions**: compile with `-fno-exceptions` (see Error Handling)
- **No RTTI**: compile with `-fno-rtti`
- **No `<iostream>`**: no `std::cout`
- **Available**: `<cstdint>`, `<cstddef>`, `<cstring>`, `<type_traits>`,
  templates, `constexpr`, placement new, move semantics

This is the C++ equivalent of Rust's `no_std`. The same binary runs on
bare-metal Cortex-M, Zephyr, FreeRTOS, NuttX, ThreadX, and Linux.

### Opt-in `std` surface

When `NROS_CPP_STD` is defined, the API exposes convenience overloads that
accept `std::string`, `std::function`, etc., plus the `rclcpp::Node` shape whose
factories return `std::shared_ptr`. These are `#ifdef`-guarded and never
required.

**It is REQUESTED by the build, never detected.** This paragraph read "or
detected via `__STDC_HOSTED__`" until phase-438, and the headers did carry such
a probe — latterly `__has_include`, which had re-broadened the gate issue 0112
narrowed. Both were deleted, for a measured reason: no compile-time probe
answers correctly on both embedded lanes. `__STDC_HOSTED__` is 1 for a hosted
compiler run `-nostdinc++` against Zephyr's minimal libcpp (0112's own finding),
and `__has_include(<string>)` is TRUE on arm-none-eabi 13.2 under
`-ffreestanding`, where libstdc++ 13's `bits/requires_hosted.h` makes the
include a hard `#error` — which broke every embedded C++ FreeRTOS build
(issue 1187). Only the request is right on both, because it is not a probe.

The consequence for the design is that the split is **ported rclcpp code versus
code written for nano-ros**, not hosted versus embedded: the std surface exists
to let an upstream ROS 2 source file compile unmodified (RFC-0089's
compile-or-conform rule), so a native host build of a nano-ros-native node stays
on the freestanding surface. Detection sits in one place,
`packages/api/nros-cpp/include/rclcpp/detail/std_detect.hpp`, which carries the
measurement.

## Error Handling

### No exceptions on embedded

C++ exceptions are **not available** on most RTOS and bare-metal targets:

- **Zephyr**: Compiles with `-fno-exceptions` by default. Enabling
  exceptions requires `CONFIG_EXCEPTIONS=y` + `CONFIG_CPP_EXCEPTIONS=y`,
  but not all architectures support it, and it adds significant code size.
- **FreeRTOS**: No built-in exception support. Toolchains for Cortex-M
  typically disable exceptions (`-fno-exceptions`).
- **NuttX**: Supports exceptions on some architectures, but not guaranteed.
- **Bare-metal**: Exception unwinding requires `libunwind` or equivalent,
  plus `.eh_frame` sections — often unavailable or too costly.

### Result-based error handling

Every fallible operation returns a value, never throws. The channel is
`rclcpp::Result` for an operation that produces nothing and
`rclcpp::ResultOf<T>` for one that produces a value; where upstream rclcpp
throws, ours returns one of these. The rule and the names are RFC-0089's
(section "The error channel, settled"); this RFC owns only the constraint
that makes it necessary: no exceptions on embedded.

No exception mode is provided. An earlier draft proposed an opt-in
`NROS_CPP_EXCEPTIONS` that would throw on error; it was never built.

## Names

This RFC records the constraints and the FFI shape. The user-facing names
are upstream's: `rclcpp::`, `rclcpp_action::` and `rclcpp_lifecycle::` under
upstream include paths, defined in `packages/api/nros-cpp/include/`. Which
names are adopted, how a bounded or refused upstream name is disposed of, and
the node and timer design are
[RFC-0089](0089-ros2-api-adoption-and-the-compile-or-conform-rule.md)'s. The
measured surface is generated and gated: `docs/reference/api-surface/` and the
per-row `docs/reference/api-parity-ledger/` (`just check api-parity`).

The sections this RFC once had for the names ("API Mapping", "Detailed API
Design", "Usage Example", "Callback Pattern Comparison") described an
`nros::` namespace that RFC-0089 deleted, and were cut on 2026-11-14
(phase-555 W3). Git history keeps them.

## Generated Message Types

### ROS 2 namespace convention

Generated C++ types use standard ROS 2 namespaces:
`<package_name>::<interface_type>::<TypeName>`. This is the convention
used by rclcpp and all standard ROS 2 tooling:

- Messages: `std_msgs::msg::Int32`, `geometry_msgs::msg::Twist`
- Services: `example_interfaces::srv::AddTwoInts`
- Actions: `example_interfaces::action::Fibonacci`

This ensures maximum compatibility with existing ROS 2 C++ code. Users
migrating from rclcpp can keep the same `#include` paths and type names.

### Generated structure

The codegen tool produces two artifacts per message type:

1. **C++ header** (`.hpp`): `#[repr(C)]`-compatible struct + type traits
2. **Rust FFI glue** (`.rs`): Conversion between C++ struct and Rust
   `RosMessage` type + `extern "C"` functions

```cpp
// Generated: std_msgs/msg/int32.hpp
#pragma once

#include <cstdint>
#include <cstddef>

namespace std_msgs {
namespace msg {

struct Int32 {
    int32_t data;

    // Type metadata (used by Publisher/Subscription templates)
    static constexpr const char* TYPE_NAME =
        "std_msgs::msg::dds_::Int32_";
    static constexpr const char* TYPE_HASH =
        "TypeHashNotSupported";
    static constexpr size_t SERIALIZED_SIZE_MAX = 8;
};

} // namespace msg
} // namespace std_msgs
```

Service types:
```cpp
// Generated: example_interfaces/srv/add_two_ints.hpp
namespace example_interfaces {
namespace srv {

struct AddTwoInts {
    struct Request {
        int64_t a;
        int64_t b;
        static constexpr size_t SERIALIZED_SIZE_MAX = 20;
    };

    struct Response {
        int64_t sum;
        static constexpr size_t SERIALIZED_SIZE_MAX = 12;
    };

    static constexpr const char* SERVICE_NAME =
        "example_interfaces::srv::dds_::AddTwoInts_";
};

} // namespace srv
} // namespace example_interfaces
```

Action types:
```cpp
// Generated: example_interfaces/action/fibonacci.hpp
namespace example_interfaces {
namespace action {

struct Fibonacci {
    struct Goal {
        int32_t order;
        static constexpr size_t SERIALIZED_SIZE_MAX = 8;
    };

    struct Result {
        // Fixed-size array for no_std; size known at codegen time
        int32_t sequence[64];
        uint32_t sequence_len;
        static constexpr size_t SERIALIZED_SIZE_MAX = 264;
    };

    struct Feedback {
        int32_t partial_sequence[64];
        uint32_t partial_sequence_len;
        static constexpr size_t SERIALIZED_SIZE_MAX = 264;
    };
};

} // namespace action
} // namespace example_interfaces
```

**Note:** Nested `namespace a::b` syntax (C++17) is avoided for C++14
compatibility; nested `namespace a { namespace b { ... } }` is used
instead.

### CMake code generation

Message bindings are generated via CMake, using the same pipeline as
the existing C code generation. The `nros_find_interfaces()` function
takes a `LANGUAGE CPP` option and reads dependent packages from
`package.xml` `<depend>` rows (no explicit PACKAGES arg — `package.xml`
is the SSoT):

```cmake
# In the example's CMakeLists.txt:
nros_find_interfaces(LANGUAGE CPP)  # reads <depend> rows from package.xml

# Generates one `<pkg>__nano_ros_cpp` target per resolved <depend> row.
# Includes: ${CMAKE_CURRENT_BINARY_DIR}/generated/cpp/
# Links:    Rust FFI static library with type-specific functions
```

The CMake function:

1. Resolves `.msg`/`.srv`/`.action` files (local → ament → bundled)
2. Runs `nros-codegen --language cpp` to generate `.hpp` headers and
   `.rs` FFI glue files
3. Compiles the Rust FFI glue into a static library via Corrosion
4. Creates a CMake target that provides the include path and links
   the FFI library

This mirrors the existing `LANGUAGE C` pipeline in
`NanoRosGenerateInterfaces.cmake`, with a parallel `cpp` output
directory.

## Implementation

See [docs/roadmap/phase-66-cpp-api.md](../roadmap/archived/phase-66-cpp-api.md) for
work items, acceptance criteria, and implementation schedule.

## Files

`packages/api/nros-cpp/`: the Rust staticlib (`src/`, with the typed FFI
exports), `build.rs` (cbindgen to `include/rclcpp/detail/nros_cpp_ffi.h`
plus the per-build config header), and the user headers under
`include/rclcpp/`, `include/rclcpp_action/`, `include/rclcpp_lifecycle/` and
`include/rclcpp_components/`.

## References

- Build/link determinism for C/C++ (one canonical `<nros/platform.h>`, board
  `[board.capabilities]` → `-DNROS_PLATFORM_HAS_*`, manifest-driven RMW link):
  [RFC-0042](0042-platform-build-determinism.md) +
  [c-api-cmake.md](../reference/c-api-cmake.md) "Board capabilities & deterministic
  build".
- rclcpp API: https://docs.ros.org/en/rolling/p/rclcpp/
- rclcpp_action API: https://docs.ros.org/en/rolling/p/rclcpp_action/
- Existing nros Rust API: `packages/core/nros-node/src/executor/`
- Existing nros C API: `packages/api/nros-c/`
- C codegen pipeline: `packages/codegen/packages/nros-codegen-c/cmake/NanoRosGenerateInterfaces.cmake`
- C message template: `packages/codegen/packages/rosidl-codegen/templates/message_c.h.jinja`

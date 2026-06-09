---
rfc: 0036
title: "Divergences from the ROS 2 standard client APIs (rclrs / rclcpp / rclc)"
status: Draft
since: 2026-06
last-reviewed: 2026-06
implements-tracked-by: []
supersedes: []
superseded-by: null
---

# RFC-0036 — Divergences from the ROS 2 standard client APIs

## Summary

nano-ros deliberately mirrors the ROS 2 client libraries — Rust ≈ rclrs 0.7.0,
C ≈ rclc, C++ ≈ rclcpp — so a ROS 2 developer can read and write nano-ros code.
But `no_std` / embedded / no-allocator / no-exceptions constraints force a set of
**deliberate divergences**. Today they are scattered across RFC-0018/0021/0022/
0002 and prose notes, with no single reference — and stale notes still label the
Rust error `RclrsError` when it is actually `NanoRosError`. This RFC is the
**one authoritative catalog** of what differs, why, and what a porting user must
adjust. It is a reference, not a new decision: each row points to the RFC that
owns the decision.

## Motivation / problem

- A user evaluating or porting to nano-ros needs one place that answers "how is
  this different from the ROS 2 API I know?" Scattered notes don't serve that.
- The divergences are load-bearing API contracts; an authoritative list keeps
  future RFCs from silently re-diverging or accidentally converging.

## Design

Each divergence: **what ROS 2 does → what nano-ros does → why → owner**.

### Type system

| ROS 2 | nano-ros | why | owner |
|---|---|---|---|
| `std::vector<T>` / `rosidl_runtime_rs::Sequence<T>` (unbounded) | `heapless::Vec<T, N>` (owned), `alloc::Vec<T>` / `nros::HeapSequence<T>` (heap), `&'a [T]` (borrowed) — per-field via `nros-codegen.toml` | no implicit heap on MCU; capacity is a local choice (invisible on CDR wire) | RFC-0033 |
| `std::string` / `rosidl_runtime_rs::String` | `heapless::String<N>` / `nros::HeapString` / `&'a str` | same | RFC-0033 |
| computed type hash (Iron+) | `TYPE_HASH = "TypeHashNotSupported"` (Humble baseline) | Humble predates type hashing; Iron deferred | — |

### Errors

| ROS 2 | nano-ros | why | owner |
|---|---|---|---|
| rclcpp throws `std::exception` subclasses | C++ returns `nros::Result` + `NROS_TRY(expr)` early-return macro | `-fno-exceptions` on Zephyr/FreeRTOS/bare-metal | RFC-0018 |
| rclrs `RclrsError` | Rust `NanoRosError { code: RclReturnCode, context, nested }`; `RclReturnCode` mirrors `rcl_ret_t` numerics as a Rust enum | result-only, `no_std`, C-ABI-compatible codes | `nros-core/src/error.rs` |
| `rcl_ret_t` int | C `nros_ret_t` enum (`NROS_RET_OK=0`, …); RMW layer `nros_rmw_ret_t` (`0 … -18`) | explicit numeric ABI | RFC-0035 (rmw) |

> **Naming note:** older prose still calls the Rust error `RclrsError`. The
> actual type is `NanoRosError` (+ `RclReturnCode`). Treat this RFC as the
> authority; correct any surviving stale reference.

### Domain ID

- ROS 2: `ROS_DOMAIN_ID` read from the environment at runtime, everywhere.
- nano-ros: **compile-time-baked on embedded** (`CONFIG_NROS_DOMAIN_ID` Kconfig /
  per-example `config.toml` → `app_config.h`); **runtime env only on native/host**
  (`nros_tests::unique_ros_domain_id()`). A runtime `ROS_DOMAIN_ID` does **not**
  reach an embedded backend (no libc `getenv` trampoline on e.g. native_sim).
- Why: embedded backends have no runtime env; the domain must be linked in.
  (CLAUDE.md "QEMU Networked Tests".)

### Naming / namespacing

- `nros::` not `rclcpp::`; `nros_*` C fns not `rcl_*`/`rclc_*`; `CONFIG_NROS_*`
  build config. Signals the embedded variant while mirroring the surface.
- Topic key conventions (`rt/`, `rq/`, `rr/`) preserved for rmw_zenoh
  interop; `QosSettings.avoid_ros_namespace_conventions` toggles them.

### Execution & blocking model

| ROS 2 | nano-ros | why | owner |
|---|---|---|---|
| `rclcpp::spin` with multiple executors / spinner threads | **one `Executor` per RTOS task**, shared by all nodes in that tier; FIFO callback dispatch | avoid OS priority-slot starvation; mixed-criticality via priority tiers | RFC-0002, RFC-0015 |
| blocking `client->async_send_request(...).get()` internally drives the loop | non-blocking `Promise<T>`; **every blocking helper takes the executor and drives it** (`promise.wait(&mut executor, timeout)`) | single source of I/O; reentrancy-safe; reliable RTOS timeouts | RFC-0021 |
| action client `wait_for_result()` blocks | spin-driven `Promise` poll/`wait(&mut executor)` | same; no deadlock on single-threaded transports | RFC-0021 |
| `rclcpp::Node` shared via `std::shared_ptr<Node>` (`Arc<Node>` in rclrs) | `&mut Executor` + short-lived `NodeCtx<'_>`; **no `Arc<Node>`**; entities owned, outlive the handle | zero allocation; two live node handles = borrow error by construction | RFC-0022 |

### Reduced / omitted surface

- **No exceptions, no RTTI, no STL** in C++ (`-fno-exceptions -fno-rtti`,
  freestanding). `const char*` not `std::string`; plain fn-ptr + `void* ctx`
  callbacks not `std::function`; value types not `shared_ptr`. (RFC-0018.)
- **QoS subset** — history/reliability/durability/liveliness/deadline/lifespan
  supported; selected at compile time; no dynamic QoS negotiation.
- **No dynamic discovery** — peers static via `nros.toml` / Kconfig locator.
- **No parameter callbacks**; parameters are read/write only.
- **No lifecycle-node graph** — a simplified state model for embedded executors.

## Alternatives considered

- **Keep divergences in per-RFC notes only.** Rejected — no single porting
  reference; stale mislabels (the `RclrsError` name) go uncaught.
- **Aim for byte-for-byte ROS 2 API parity.** Rejected — impossible under
  `no_std`/no-exceptions; the divergences are the point of the project.

## Open questions

1. Should this RFC carry per-language migration snippets (rclcpp→nros side by
   side), or link to a `book/` migration chapter? Proposed: keep the catalog
   here; put runnable side-by-sides in `book/`.
2. Track convergence opportunities (e.g. a hosted-only `std`-backed mode closer
   to rclrs)? Proposed: out of scope; note if it arises.

## Changelog

- 2026-06 — created (Draft). Consolidated the type/error/domain-id/naming/
  execution/omitted-surface divergences from RFC-0018/0021/0022/0002 + code;
  noted the stale `RclrsError` → actual `NanoRosError` naming.

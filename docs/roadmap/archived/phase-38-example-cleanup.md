# Phase 38: Example Cleanup — Eliminate Low-Level Leaks

**Status: Complete**

**Prerequisites:** Phase 36.8 (orthogonal feature axes — complete), Phase 34 (RMW abstraction — complete)

## Goal

Every example in the `examples/` tree should look like end-user code: depend only on `nros` (or a board crate) plus generated message types, use the public Node/Executor API, contain no `unsafe` beyond what bare-metal entry points require, and never touch internal crates (`nros-rmw-*`, `nros-core`, `zpico-*`, `xrce-*`) directly. This phase audits every example category, documents violations, and plans the fixup work.

## Audit Results

### Summary

| Category          | Count | Status                       | Severity |
|-------------------|-------|------------------------------|----------|
| Native Rust zenoh | 7     | Clean                        | —        |
| Native Rust XRCE  | 8     | Clean (ported in 38.1–38.3)  | —        |
| Native C zenoh    | 3     | Clean                        | —        |
| QEMU ARM zenoh    | 2     | Clean                        | —        |
| QEMU ESP32 zenoh  | 2     | Clean (fixed in 38.4–38.6)   | —        |
| ESP32 hardware    | 2     | Clean                        | —        |
| STM32F4 zenoh     | 1     | Clean (2 moved in 38.7–38.9) | —        |
| Zephyr Rust zenoh | 6     | Clean                        | —        |

Total: 33 examples audited, 12 had issues, all resolved.

### Clean examples (all 31 in `examples/`)

All examples now use only the public API:

- **Native Rust zenoh** (7): talker, listener, service-server, service-client, action-server, action-client, custom-msg — all use `nros` + `Context::from_env()` / executor API
- **Native Rust XRCE** (8): talker, listener, serial-talker, serial-listener, service-server, service-client, action-server, action-client — all use `nros` with `rmw-xrce` feature + `XrceExecutor`/`XrceNode` API
- **Native C zenoh** (3): talker, listener, custom-msg — use public C headers via `nros/init.h`, `nros/node.h`
- **QEMU ARM** (2): talker, listener — use `nros-mps2-an385` board crate
- **QEMU ESP32** (2): talker, listener — use `nros-esp32-qemu` board crate + generated `std_msgs`
- **ESP32 hardware** (2): talker, listener — use `nros-esp32` board crate
- **STM32F4 talker** (1): uses `nros-stm32f4` board crate
- **Zephyr** (6): talker, listener, service-server, service-client, action-server, action-client — use `nros` with `ShimExecutor`/`ShimNode`

### Porting references (moved out of `examples/`)

- **STM32F4 polling/rtic** (2): moved to `packages/reference/stm32f4-porting/` — these are porting references that demonstrate raw platform plumbing, not user-facing examples

## Completed Work Items

### Phase 38A: Quick wins (38.4–38.9) ✓

- [x] **38.4–38.6**: QEMU ESP32 generated bindings — added `package.xml`, `std_msgs` dep, `.cargo/config.toml` patches; replaced hand-written `mod msg { Int32 }` with `use std_msgs::msg::Int32;`
- [x] **38.7–38.9**: Reclassified STM32F4 polling/rtic — moved to `packages/reference/stm32f4-porting/`, fixed relative paths, added README, updated justfile

### Phase 38B: Safety listener port (38.10–38.11) ✓

- [x] **38.10**: Added `SubscriptionCallbackWithSafety<M>` trait + `SubscriptionEntryWithSafety` to executor; added `NodeHandle::create_subscription_with_safety()` method (gated on `safety-e2e` feature)
- [x] **38.11**: Ported safety-e2e listener from deprecated `ConnectedNode::new()` to `Context::from_env()` + executor + `create_subscription_with_safety`

### Phase 38C: XRCE node API (38.1–38.3) ✓

- [x] **38.1**: Created `packages/core/nros-node/src/xrce.rs` with typed wrappers: `XrceExecutor`, `XrceNode`, `XrceNodePublisher<M>`, `XrceNodeSubscription<M>`, `XrceNodeServiceServer<S>`, `XrceNodeServiceClient<S>`, `XrceNodeError`; safe transport init (`init_posix_udp`, `init_posix_serial`); re-exported through `nros::xrce::*`
- [x] **38.2**: Ported 6 XRCE pub/sub + service examples to single `nros` dep with `use nros::xrce::*`
- [x] **38.3**: Ported 2 XRCE action examples — use `XrceExecutor` for session lifecycle + `executor.session_mut()` for manual action protocol composition (typed action API out of scope); added RMW trait re-exports (`Publisher`, `Subscriber`, `Session`, `ServiceServerTrait`, `ServiceClientTrait`) and `heapless` to `nros` crate

### Phase 38D: Public API cleanup (38.12–38.14) ✓

- [x] **38.12–38.13**: Moved zenoh backend internals to `nros::internals::*` (`Ros2Liveliness`, `ZenohSession`, `ZenohTransport`, `ShimPublisher`, `ShimSession`, `ShimTransport`, `ShimLivelinessToken`, etc.); kept user-facing types at top level (shim node types, core types, XRCE node types)
- [x] **38.14**: Deprecated `TransportConfig` with `#[deprecated(note = "Use Context::from_env() or Context::new(InitOptions) instead")]`

## Verification Results

All checks pass:

1. `just quality` — format + clippy + 456 unit tests + Miri + embedded examples all green
2. `cargo check -p nros --no-default-features -F rmw-xrce,platform-posix,std,ros-humble` — XRCE feature path compiles
3. `cargo check -p nros --no-default-features -F rmw-zenoh,platform-posix,std,safety-e2e,ros-humble` — safety-e2e path compiles
4. `rg 'use (nros_rmw[^_])' examples/` — zero matches
5. `rg 'use nros_core' examples/` — zero matches
6. `rg 'use (zpico_|xrce_)' examples/` — zero matches
7. `rg 'ConnectedNode::new' examples/` — zero matches (deprecated API removed from all examples)

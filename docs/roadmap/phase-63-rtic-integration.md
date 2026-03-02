# Phase 63 — RTIC Integration

**Goal**: Enable nano-ros on RTIC (Real-Time Interrupt-driven Concurrency) by documenting the
usage pattern and completing the board-crate API changes needed to support RTIC's `#[init]` model.

**Status**: Not Started

**Priority**: Medium

**Depends on**: Phase 51 (board crate `run()` API), Phase 61 (FFI guards), Phase 62 (async waking)

## Overview

[RTIC](https://rtic.rs/) is a hardware-accelerated concurrency framework for Cortex-M and
RISC-V. It uses the interrupt controller (NVIC on Cortex-M, CLIC on RISC-V) as its scheduler,
with the Stack Resource Policy (SRP) enforced at compile time by the `#[app]` proc macro.

Key properties:
- **Deadlock freedom** — proven via Coq (RTFM-core compilation)
- **Bounded priority inversion** — at most one critical section
- **Single shared stack** — no per-task stacks, minimal RAM
- **Zero-cost at runtime** — scheduling is pure hardware interrupt dispatch
- **~4 microsecond total latency** on nRF52840

### Position in the Architecture

RTIC does **not** map to any of nano-ros's three orthogonal axes. It is an alternative
execution model that replaces the Executor's callback dispatch:

```
              RMW Backend      Platform              Scheduler/Runtime
              -----------      --------              -----------------
existing:     rmw-zenoh        platform-posix        Executor::spin_once()
              rmw-xrce         platform-zephyr       Executor::spin_blocking()
                               platform-bare-metal   Executor::spin_async()
                               platform-freertos
                               platform-nuttx
                               platform-threadx

RTIC:         (orthogonal)     NOT a platform        replaces Executor dispatch
```

- **RMW backends** — fully orthogonal; both zenoh-pico and XRCE-DDS work under RTIC
- **Platforms** — RTIC is NOT a platform; it provides no networking, memory, or clock.
  A platform layer (bare-metal + smoltcp) is still needed underneath
- **Execution model** — RTIC replaces the Executor's callback arena and spin loop
  with hardware-priority scheduling. The Executor is still needed for transport I/O
  via `spin_once(0)`

### Key Insight: Handles Are Independent After Creation

All nano-ros handles are **independent after creation** — they call zpico/XRCE FFI
directly without needing the Executor:

- `Publisher::publish()` → `zpico_publish()` FFI directly
- `Subscription::try_recv()` → reads from atomic buffer directly
- `ServiceServer::handle_request()` → FFI directly
- `ServiceClient::call()` → FFI directly, returns `Promise`
- `ActionServer` / `ActionClient` — same pattern

`Node` is a **temporary factory** that borrows the session during init to create handles,
then can be dropped. The only thing that still needs the executor post-init is I/O driving.
When `MAX_CBS=0`, `spin_once()` reduces to just `session.drive_io()`.

### Execution Model Taxonomy

| Model                 | Type               | Platforms                      | nano-ros API used                              | Scheduling                  |
|-----------------------|--------------------|--------------------------------|------------------------------------------------|-----------------------------|
| **Built-in Executor** | Built-in           | All                            | `spin_once()`, `spin_blocking()`, callbacks    | Software poll loop          |
| **RTOS task loop**    | Thread-per-node    | FreeRTOS, NuttX, Zephyr, POSIX | `spin_once()` in task/thread                   | RTOS preemptive             |
| **tokio**             | Async runtime      | POSIX                          | `spin_async()` via `spawn_local`               | Cooperative + OS threads    |
| **Embassy**           | Async runtime      | Cortex-M, RISC-V, Zephyr       | `spin_async()` via `#[embassy_executor::task]` | Cooperative (WFI/SEV)       |
| **RTIC**              | Hardware scheduler | Cortex-M, RISC-V               | `spin_once(0)` + `try_recv()` (manual poll)    | Preemptive, SRP (NVIC/CLIC) |

The first four are **additive** (they layer on top of the Executor). RTIC is **substitutive**
(it replaces the Executor's callback dispatch with its own scheduling model).

### No Feature Flag Needed

RTIC integration is an application-level usage pattern, like tokio or Embassy. No
`executor-rtic` or `scheduler-rtic` feature is needed. The only nano-ros feature RTIC
users might need is `sync-critical-section` (already exists) for RTIC-compatible mutexes.

If a helper crate were ever shipped (e.g., `nros-rtic`), it would be a **separate crate**,
not a feature flag on `nros`.

## Design

### The RTIC Pattern for nano-ros

All nano-ros entities go in `#[local]` resources — no locks needed:

```
#[init]                          #[local] to net_poll task
  board::init_hardware()           Executor<_, 0, 0>
  Executor::open()
  node = executor.create_node()  #[local] to application tasks
  publisher = node.create_*()      Publisher, Subscription, ServiceServer, etc.
  subscription = node.create_*()
  (node dropped)

#[shared]
  struct Shared {}               ← empty, no locks needed
```

### Priority Design

With Phase 61 (FFI guards) complete, all FFI calls are wrapped in
`critical_section::with()` when `ffi-sync` is enabled. This means:

| Scenario                              | Without `ffi-sync` | With `ffi-sync` |
|---------------------------------------|-------------------------------------|----------------------------------|
| All tasks at priority 1               | **Safe** (cooperative)              | **Safe**                         |
| net_poll at priority 2, app at 1      | **Unsafe**                          | **Safe**                         |
| Any mixed priorities                  | **Unsafe**                          | **Safe**                         |

**Recommendation**: All initial RTIC examples use priority 1 for simplicity.
Advanced users can enable `ffi-sync` for mixed-priority configurations
(e.g., higher-priority `net_poll` for lower latency). See Phase 61 for full reentrancy
analysis.

### Async API Compatibility

**`Promise` implements `core::future::Future`** (`handles.rs:300`). With Phase 62
completed, `promise.await` is **event-driven** — the `AtomicWaker` fires when data
arrives, so the CPU can enter WFI between events.

**`FeedbackStream` has `recv().await`** (`handles.rs:737`) and optionally implements
`futures_core::Stream` (behind the `stream` feature). Also event-driven after Phase 62.

**`Subscription` has `recv().await`** after Phase 62 (new Future/Stream implementation).
For poll-based usage, `try_recv()` remains available.

Summary of async availability (with Phase 62 complete):

| Type                        | `.await`                        | `try_recv()` loop           | Notes                      |
|-----------------------------|---------------------------------|-----------------------------|----------------------------|
| `Promise` (service reply)   | **Yes** — `promise.await?`      | Yes                         | Event-driven (AtomicWaker) |
| `Promise` (goal acceptance) | **Yes** — `promise.await?`      | Yes                         | Same                       |
| `Promise` (action result)   | **Yes** — `promise.await?`      | Yes                         | Same                       |
| `FeedbackStream`            | **Yes** — `stream.recv().await` | Yes (`try_recv_feedback()`) | Same                       |
| `Subscription`              | **Yes** — `sub.recv().await`    | Yes (`try_recv()`)          | Same                       |
| `ServiceServer`             | **No**                          | Yes (`handle_request()`)    | Must use `Mono::delay()`   |

### Promise::wait() Limitation

`Promise::wait()` takes `&mut Executor` and is NOT usable in RTIC, because the executor
is `#[local]` to the `net_poll` task. Use `.await` (event-driven after Phase 62) or a
`try_recv()` + `Mono::delay().await` loop instead.

## Usage Examples

All examples use only existing nano-ros API — no new methods required.

### Talker (Publisher)

```rust
#![no_std]
#![no_main]

use panic_semihosting as _;
use my_pac as pac;  // STM32, nRF, or other PAC
use nros::prelude::*;
use nros_my_board::{self as board, Config};
use std_msgs::msg::Int32;

type NrosSession = nros::RmwSession;

#[rtic::app(device = pac, dispatchers = [UART0, UART1])]
mod app {
    use super::*;

    #[shared]
    struct Shared {}

    #[local]
    struct Local {
        executor: Executor<NrosSession, 0, 0>,
        publisher: Publisher<NrosSession, Int32>,
    }

    #[init]
    fn init(_cx: init::Context) -> (Shared, Local) {
        let config = Config::default();
        board::init_hardware(&config);

        let exec_config = ExecutorConfig::new(config.zenoh_locator)
            .domain_id(config.domain_id)
            .node_name("talker");
        let mut executor = Executor::<_, 0, 0>::open(&exec_config).unwrap();
        let mut node = executor.create_node("talker").unwrap();
        let publisher = node.create_publisher::<Int32>("/chatter").unwrap();

        net_poll::spawn().unwrap();
        publish::spawn().unwrap();

        (Shared {}, Local { executor, publisher })
    }

    /// Drive transport I/O — equivalent to rclcpp spin_some().
    #[task(local = [executor], priority = 1)]
    async fn net_poll(cx: net_poll::Context) {
        loop {
            cx.local.executor.spin_once(0);
            Mono::delay(1.millis()).await;
        }
    }

    /// Publish messages. Does not require the executor (same as rclrs).
    #[task(local = [publisher], priority = 1)]
    async fn publish(cx: publish::Context) {
        Mono::delay(1000.millis()).await; // wait for zenoh session

        for i in 0..10i32 {
            Mono::delay(1000.millis()).await;
            cx.local.publisher.publish(&Int32 { data: i }).unwrap();
        }

        board::exit_success();
    }
}
```

### Listener (Subscription)

```rust
#[rtic::app(device = pac, dispatchers = [UART0, UART1])]
mod app {
    use super::*;

    #[shared]
    struct Shared {}

    #[local]
    struct Local {
        executor: Executor<NrosSession, 0, 0>,
        subscription: Subscription<NrosSession, Int32>,
    }

    #[init]
    fn init(_cx: init::Context) -> (Shared, Local) {
        let config = Config::listener();
        board::init_hardware(&config);

        let exec_config = ExecutorConfig::new(config.zenoh_locator)
            .domain_id(config.domain_id)
            .node_name("listener");
        let mut executor = Executor::<_, 0, 0>::open(&exec_config).unwrap();
        let mut node = executor.create_node("listener").unwrap();
        let subscription = node.create_subscription::<Int32>("/chatter").unwrap();

        net_poll::spawn().unwrap();
        listen::spawn().unwrap();

        (Shared {}, Local { executor, subscription })
    }

    #[task(local = [executor], priority = 1)]
    async fn net_poll(cx: net_poll::Context) {
        loop {
            cx.local.executor.spin_once(0);
            Mono::delay(1.millis()).await;
        }
    }

    #[task(local = [subscription], priority = 1)]
    async fn listen(cx: listen::Context) {
        let mut count = 0u32;
        loop {
            if let Some(msg) = cx.local.subscription.try_recv().unwrap() {
                count += 1;
                board::println!("Received [{}]: {}", count, msg.data);
                if count >= 10 {
                    board::exit_success();
                }
            }
            Mono::delay(1.millis()).await;
        }
    }
}
```

### Service Server

```rust
#[rtic::app(device = pac, dispatchers = [UART0, UART1])]
mod app {
    use super::*;
    use example_interfaces::srv::AddTwoInts;

    #[shared]
    struct Shared {}

    #[local]
    struct Local {
        executor: Executor<NrosSession, 0, 0>,
        service: EmbeddedServiceServer<AddTwoInts, /* ... */>,
    }

    #[init]
    fn init(_cx: init::Context) -> (Shared, Local) {
        let config = Config::default();
        board::init_hardware(&config);

        let exec_config = ExecutorConfig::new(config.zenoh_locator)
            .domain_id(config.domain_id)
            .node_name("add_server");
        let mut executor = Executor::<_, 0, 0>::open(&exec_config).unwrap();
        let mut node = executor.create_node("add_server").unwrap();
        let service = node.create_service::<AddTwoInts>("/add_two_ints").unwrap();

        net_poll::spawn().unwrap();
        serve::spawn().unwrap();

        (Shared {}, Local { executor, service })
    }

    #[task(local = [executor], priority = 1)]
    async fn net_poll(cx: net_poll::Context) {
        loop {
            cx.local.executor.spin_once(0);
            Mono::delay(1.millis()).await;
        }
    }

    #[task(local = [service], priority = 1)]
    async fn serve(cx: serve::Context) {
        loop {
            cx.local.service.handle_request(|req| {
                AddTwoIntsReply { sum: req.a + req.b }
            }).unwrap();
            Mono::delay(1.millis()).await;
        }
    }
}
```

### Service Client

```rust
#[rtic::app(device = pac, dispatchers = [UART0, UART1])]
mod app {
    use super::*;
    use example_interfaces::srv::AddTwoInts;

    #[shared]
    struct Shared {}

    #[local]
    struct Local {
        executor: Executor<NrosSession, 0, 0>,
        client: EmbeddedServiceClient<AddTwoInts, /* ... */>,
    }

    #[init]
    fn init(_cx: init::Context) -> (Shared, Local) {
        let config = Config::default();
        board::init_hardware(&config);

        let exec_config = ExecutorConfig::new(config.zenoh_locator)
            .domain_id(config.domain_id)
            .node_name("add_client");
        let mut executor = Executor::<_, 0, 0>::open(&exec_config).unwrap();
        let mut node = executor.create_node("add_client").unwrap();
        let client = node.create_client::<AddTwoInts>("/add_two_ints").unwrap();

        net_poll::spawn().unwrap();
        call_service::spawn().unwrap();

        (Shared {}, Local { executor, client })
    }

    #[task(local = [executor], priority = 1)]
    async fn net_poll(cx: net_poll::Context) {
        loop {
            cx.local.executor.spin_once(0);
            Mono::delay(1.millis()).await;
        }
    }

    /// Promise implements Future — .await works directly in RTIC async tasks.
    /// Event-driven waking (Phase 62) — CPU enters WFI between polls.
    #[task(local = [client], priority = 1)]
    async fn call_service(cx: call_service::Context) {
        Mono::delay(2000.millis()).await; // wait for server

        let request = AddTwoIntsRequest { a: 5, b: 3 };
        let reply = cx.local.client.call(&request).unwrap().await.unwrap();
        board::println!("Sum: {}", reply.sum);

        board::exit_success();
    }
}
```

### Action Server

```rust
#[rtic::app(device = pac, dispatchers = [UART0, UART1])]
mod app {
    use super::*;
    use example_interfaces::action::Fibonacci;

    #[shared]
    struct Shared {}

    #[local]
    struct Local {
        executor: Executor<NrosSession, 0, 0>,
        server: ActionServer<Fibonacci, /* ... */>,
    }

    #[init]
    fn init(_cx: init::Context) -> (Shared, Local) {
        let config = Config::default();
        board::init_hardware(&config);

        let exec_config = ExecutorConfig::new(config.zenoh_locator)
            .domain_id(config.domain_id)
            .node_name("fibonacci_server");
        let mut executor = Executor::<_, 0, 0>::open(&exec_config).unwrap();
        let mut node = executor.create_node("fibonacci_server").unwrap();
        let server = node.create_action_server::<Fibonacci>("/fibonacci").unwrap();

        net_poll::spawn().unwrap();
        action_serve::spawn().unwrap();

        (Shared {}, Local { executor, server })
    }

    #[task(local = [executor], priority = 1)]
    async fn net_poll(cx: net_poll::Context) {
        loop {
            cx.local.executor.spin_once(0);
            Mono::delay(1.millis()).await;
        }
    }

    #[task(local = [server], priority = 1)]
    async fn action_serve(cx: action_serve::Context) {
        loop {
            // Accept new goals
            if let Ok(Some(goal_id)) = cx.local.server.try_accept_goal(|_id, goal| {
                GoalResponse::AcceptAndExecute
            }) {
                cx.local.server.set_goal_status(&goal_id, GoalStatus::Executing);

                // Execute goal (compute fibonacci sequence)
                let result = FibonacciResult { sequence: /* ... */ };
                cx.local.server.complete_goal(
                    &goal_id, GoalStatus::Succeeded, result,
                );

                // CRITICAL: must call explicitly in manual-poll mode
                // (action server is NOT arena-registered)
                for _ in 0..200 {
                    let _ = cx.local.server.try_handle_get_result();
                    Mono::delay(10.millis()).await;
                }
            }

            // Handle cancel requests
            let _ = cx.local.server.try_handle_cancel(|_id, _status| {
                CancelResponse::Ok
            });

            Mono::delay(10.millis()).await;
        }
    }
}
```

### Action Client

```rust
#[rtic::app(device = pac, dispatchers = [UART0, UART1])]
mod app {
    use super::*;
    use example_interfaces::action::Fibonacci;

    #[shared]
    struct Shared {}

    #[local]
    struct Local {
        executor: Executor<NrosSession, 0, 0>,
        client: ActionClient<Fibonacci, /* ... */>,
    }

    #[init]
    fn init(_cx: init::Context) -> (Shared, Local) {
        let config = Config::default();
        board::init_hardware(&config);

        let exec_config = ExecutorConfig::new(config.zenoh_locator)
            .domain_id(config.domain_id)
            .node_name("fibonacci_client");
        let mut executor = Executor::<_, 0, 0>::open(&exec_config).unwrap();
        let mut node = executor.create_node("fibonacci_client").unwrap();
        let client = node.create_action_client::<Fibonacci>("/fibonacci").unwrap();

        net_poll::spawn().unwrap();
        action_call::spawn().unwrap();

        (Shared {}, Local { executor, client })
    }

    #[task(local = [executor], priority = 1)]
    async fn net_poll(cx: net_poll::Context) {
        loop {
            cx.local.executor.spin_once(0);
            Mono::delay(1.millis()).await;
        }
    }

    /// Action client uses .await on promises and feedback stream.
    /// Event-driven waking (Phase 62) — CPU enters WFI between events.
    #[task(local = [client], priority = 1)]
    async fn action_call(cx: action_call::Context) {
        Mono::delay(2000.millis()).await; // wait for server

        // Send goal — .await on acceptance promise
        let goal = FibonacciGoal { order: 5 };
        let (goal_id, accept_promise) =
            cx.local.client.send_goal(&goal).unwrap();
        let accepted = accept_promise.await.unwrap();
        if !accepted {
            board::println!("Goal rejected");
            board::exit_failure();
        }

        // Receive feedback via async stream
        let mut stream = cx.local.client.feedback_stream_for(goal_id);
        while let Some(result) = stream.recv().await {
            let (_id, feedback) = result.unwrap();
            board::println!("Feedback: {:?}", feedback.partial_sequence);
            // break when done (e.g., after expected count)
        }

        // Get result — .await on result promise
        let (status, result) =
            cx.local.client.get_result(&goal_id).unwrap().await.unwrap();
        board::println!("Result: {:?} {:?}", status, result.sequence);

        board::exit_success();
    }
}
```

## Work Items

- [ ] 63.1 — Factor `board::init_hardware()` out of `board::run()`
- [ ] 63.2 — RTIC talker/listener example (`examples/stm32f4/rust/zenoh/rtic-{talker,listener}/`)
- [ ] 63.3 — RTIC service example (`rtic-service-{server,client}/`)
- [ ] 63.4 — RTIC action example (`rtic-action-{server,client}/`)
- [ ] 63.5 — RTIC integration test (lm3s6965evb QEMU + lm3s6965 PAC)

### 63.1 — Factor `board::init_hardware()` out of `board::run()`

Board crates currently bundle hardware init and application execution in `run()`. RTIC's
`#[init]` needs these separated so it can call `init_hardware()` and then return
`(Shared, Local)`. Expose existing helpers (`create_ethernet()`, `init_network()`) as
public API.

This overlaps with Phase 51 (board crate `run()` API) — coordinate to avoid duplication.

**Status**: Not Started

**Files**:
- `packages/boards/nros-stm32f4/src/lib.rs`
- `packages/boards/nros-mps2-an385/src/lib.rs`
- `packages/boards/nros-esp32/src/lib.rs`

### 63.2 — RTIC Talker/Listener Example

Create a working RTIC example on STM32F4 (Nucleo-F429ZI) with talker and listener
using `#[local]` resources, `spin_once(0)` net_poll task, and `try_recv()` subscription
polling. STM32F4 is chosen because `nros-stm32f4` board crate already exists and the
`stm32f4xx-hal` PAC provides interrupt definitions for RTIC's `dispatchers`.

The `rtic-` prefix follows the existing `async-` prefix convention (e.g.,
`async-service`, `async-action`) — execution model variants are prefixed on the
use-case name within the standard 4-level hierarchy.

**Status**: Not Started

**Files**:
- `examples/stm32f4/rust/zenoh/rtic-talker/` (new)
- `examples/stm32f4/rust/zenoh/rtic-listener/` (new)

### 63.3 — RTIC Service Example

Service server and client examples. Service client demonstrates `promise.await`
(or `try_recv()` loop for power-sensitive applications).

**Status**: Not Started

**Files**:
- `examples/stm32f4/rust/zenoh/rtic-service-server/` (new)
- `examples/stm32f4/rust/zenoh/rtic-service-client/` (new)

### 63.4 — RTIC Action Example

Action server and client examples. Server demonstrates explicit `try_handle_get_result()`
calls (required in manual-poll mode). Client demonstrates `promise.await` for goal
acceptance and result, `stream.recv().await` for feedback.

**Status**: Not Started

**Files**:
- `examples/stm32f4/rust/zenoh/rtic-action-server/` (new)
- `examples/stm32f4/rust/zenoh/rtic-action-client/` (new)

### 63.5 — RTIC Integration Test

Use the same QEMU test strategy as the RTIC project itself: `lm3s6965evb` machine
with the [`lm3s6965`](https://crates.io/crates/lm3s6965) PAC crate (v0.2, MIT/Apache-2.0).
This PAC provides only interrupt bindings (`Interrupt` enum with 44 variants) — no
register APIs, no HAL. nano-ros already has `lm3s6965evb` QEMU infrastructure
(`QemuProcess::start_cortex_m3()` in `nros-tests`).

**Two-tier test plan**:

| Tier | QEMU Machine     | PAC         | Networking | What it validates                                   |
|------|------------------|-------------|------------|-----------------------------------------------------|
| 1    | `lm3s6965evb`    | `lm3s6965`  | None       | RTIC task dispatch, `spin_once(0)`, `try_recv()`    |
| 2    | `mps2-an385`     | Minimal stub| LAN9118    | RTIC + zenoh talker/listener over network           |

**Tier 1** (non-networked): Validates RTIC + nano-ros handle lifecycle, init pattern,
and cooperative scheduling. Uses semihosting for output and `cortex_m_semihosting::
debug::exit(EXIT_SUCCESS)` for pass/fail. QEMU runner in `.cargo/config.toml`:

```toml
[target.thumbv7m-none-eabi]
runner = "qemu-system-arm -cpu cortex-m3 -machine lm3s6965evb -nographic -semihosting-config enable=on,target=native -kernel"
```

**Tier 2** (networked): Requires a minimal MPS2-AN385 PAC with CMSDK interrupt
definitions (AN385 has 45 IRQs). This can be a small in-tree crate (~100 lines)
following the `lm3s6965` pattern. RTIC dispatchers only need unused NVIC slots —
the interrupt names just need valid vector numbers not used by LAN9118 or timers.

**Status**: Not Started

**Files**:
- `tests/test-rtic.sh` (new)
- `justfile` — add `test-rtic` recipe
- `packages/testing/nros-tests/` — `lm3s6965` PAC dependency for tier 1
- Tier 2: minimal MPS2-AN385 PAC crate (future, if networked RTIC tests needed)

## Acceptance Criteria

- [ ] `board::init_hardware()` is a public function on at least one board crate
- [ ] RTIC talker/listener example compiles and runs on target hardware (or QEMU)
- [ ] All RTIC examples use `#[local]` for all nano-ros handles (no `#[shared]` locks)
- [ ] All RTIC examples use only existing nano-ros API (`spin_once(0)`, `try_recv()`,
      `publish()`, `handle_request()`, `try_handle_get_result()`, `.await`) — no new methods
- [ ] `Promise::wait()` limitation is documented; examples use `.await` or `try_recv()` loops
- [ ] All tasks run at priority 1 (documented as safety requirement)
- [ ] `just quality` passes

## Notes

- **QEMU test platform**: RTIC's own CI uses `lm3s6965evb` QEMU + `lm3s6965` PAC
  (interrupt-only, ~44 IRQ variants, no register APIs). nano-ros already has
  `lm3s6965evb` QEMU support via `QemuProcess::start_cortex_m3()`. Non-networked
  RTIC tests (tier 1) use this directly. Networked RTIC tests (tier 2) need
  MPS2-AN385 with a minimal in-tree PAC stub for CMSDK interrupts
- **RTIC v2 async**: All examples use RTIC v2 `async fn` software tasks. Hardware tasks
  (`#[task(binds = TIM2)]`) could be used for periodic net_poll but add complexity
  without clear benefit for the initial integration
- **Target board**: STM32F4 (Nucleo-F429ZI) is the primary target for real-hardware
  examples — `nros-stm32f4` board crate exists, `stm32f4xx-hal` PAC provides RTIC
  dispatcher interrupts, and RTIC has strong STM32F4 community support. nRF52840 is
  a secondary option. For CI testing, `lm3s6965evb` QEMU (see above)
- **Example naming**: `rtic-` prefix on use-case (e.g., `rtic-talker`) follows the
  existing `async-` prefix convention. RTIC is an execution model variant, not a
  platform or RMW choice, so it stays within the 4-level hierarchy:
  `examples/stm32f4/rust/zenoh/rtic-{use-case}/`
- **`sync-critical-section`**: Already exists in `nros`. RTIC users should enable this
  feature for RTIC-compatible mutex implementations
- **No `drive_io()` method**: A dedicated `drive_io()` on Executor was considered and
  rejected to stay aligned with ROS 2 API conventions. `spin_once(0)` is the equivalent
  of rclcpp's `spin_some()` — "process available work, don't block"
- **Prerequisites**: FFI reentrancy guards (Phase 61) and event-driven async waking
  (Phase 62) are completed before this phase, so RTIC examples ship with mixed-priority
  support and proper `.await` from day one

# Phase 63 — RTIC Integration

**Goal**: Enable nano-ros on RTIC (Real-Time Interrupt-driven Concurrency) by documenting the
usage pattern and completing the board-crate API changes needed to support RTIC's `#[init]` model.

**Status**: In Progress (63.1–63.7 done)

**Priority**: Medium

**Depends on**: Phase 51 (board crate `run()` API — ✅ Complete), Phase 61 (FFI guards — ✅ Complete), Phase 62 (async waking — ✅ Complete)

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
#[init](cx)                      #[local] to net_poll task
  syst = board::init_hardware(     Executor<_, 0, 0>
    &config, cx.device, cx.core)
  Mono::start(syst, freq)       #[local] to application tasks
  Executor::open()                 Publisher, Subscription, ServiceServer, etc.
  node = executor.create_node()
  publisher = node.create_*()
  subscription = node.create_*()
  (node dropped)

#[shared]
  struct Shared {}               ← empty, no locks needed
```

`init_hardware()` accepts device and core peripherals by value (avoiding ownership
conflicts with RTIC) and returns `SYST` for the monotonic timer.

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

- [x] 63.1 — Factor `board::init_hardware()` out of `board::run()`
- [x] 63.2 — RTIC talker/listener example (`examples/stm32f4/rust/zenoh/rtic-{talker,listener}/`)
- [x] 63.3 — RTIC service example (`rtic-service-{server,client}/`)
- [x] 63.4 — RTIC action example (`rtic-action-{server,client}/`)
- [x] 63.5 — MPS2-AN385 PAC crate (`packages/boards/mps2-an385-pac/`)
- [x] 63.6 — RTIC QEMU examples (`examples/qemu-arm-baremetal/rust/zenoh/rtic-{talker,listener}/`)
- [x] 63.7 — RTIC QEMU integration test (`test-rtic` justfile recipe)

### 63.1 — Factor `board::init_hardware()` out of `board::run()`

Board crates currently bundle hardware init and application execution in `run()`. RTIC's
`#[init]` needs these separated so it can call `init_hardware()` and then return
`(Shared, Local)`. Expose existing helpers (`create_ethernet()`, `init_network()`) as
public API.

This overlaps with Phase 51 (board crate `run()` API) — coordinate to avoid duplication.

**Status**: Complete

**Implementation**: All 8 board crates now export `init_hardware()`. The 4 smoltcp-based
crates (stm32f4, mps2-an385, esp32, esp32-qemu) use `MaybeUninit` statics to store
network objects (Ethernet device, smoltcp Interface, SocketSet) so `set_network_state()`
pointers remain valid after `init_hardware()` returns. The 4 RTOS-based crates (freertos,
nuttx, threadx-linux, threadx-qemu-riscv64) have trivial implementations (no-ops) for
API consistency — their hardware init is handled by the RTOS kernel/C code. `run()` now
delegates to `init_hardware()` internally.

**STM32F4 peripheral ownership**: `init_hardware()` accepts `pac::Peripherals` and
`cortex_m::Peripherals` by value and returns `cortex_m::peripheral::SYST` (unused by
init, needed by RTIC for `Mono::start()`). This avoids ownership conflicts with RTIC
which takes peripherals before calling `#[init]`. `run()` calls `Peripherals::take()`
internally so existing non-RTIC code is unaffected.

**Files**:
- `packages/boards/nros-stm32f4/src/node.rs` + `lib.rs`
- `packages/boards/nros-mps2-an385/src/node.rs` + `lib.rs`
- `packages/boards/nros-esp32/src/node.rs` + `lib.rs`
- `packages/boards/nros-esp32-qemu/src/node.rs` + `lib.rs`
- `packages/boards/nros-mps2-an385-freertos/src/node.rs` + `lib.rs`
- `packages/boards/nros-nuttx-qemu-arm/src/node.rs` + `lib.rs`
- `packages/boards/nros-threadx-linux/src/node.rs` + `lib.rs`
- `packages/boards/nros-threadx-qemu-riscv64/src/node.rs` + `lib.rs`

### 63.2 — RTIC Talker/Listener Example

Create a working RTIC example on STM32F4 (Nucleo-F429ZI) with talker and listener
using `#[local]` resources, `spin_once(0)` net_poll task, and `try_recv()` subscription
polling. STM32F4 is chosen because `nros-stm32f4` board crate already exists and the
`stm32f4xx-hal` PAC provides interrupt definitions for RTIC's `dispatchers`.

The `rtic-` prefix follows the existing `async-` prefix convention (e.g.,
`async-service-client`, `async-action-client`) — execution model variants are prefixed on the
use-case name within the standard 4-level hierarchy.

**Status**: Complete

**Implementation**: Both examples use RTIC v2 async tasks with `rtic-monotonics` SysTick
monotonic for delays. `init_hardware(config, cx.device, cx.core)` receives peripherals
from RTIC's context and returns SYST for `Mono::start()`. Type aliases
(`NrosExecutor`, `NrosPublisher`/`NrosSubscription`) provide clean `Local` struct
annotations using `nros::internals::Rmw*` types.

Key patterns demonstrated:
- `Executor<_, 0, 0>` — zero callback arena (RTIC replaces callback dispatch)
- `spin_once(0)` in `net_poll` task — non-blocking I/O drive
- `try_recv()` in `listen` task — manual subscription polling
- All handles `#[local]` — no `#[shared]` locks needed
- Both tasks at priority 1 for safety (see Priority Design section)

**Files**:
- `examples/stm32f4/rust/zenoh/rtic-talker/` (new)
- `examples/stm32f4/rust/zenoh/rtic-listener/` (new)

### 63.3 — RTIC Service Example

Service server and client examples. Server demonstrates `handle_request()` polling.
Client demonstrates `client.call()` + `promise.try_recv()` loop (RTIC-compatible
pattern since `Promise::wait()` requires `&mut Executor` which is `#[local]` to net_poll).

**Status**: Complete

**Implementation**: Both STM32F4 cross-compiled examples and native x86 test equivalents.
The STM32F4 examples follow the same RTIC v2 patterns as 63.2 (zero callback arena,
`spin_once(0)`, all handles `#[local]`, priority 1). The service client uses a
`try_recv()` + `Mono::delay().await` loop instead of `Promise::wait()`.

Native equivalents exercise the identical API pattern on x86 for interop testing.
Integration test (`test_rtic_pattern_service` in `nano2nano.rs`) validates 4/4 service
calls succeed with correct results via zenohd.

**Files**:
- `examples/stm32f4/rust/zenoh/rtic-service-server/` (new)
- `examples/stm32f4/rust/zenoh/rtic-service-client/` (new)
- `examples/native/rust/zenoh/rtic-service-server/` (new, test equivalent)
- `examples/native/rust/zenoh/rtic-service-client/` (new, test equivalent)

### 63.4 — RTIC Action Example

Action server and client examples. Server demonstrates explicit `try_accept_goal()`,
`publish_feedback()`, `complete_goal()`, and `try_handle_get_result()` calls (required
in manual-poll mode — action server is NOT arena-registered). Client demonstrates
`send_goal()` + `promise.try_recv()` for acceptance and `try_recv_feedback()` for
feedback polling (RTIC-compatible patterns since `Promise::wait()` and
`FeedbackStream::wait_next()` require `&mut Executor`).

**Status**: Complete

**Implementation**: Both STM32F4 cross-compiled examples and native x86 test equivalents.
The action server computes Fibonacci sequences, publishing feedback after each step and
calling `try_handle_get_result()` explicitly after `complete_goal()`. The action client
uses `try_recv()` loops for goal acceptance and `try_recv_feedback()` filtered by
`goal_id.uuid` for feedback. Native integration test (`test_rtic_pattern_action` in
`nano2nano.rs`) validates goal acceptance and 6 feedback messages via zenohd.

**Files**:
- `examples/stm32f4/rust/zenoh/rtic-action-server/` (new)
- `examples/stm32f4/rust/zenoh/rtic-action-client/` (new)
- `examples/native/rust/zenoh/rtic-action-server/` (new, test equivalent)
- `examples/native/rust/zenoh/rtic-action-client/` (new, test equivalent)

### 63.5 — MPS2-AN385 PAC Crate

Create a minimal in-tree PAC for the ARM CMSDK Cortex-M3 (MPS2-AN385 FPGA image).
RTIC needs a PAC with an `Interrupt` enum and vector table — no register APIs required.

**Why MPS2-AN385**: nano-ros already has full networking infrastructure for this QEMU
machine — `lan9118-smoltcp` driver, `nros-mps2-an385` board crate, TAP bridge networking
(192.0.3.x), and `QemuProcess::start_mps2_an385_networked()` test helpers. A single
platform covers both RTIC task dispatch validation AND networked zenoh communication,
eliminating the need for a separate non-networked tier.

**Why not lm3s6965**: The `lm3s6965` PAC exists on crates.io and RTIC's own CI uses it,
but `lm3s6965evb` QEMU has no LAN9118 Ethernet — only Stellaris MAC (no smoltcp driver).
A non-networked test validates RTIC dispatch but not the full nano-ros + zenoh stack,
which is what matters for integration testing.

**PAC structure** (follows the [`lm3s6965`](https://crates.io/crates/lm3s6965) pattern):

```
packages/boards/mps2-an385-pac/
├── Cargo.toml          # cortex-m 0.7 + cortex-m-rt 0.7 (device feature)
└── src/
    └── lib.rs          # ~150 lines: Interrupt enum, Nr impl, __INTERRUPTS, Peripherals
```

**CMSDK interrupt map** (from ARM CMSDK_CM3.h, confirmed against QEMU `mps2.c`):

QEMU configures 32 external NVIC interrupts for the AN385 variant.

| IRQ | Name          | Hardware                  | RTIC use      |
|-----|---------------|---------------------------|---------------|
| 0   | `UARTRX0`    | CMSDK UART0 RX            | **Dispatcher** |
| 1   | `UARTTX0`    | CMSDK UART0 TX            | **Dispatcher** |
| 2   | `UARTRX1`    | CMSDK UART1 RX            | **Dispatcher** |
| 3   | `UARTTX1`    | CMSDK UART1 TX            | Available      |
| 4   | `UARTRX2`    | CMSDK UART2 RX            | Available      |
| 5   | `UARTTX2`    | CMSDK UART2 TX            | Available      |
| 6   | `PORT0_ALL`  | GPIO Port 0 combined      | Available      |
| 7   | `PORT1_ALL`  | GPIO Port 1 combined      | Available      |
| 8   | `TIMER0`     | CMSDK Timer 0             | Available      |
| 9   | `TIMER1`     | CMSDK Timer 1             | Available      |
| 10  | `DUALTIMER`  | CMSDK Dual Timer          | Available      |
| 11  | `SPI`        | SPI                       | Available      |
| 12  | `UARTOVF`    | UART 0/1/2 overflow (OR'd)| Available      |
| 13  | `ETHERNET`   | LAN9118 (wired in QEMU)   | **Reserved**   |
| 14  | `I2S`        | Audio I2S                 | Available      |
| 15  | `TSC`        | Touch Screen Controller   | Available      |
| 16  | `PORT2_ALL`  | GPIO Port 2 combined      | Available      |
| 17  | `PORT3_ALL`  | GPIO Port 3 combined      | Available      |
| 18  | `UARTRX3`    | CMSDK UART3 RX            | Available      |
| 19  | `UARTTX3`    | CMSDK UART3 TX            | Available      |
| 20  | `UARTRX4`    | CMSDK UART4 RX            | Available      |
| 21  | `UARTTX4`    | CMSDK UART4 TX            | Available      |
| 22  | `ADCSPI`     | ADC SPI                   | Available      |
| 23  | `SHIELDSPI`  | Shield SPI                | Available      |
| 24–31 | `PORT0_0`–`PORT0_7` | GPIO Port 0 per-pin | Available    |

nano-ros uses **zero** NVIC interrupts on MPS2-AN385 (all I/O is polled). IRQ 13
(`ETHERNET`) is wired to LAN9118 in QEMU but no handler is bound — reserved to avoid
future conflicts if interrupt-driven Ethernet is added.

Dispatchers: `UARTRX0` (IRQ 0), `UARTTX0` (IRQ 1), `UARTRX1` (IRQ 2) — matching the
UART convention from STM32F4 examples. Three slots support up to 3 RTIC priority levels.

**Implementation requirements**:

1. `Interrupt` enum with 32 variants (one per NVIC slot)
2. `unsafe impl cortex_m::interrupt::Nr` — maps each variant to its IRQ number.
   cortex-m 0.7 has a blanket impl `InterruptNumber for T: Nr`, so RTIC v2 works
3. `extern "C"` function declarations for each interrupt (linker symbols)
4. `__INTERRUPTS: [Vector; 32]` in `.vector_table.interrupts` section
5. `Peripherals` struct with `unsafe fn steal()` (can be empty — RTIC requires it)
6. `NVIC_PRIO_BITS: u8 = 3` constant (Cortex-M3 default)
7. Edition 2024: `unsafe extern "C"` blocks, `#[unsafe(no_mangle)]` on statics

**Status**: Complete

**Implementation**: Created minimal PAC with `Interrupt` enum (32 variants matching
CMSDK CM3 interrupt map), `Nr` trait impl, `__INTERRUPTS` vector table, `Peripherals`
struct, and `device.x` linker script. Edition 2024 conventions: `unsafe extern "C"`,
`#[unsafe(no_mangle)]`, `#[unsafe(link_section)]`. Verified compilation for
`thumbv7m-none-eabi`.

**Files**:
- `packages/boards/mps2-an385-pac/Cargo.toml` (new)
- `packages/boards/mps2-an385-pac/src/lib.rs` (new)
- `packages/boards/mps2-an385-pac/device.x` (new)
- `packages/boards/mps2-an385-pac/build.rs` (new)

### 63.6 — RTIC QEMU Examples

Create RTIC talker and listener examples targeting `mps2-an385` QEMU with LAN9118
networking. These use the MPS2-AN385 PAC from 63.5 and the `nros-mps2-an385` board
crate.

The examples follow the same directory convention as existing QEMU bare-metal examples
(`examples/qemu-arm-baremetal/rust/zenoh/`) with the `rtic-` prefix.

Key differences from STM32F4 RTIC examples:
- PAC: `mps2_an385_pac` instead of `stm32f4xx_hal::pac`
- Board crate: `nros-mps2-an385` instead of `nros-stm32f4`
- Target: `thumbv7m-none-eabi` (Cortex-M3) instead of `thumbv7em-none-eabihf` (Cortex-M4F)
- Networking: LAN9118 over TAP bridge (QEMU emulated) instead of STM32 Ethernet
- Output: semihosting (`cortex_m_semihosting`) instead of defmt-rtt

**Status**: Complete

**Implementation**: Both examples use RTIC v2 async tasks with `rtic-monotonics` SysTick
monotonic (25 MHz QEMU clock). `init_hardware(config, cx.core)` receives core peripherals
from RTIC context. Dispatchers: `UARTRX0`, `UARTTX0` (unused CMSDK UARTs). Talker
publishes 10 Int32 messages on `/chatter` and exits via semihosting. Listener subscribes
to `/chatter`, counts 10 messages, exits — or times out after 30s with `exit_failure()`.
Output via `cortex_m_semihosting::hprintln!` (not defmt).

**Files**:
- `examples/qemu-arm-baremetal/rust/zenoh/rtic-talker/` (new)
- `examples/qemu-arm-baremetal/rust/zenoh/rtic-listener/` (new)

### 63.7 — RTIC QEMU Integration Test

Add networked RTIC integration tests using `QemuProcess::start_mps2_an385_networked()`
from `nros-tests`. Tests run the RTIC talker and listener as separate QEMU processes
on different TAP devices, communicating via zenohd on the bridge IP.

**Test strategy**:
- Uses existing TAP bridge infrastructure (talker on `tap-qemu0`, listener on `tap-qemu1`)
- Listener starts first, then talker (zenoh doesn't buffer for unknown subscribers)
- 5s stabilization delay between subscriber connection and publisher start
- Validates message delivery (10/10 messages) via semihosting output parsing
- Build helpers in `fixtures/binaries.rs`, test in `emulator.rs`

**Status**: Complete (build tests pass; networked E2E test skipped — requires `just build-zenoh-pico-arm`)

**Implementation**: Added `build_qemu_rtic_talker()` and `build_qemu_rtic_listener()`
helpers in `fixtures/binaries.rs`. Build tests (`test_qemu_rtic_talker_builds`,
`test_qemu_rtic_listener_builds`) verify cross-compilation. Networked E2E test
(`test_qemu_rtic_pubsub_e2e`) launches listener on `tap-qemu1`, talker on `tap-qemu0`,
with zenohd on port 7447 and 5s stabilization delay. Validates "Received 10 messages"
and "Done publishing" in semihosting output. Test is gated by `require_tap_bridge()` +
`require_zenoh_pico_arm()` guards.

**Files**:
- `packages/testing/nros-tests/src/fixtures/binaries.rs` — build helpers
- `packages/testing/nros-tests/tests/emulator.rs` — build + networked tests

## Acceptance Criteria

- [x] `board::init_hardware()` is a public function on at least one board crate
- [x] RTIC talker/listener example compiles for target hardware
- [x] All RTIC examples use `#[local]` for all nano-ros handles (no `#[shared]` locks)
- [x] All RTIC examples use only existing nano-ros API (`spin_once(0)`, `try_recv()`,
      `publish()`, `handle_request()`, `try_handle_get_result()`, `.await`) — no new methods
- [x] `Promise::wait()` limitation is documented; examples use `.await` or `try_recv()` loops
- [x] All tasks run at priority 1 (documented as safety requirement)
- [x] MPS2-AN385 PAC crate compiles for `thumbv7m-none-eabi`
- [ ] RTIC QEMU talker/listener communicate over LAN9118 via zenohd (requires `just build-zenoh-pico-arm`)
- [x] `just quality` passes

## Testing

### Current: Compile Tests + Native Interop

STM32F4 RTIC examples are verified by compile testing (`cargo build --release`). These
target real hardware (Nucleo-F429ZI) and cannot run in QEMU because the MPS2-AN385
machine has a different SoC and board crate.

Native x86 equivalents (in `examples/native/rust/zenoh/rtic-*/`) exercise the identical
RTIC API patterns (`Executor<_, 0, 0>`, `spin_once(0)`, `try_recv()`) and are tested as
separate processes against zenohd. Integration tests in `nano2nano.rs` validate:
- `test_rtic_pattern_communication` — pub/sub (10/10 messages)
- `test_rtic_pattern_service` — service (4/4 calls)
- `test_rtic_pattern_action` — action (goal accepted, 6 feedback messages)

### Implemented: QEMU Runtime Tests (63.6–63.7)

QEMU runtime tests validate RTIC on real Cortex-M3 hardware emulation with networked
zenoh communication. Uses the MPS2-AN385 QEMU machine with LAN9118 Ethernet and
the in-tree PAC from 63.5.

| QEMU Machine | PAC               | Networking | What it validates                                |
|--------------|-------------------|------------|--------------------------------------------------|
| `mps2-an385` | `mps2-an385-pac`  | LAN9118    | RTIC task dispatch + zenoh pub/sub over network  |

Uses `QemuProcess::start_mps2_an385_networked()` with the existing TAP bridge
infrastructure. Two QEMU processes (talker + listener) communicate via zenohd on
the bridge IP (192.0.3.1:7447).

## Notes

- **RTIC v2 async**: All examples use RTIC v2 `async fn` software tasks. Hardware tasks
  (`#[task(binds = TIM2)]`) could be used for periodic net_poll but add complexity
  without clear benefit for the initial integration
- **Target board**: STM32F4 (Nucleo-F429ZI) is the primary target for real-hardware
  examples — `nros-stm32f4` board crate exists, `stm32f4xx-hal` PAC provides RTIC
  dispatcher interrupts, and RTIC has strong STM32F4 community support
- **QEMU test platform**: MPS2-AN385 is chosen over lm3s6965evb because it has LAN9118
  Ethernet (with existing nano-ros driver and test infrastructure), enabling full
  networked RTIC + zenoh integration tests. The lm3s6965evb machine has only Stellaris
  MAC (no smoltcp driver) and would be limited to non-networked testing
- **PAC design**: The `mps2-an385-pac` follows the `lm3s6965` pattern — interrupt
  bindings only, no register APIs, ~150 lines. cortex-m 0.7's blanket impl
  `InterruptNumber for T: Nr` makes it compatible with both RTIC v1 and v2.
  CMSDK interrupt map sourced from ARM's `CMSDK_CM3.h` header and verified against
  QEMU's `mps2.c` (32 external IRQs, Ethernet at IRQ 13)
- **Example naming**: `rtic-` prefix on use-case (e.g., `rtic-talker`) follows the
  existing `async-` prefix convention. RTIC is an execution model variant, not a
  platform or RMW choice, so it stays within the 4-level hierarchy
- **`sync-critical-section`**: Already exists in `nros`. RTIC users should enable this
  feature for RTIC-compatible mutex implementations
- **No `drive_io()` method**: A dedicated `drive_io()` on Executor was considered and
  rejected to stay aligned with ROS 2 API conventions. `spin_once(0)` is the equivalent
  of rclcpp's `spin_some()` — "process available work, don't block"
- **Prerequisites**: FFI reentrancy guards (Phase 61) and event-driven async waking
  (Phase 62) are completed before this phase, so RTIC examples ship with mixed-priority
  support and proper `.await` from day one
- **Reference implementation**: A partial RTIC reference exists at
  `packages/reference/stm32f4-porting/rtic/src/main.rs` (STM32F4, stm32_eth + smoltcp +
  zpico_smoltcp). It demonstrates hardware init and async tasks but uses `#[shared]`
  resources (Phase 63 prescribes `#[local]`). This is a porting reference, not a
  production example

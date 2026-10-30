# Embassy Integration

> **Embassy is not an `nros::main!` framework.** It was one, as an
> out-of-tree seam: an `EmbassyBoardEntry` trait and an `nros::main!()` branch
> a board selected with `[package.metadata.nros.board] framework = "embassy"`.
> No in-tree board had used it since phase-337 W7.a deleted
> `nros-board-embassy-stm32f4`, no fixture or CI cell expanded it, and the code
> it emitted called board methods no trait declared. Issue 2134 retired it
> under RFC-0112 D3, which admits a framework to `nros::main!` codegen only
> with an in-tree board and a CI cell. A board that still declares
> `framework = "embassy"` is refused at expansion with a pointer to this page.

[Embassy](https://embassy.dev) is an async/await framework for embedded Rust.
It owns `fn main`, the spawner and a cooperative executor that polls futures
from one context per priority. nano-ros runs under it the way it runs under
any other host that owns the loop (RFC-0112's Tier 0, "a host drives it"):
your `#[embassy_executor::main]` opens the nano-ros `Executor`, creates the
nodes, and spawns one task that spins it.

## The shape

```rust
#![no_std]
#![no_main]

use core::time::Duration;
use embassy_executor::Spawner;
use embassy_time::Timer;
use nros::prelude::*;
use std_msgs::msg::Int32;

#[embassy_executor::task]
async fn spin_task(mut executor: Executor<'static>) -> ! {
    loop {
        // Never blocks: dispatch whatever is ready, then yield.
        let _ = executor.spin_some(Duration::ZERO);
        Timer::after_millis(1).await;
    }
}

#[embassy_executor::main]
async fn main(spawner: Spawner) {
    // Board bring-up (clocks, network, ...) and backend registration are
    // yours: call your RMW backend's `register()` here, as an embedded
    // board's boot path does.
    let config = ExecutorConfig::new("tcp/192.168.1.1:7447").node_name("listener");
    let mut executor = Executor::open(&config).expect("open executor");
    {
        let mut node = executor.create_node("listener").expect("node");
        node.create_subscription::<Int32, _>("/chatter", |msg: &Int32| {
            let _ = msg.data; // runs inside `spin_some`, on the spin task
        })
        .expect("subscription");
    }
    spawner.spawn(spin_task(executor)).unwrap();
}
```

Adapt the locator and the backend registration to your board; the
[STM32F4 out-of-tree example](../porting/stm32f4-out-of-tree.md) shows the
board-side half.

## What this does not give you yet

- **Sleeping until there is work.** The loop polls every millisecond when
  idle. The timer half exists: `Executor::next_deadline_us()` says how long
  until the next timer or backend deadline, `None` when nothing is scheduled
  (issue 2143). The "work arrived" half is `Executor::set_ready_notifier()`
  (issue 2144), but it needs `alloc` and a backend with a live async wake
  (`ready_notifier_supported()`), and no example wires it to an Embassy
  signal yet. With both, the task can wait on
  `select(notified, Timer::after_micros(next_deadline))` and sleep. Do not use
  `Executor::spin_async()` here: it no longer blocks the Embassy thread
  inside a poll (issue 2145), but it still wakes itself on every poll, so
  it is a busy loop that never lets the Embassy executor sleep.
- **Deferred dispatch across tasks.** Callbacks run inline, inside
  `spin_some`, unless you mark them with `Executor::set_deferred`; then the
  spin queues them and calls your `set_defer_notifier` function, and
  `run_deferred()` runs them (see
  [Deferring callbacks without the macros](../internals/dispatch-strategy.md#deferring-callbacks-without-the-macros)).
  Both calls need `&mut Executor`, so with the executor moved into the spin
  task the drain runs in that task too. To run work in ANOTHER task, keep
  callbacks short and have them signal an `embassy_sync` channel that task
  drains.
- **`system.toml` assembly.** Nodes are created by hand. Issue 2150 splits the
  model-driven assembly out of `nros::main!` so a host-driven entry can reuse
  it.

## See also

- [Dispatch Strategy (internals)](../internals/dispatch-strategy.md)
- [RTIC Integration](./rtic-integration.md) — the framework that IS an
  `nros::main!` entry shape, with an in-tree board.
- RFC-0112 (`docs/design/0112-framework-support-tiers.md`) — why frameworks
  are integrated through one host-driven contract.

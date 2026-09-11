# Embassy Integration

> **Status: NO in-tree Embassy board — read before relying on this chapter.**
> `nros-board-embassy-stm32f4` was the only one, and phase-337 W7.a deleted it:
> it was registry-labelled `scaffold` (issue 0248 — entry methods `todo!()`, a
> Deferred image signalling into a channel nothing drained), and RFC-0064's rule
> is that a board with no Runtime cell is not a support promise this repo can
> make. What survives is the **seam**, which is the part an integrator needs:
> the `EmbassyBoardEntry` trait in `nros-platform` and the `nros::main!()`
> Embassy emit branch, plus `nros ws check`'s Deferred-dispatch lint, which
> routes on the board crate's `framework = "embassy"` metadata.
>
> One consequence for this chapter: every path below names a board YOU write —
> see [Worked Example — STM32F4 Out of
> Tree](../porting/stm32f4-out-of-tree.md) for the board-crate shape. Reaching
> the Embassy emit from out of tree does work (issue 0415, resolved by
> phase-346): the board crate declares
> `[package.metadata.nros.board] framework = "embassy"`, and a one-line
> `build.rs` calling `nros_build::emit_board_framework()` hands that to macro
> expansion. The in-tree board table covers in-tree boards only, and maps none
> of them to Embassy. (The RTIC twin of this integration is complete and
> runtime-tested on `mps2-an385` — see
> [RTIC Integration](rtic-integration.md).)

[Embassy](https://embassy.dev) is an async/await framework for embedded
Rust, built around a cooperative executor that polls futures from a
single context (per priority tier). nano-ros runs on top of Embassy by
letting the framework own `fn main`, the spawner, and the async
executor, while nano-ros contributes one `__nros_spin_task` async fn
and (optionally) one `__nros_dispatch_task` async fn.

This chapter is the user-facing tutorial for that integration. For the
underlying design — why per-Node dispatch strategies, why tags instead
of closures, why `on_callback` stays sync even on async runtimes — see
the sibling internals page
[Dispatch Strategy](../internals/dispatch-strategy.md).

## What Embassy buys you and where nano-ros fits

Embassy is an alternative to RTOS-style preemptive tasks: every
"task" is an async fn driven by `embassy_executor::Spawner::spawn`.
Tasks yield at `.await` points; there's no scheduler tick, no context
switch overhead beyond a future poll. For nano-ros that means:

- A natural place for I/O-driven background work (SPI bus servicing,
  GPIO debounce, sensor frame parsing) that yields at every `.await`
  on a `embassy_time::Timer` or `embassy_stm32::spi::Spi::transfer`.
- A natural place to spawn downstream work *from inside* a nano-ros
  callback — the "spawn-from-sync escape" below.
- Cooperative scheduling means a Node `on_callback` runs to
  completion before the executor polls anything else at the same
  priority. Keep handlers short, hand off long work to a spawned
  task.

**Pattern A** is hand-written `#[embassy_executor::main]`, hand-written
`zenoh_poll_task`, hand-written `publisher_task`. It works, but it is ~150
lines and each author re-derives the spawn topology. The 216.C.4 board-entry
path collapses it to:

```rust
// File: src/main.rs
#![no_std]
#![no_main]

use defmt_rtt as _;
use panic_halt as _;

nros::main!();
```

The proc-macro reads the image's board out of `system.toml` —
`[image.<id>] board = "<your-embassy-board>"` — finds that board declaring
`framework = "embassy"`, and expands into a full
`#[embassy_executor::main] async fn main(spawner: Spawner)` including the
spin task spawn, the dispatch task spawn, and the `run_plan` registration
call.

The framework is a property of the **board**, not something you spell in a
manifest. For a board in the in-tree table the name alone decides it; for
your own board crate the declaration travels through the `build.rs` line in
the banner above.

## Why sync `on_callback` even on Embassy

A natural-feeling Embassy API would make `ExecutableNode::on_callback`
an `async fn`. We don't — for two reasons:

1. **The no-alloc contract.** Async fns desugar to anonymous future
   types; storing them generically in the runtime requires either
   boxing (`Box<dyn Future>` → `alloc` dependency) or const-generic
   GAT plumbing through every trait. Both add cost without buying
   anything `Spawner::spawn` doesn't already give us.
2. **Framework-task routing.** The runtime already dispatches
   callbacks from a framework-owned task (`__nros_dispatch_task` on
   Embassy, `__nros_dispatch` on RTIC). The Node author can spawn
   their own async task from inside the sync `on_callback`; that
   task runs under the same executor with no extra plumbing.

So `on_callback` keeps the same callback-token signature as RTIC, POSIX,
and every other backend. The escape for "I need to await something" is the
spawn-from-sync pattern below.

`AsyncNode` (an async-on-callback trait via RPITIT) is reserved as a
design slot — see [When to wait for
`AsyncNode`](#when-to-wait-for-asyncnode) at the end of this chapter.

## Where the pieces live

The shape is identical to RTIC (the [3-pkg-role
taxonomy](./component-and-entry-pkg.md), per
`docs/design/0024-multi-node-workspace-layout.md` §11): node packages plus
a bringup, no root build file, and **no Entry package to write** — the
entry is generated per `[image.*]` under `build/`.

```text
my_embassy_robot/
├── .colcon_workspace                    # marks the root; no root build file
└── src/
    ├── listener_pkg/                    # Node pkg — board-agnostic
    │   ├── package.xml
    │   ├── Cargo.toml
    │   └── src/lib.rs                   # impl Node for Listener + nros::node!(Listener)
    └── demo_bringup/                    # Bringup pkg — no code
        ├── system.toml
        └── launch/
```

The Node pkg stays board-agnostic — `listener_pkg/` from the RTIC
chapter could be deployed under Embassy by changing one word in
`system.toml`. That's the point of the split.

### The `system.toml` that makes it Embassy

```toml
[system]
name      = "my_embassy_robot"
rmw       = "zenoh"
domain_id = 0

[[component]]
pkg      = "listener_pkg"
class    = "listener_pkg::Listener"
name     = "listener"
dispatch = "deferred"

[image.embassy]
board   = "my-embassy-board"
locator = "tcp/192.168.1.10:7447"
```

`board` is the only line that says Embassy. The RMW, the domain and the
network identity live beside it, and nothing about any of them appears in a
`Cargo.toml`.

A **single-package** Embassy project — one crate holding `system.toml`,
`src/lib.rs` and `src/main.rs` — works too; it is the shape every in-tree
RTIC example takes. Such a package is its own entry, so it does still name
the board crate in its own `[dependencies]`, and that dependency is not
generated yet: change `board` there and you must change the board crate
beside it, or the build fails in your own `main.rs` with
`cannot find <board crate> in the crate root`. That is
[issue 1305](https://github.com/NEWSLabNTU/nano-ros/blob/main/docs/issues/1305-single-package-board-crate-dep-not-generated.md).

## `DispatchStrategy::Deferred` is the common case

Every callback-driven Embassy Node should declare
`DispatchStrategy::Deferred`:

```rust
impl Node for Listener {
    const NAME: &'static str = "listener";
    const DISPATCH: DispatchStrategy = DispatchStrategy::Deferred;
    // ...
}
```

Deferred means: the spin task pushes signaled callbacks into an
`embassy_sync::channel::Channel<NoopRawMutex, _, CHANNEL_CAPACITY>`
(default capacity 32). A separate `__nros_dispatch_task` awaits the
channel receive end and routes each callback to the right Node's
`on_callback`. Both tasks run cooperatively on the same Embassy
executor; you can spawn your own tasks alongside them.

Inline is allowed but unusual on Embassy:

- The `nros check` lint (Phase 216.D.1) emits a warning for
  `framework = "embassy"` with `DispatchStrategy::Inline` and
  suggests switching to Deferred — running callbacks inline on
  Embassy ties them to the spin task's poll point, which makes the
  scheduling cost-vs-benefit confusing.
- If your Node is genuinely pub-only (no subscriptions, no
  services, no actions), Inline is still fine — the Inline path
  simply never enters `on_callback`. The warning above does not
  apply for pure-publisher Nodes.

## The spawn-from-sync escape

Real Embassy Nodes need to do async work downstream from a callback:
write to an SPI bus, send a UART frame, poll a sensor with timeout.
The pattern is:

1. Hold an `embassy_executor::Spawner` on `Self::State`.
2. From inside the sync `on_callback`, call
   `state.spawner.spawn(handle_downstream(msg)).unwrap()`.
3. The downstream `#[embassy_executor::task] async fn`
   handles the await-heavy work.

### Worked example — Listener that writes received data to SPI

```rust
// File: src/listener_pkg/src/lib.rs
#![no_std]

use embassy_executor::Spawner;
use nros::prelude::*;
use std_msgs::msg::Int32;

pub struct Listener;

pub struct ListenerState {
    sub_chatter: SubscriptionTag,
    spawner: Spawner,
}

impl Node for Listener {
    const NAME: &'static str = "listener";
    const DISPATCH: DispatchStrategy = DispatchStrategy::Deferred;

    fn register(ctx: &mut NodeContext<'_>) -> NodeResult<()> {
        let _tag = ctx.create_subscription_static::<Int32>("/chatter")?;
        Ok(())
    }
}

impl ExecutableNode for Listener {
    type State = ListenerState;

    fn init() -> Self::State {
        ListenerState {
            sub_chatter: SubscriptionTag::placeholder(),
            // The macro-emitted glue populates the Spawner from the
            // EmbassyBoardEntry init hook; until then, hold a sentinel
            // that's overwritten before the first dispatch.
            spawner: Spawner::for_current_executor(),
        }
    }

    fn on_callback(
        state: &mut Self::State,
        cb: Callback<'_>,
        ctx: &mut CallbackCtx<'_>,
    ) {
        if state.sub_chatter == cb {
            let msg: Int32 = ctx.downcast().unwrap();
            // Spawn the async work and return immediately. The
            // dispatch task stays unblocked.
            state.spawner.spawn(handle_msg(msg)).unwrap();
        }
    }
}

#[embassy_executor::task(pool_size = 4)]
async fn handle_msg(msg: Int32) {
    // Pretend we have an SPI handle stashed somewhere accessible.
    // The real wiring depends on your peripheral-access pattern;
    // the point is `.await` is free here, even though on_callback
    // is sync.
    defmt::info!("handling /chatter sample: {}", msg.data);
    embassy_time::Timer::after_millis(5).await;
    // spi.write(&msg.data.to_le_bytes()).await.unwrap();
}

nros::node!(Listener);
```

Two things to note:

- **Spawner is `Copy`.** Holding a `Spawner` on `Self::State` adds no
  runtime cost beyond an integer copy. You can clone it freely
  across multiple `on_callback` calls.
- **`pool_size` matters.** `#[embassy_executor::task(pool_size = N)]`
  allocates `N` static slots. If you spawn faster than the spawned
  tasks complete, `spawn(...)` returns `Err(SpawnError::Busy)` —
  handle it (drop the message, log a warning, etc.). The
  default is `pool_size = 1`.

### When the spawn pool fills up

Two patterns for backpressure:

1. **Drop on full.** Treat the spawned task as best-effort. If the
   `spawn` returns an error, log it and continue.
2. **Channel-based queue.** Pre-spawn one long-lived
   `#[embassy_executor::task] async fn` that holds an
   `embassy_sync::channel::Channel` receive end. The `on_callback`
   pushes into the channel's send end (drop on full or block — your
   choice). Lets you control queue depth independently of `pool_size`.

The right pick depends on whether dropped messages are tolerable for
your workload; nano-ros doesn't impose either.

## When to wait for `AsyncNode`

The spawn-from-sync escape covers every case we know of today. But if
you find yourself writing a lot of one-off pool-of-one spawn dances —
each callback spawning a task whose body is a single `.await` — that's
a smell, and we'd want to know.

The design slot for direct async callbacks is `AsyncNode` (Phase
216.E.2):

```rust
// Design sketch — NOT shipping today.
pub trait AsyncNode: 'static {
    const NAME: &'static str;
    const DISPATCH: DispatchStrategy = DispatchStrategy::Deferred;

    fn register(ctx: &mut NodeContext<'_>) -> NodeResult<()>;
    async fn on_callback(
        &mut self,
        callback: AsyncCallbackToken,
        ctx: CallbackCtx,
    );
}
```

It would compile only on Embassy targets (RTIC has no async runtime
to drive RPITIT futures into; POSIX + RTOS don't either), and the
`nros::node!()` macro would emit a separate
`__nros_node_<pkg>_on_callback_async` ABI symbol the Embassy dispatch
task picks up. **216.E.2 lands only if real usage shows
spawn-from-sync is consistently painful.** If your application is
hitting that case, file an issue with the call pattern that's
prompting it.

Until then: spawn from sync. It's two lines per callback and stays
fully no-alloc.

## See also

- [Dispatch Strategy (internals)](../internals/dispatch-strategy.md) —
  the trichotomy and the per-Node-vs-per-callback rationale.
- [RTIC Integration](./rtic-integration.md) — the interrupt-driven
  sibling of this chapter.
- [Role reference](./component-and-entry-pkg.md) —
  the 3-pkg-role taxonomy in full.
- [Scheduling Models](../internals/scheduling-models.md) — the Embassy
  cooperative model alongside RTIC, RTOS, and POSIX.

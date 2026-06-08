# RTIC Integration

RTIC ([Real-Time Interrupt-driven Concurrency](https://rtic.rs)) is a
concurrency framework for ARM Cortex-M that compiles tasks directly to
hardware interrupt handlers — no RTOS kernel, no task control blocks, no
software scheduler. nano-ros runs on top of RTIC by letting the framework
own `fn main`, the scheduler, and the dispatchers, while nano-ros
contributes one spin task and (optionally) one callback-dispatch task.

This chapter is the user-facing tutorial for that integration. For the
underlying design — why per-Node dispatch strategies, why tags instead of
closures, why no `AsyncNode` trait yet — see the sibling internals page
[Dispatch Strategy](../internals/dispatch-strategy.md).

## When to reach for `nros::main!()` instead of Pattern A

The current in-tree RTIC examples (e.g.
`examples/stm32f4/rust/talker-rtic/`) use the **Pattern A escape
hatch**: hand-written `#[rtic::app]`, manual `Executor::open` inside
`#[init]`, manual `spin_once(0)` polling task. It looks like this:

```rust
// File: examples/stm32f4/rust/talker-rtic/src/main.rs (today, pre-216.B.5)
#[rtic::app(device = stm32f4xx_hal::pac, dispatchers = [USART1, USART2])]
mod app {
    #[init]
    fn init(cx: init::Context) -> (Shared, Local) {
        let syst = nros_board_stm32f4::init_hardware(&config, cx.device, cx.core);
        nros_rmw_zenoh::register().expect("Failed to register RMW backend");
        let mut executor = Executor::open(&exec_config).unwrap();
        let mut node = executor.create_node("talker").unwrap();
        let publisher = node.create_publisher::<Int32>("/chatter").unwrap();
        net_poll::spawn().unwrap();
        publish::spawn().unwrap();
        (Shared {}, Local { executor, publisher })
    }

    #[task(local = [executor], priority = 1)]
    async fn net_poll(cx: net_poll::Context) {
        loop {
            cx.local.executor.spin_once(core::time::Duration::from_millis(0));
            Mono::delay(10.millis()).await;
        }
    }
}
```

That pattern works, but it's ~90 lines of glue per binary, and it puts
the burden of getting RTIC + nros + the dispatcher list right on every
example author. Phase 216.B.5 collapses it to one line:

```rust
// File: examples/stm32f4/rust/talker-rtic/src/main.rs (post-216.B.5)
#![no_std]
#![no_main]

use defmt_rtt as _;
use panic_probe as _;

nros::main!();
```

The `nros::main!()` proc-macro reads `[package.metadata.nros.entry]
deploy = "rtic-stm32f4"` from the Entry pkg's `Cargo.toml`, sees that
the board's metadata declares `framework = "rtic"`, and emits a full
`#[rtic::app]` module — including `#[init]`, `__nros_spin`, and (if any
deployed Node declares `DispatchStrategy::Deferred`) `__nros_dispatch`.

Pick `nros::main!()` whenever:

- You want a one-line `main.rs` and don't need custom RTIC tasks.
- Your Node logic is portable across boards (the Node pkg is
  framework-agnostic; only the Entry pkg picks RTIC).
- You're happy with the default dispatcher list from the board crate.

Keep Pattern A when:

- You need fine-grained control of dispatcher priorities, monotonic
  setup, or hand-tuned `#[shared]` state.
- You're shipping a one-off bring-up binary and don't want the
  Node-pkg / Entry-pkg split overhead.

Both paths stay supported. The escape hatch is the "I want full
control" path; `nros::main!()` is the ergonomic path on top.

## The three pkg roles

A nano-ros RTIC workspace is three packages (the [3-pkg-role
taxonomy](../user-guide/component-and-entry-pkg.md), per
`docs/design/multi-node-workspace-layout.md` §11):

```text
my_rtic_robot/
├── Cargo.toml                         # [workspace] members = [...]
└── src/
    ├── talker_pkg/                    # Node pkg — board-agnostic
    │   ├── package.xml
    │   ├── Cargo.toml
    │   └── src/lib.rs                 # impl Node for Talker + nros::node!(Talker)
    └── talker_entry/                  # Entry pkg — picks RTIC board
        ├── package.xml
        ├── Cargo.toml                 # [package.metadata.nros.entry] deploy = "rtic-stm32f4"
        └── src/main.rs                # nros::main!();
```

- **Node pkg** — declares what the node does (publishers,
  subscriptions, services, actions). No `main`, no `#[rtic::app]`, no
  board choice. Builds as `rlib + staticlib` and gets linked into one
  or more Entry pkgs.
- **Entry pkg** — picks the board crate (`nros-board-rtic-stm32f4`),
  pins the deploy target, and runs `nros::main!();`.
- **Bringup pkg** — optional, only when ≥2 Entry pkgs share the same
  `launch/*.launch.xml` topology. Skipped here because we have one
  binary.

The split exists so the same `talker_pkg/` can be deployed under RTIC
on STM32F4, under FreeRTOS on QEMU, and under POSIX on a Linux host
without any per-target Node-pkg fork.

### A minimal Node pkg — pub-only Talker

A pub-only Node declares `DispatchStrategy::Inline` (the default) — it
publishes from its own RTIC task and never enters `on_callback`.

```rust
// File: src/talker_pkg/src/lib.rs
#![no_std]

use core::time::Duration;
use nros::prelude::*;
use std_msgs::msg::Int32;

pub struct Talker;

pub struct TalkerState {
    publisher: Publisher<Int32>,
    counter: i32,
}

impl Node for Talker {
    const NAME: &'static str = "talker";
    // Inline is the default; spelled out here for clarity.
    const DISPATCH: DispatchStrategy = DispatchStrategy::Inline;

    fn register(ctx: &mut NodeContext<'_>) -> NodeResult<()> {
        ctx.create_publisher::<Int32>("/chatter")?;
        Ok(())
    }
}

impl ExecutableNode for Talker {
    type State = TalkerState;

    fn init() -> Self::State {
        TalkerState { publisher: Publisher::placeholder(), counter: 0 }
    }

    fn tick(state: &mut Self::State, _ctx: &mut TickCtx<'_>) {
        state.counter = state.counter.wrapping_add(1);
        let _ = state.publisher.publish(&Int32 { data: state.counter });
    }

    fn tick_period(_state: &Self::State) -> Option<Duration> {
        Some(Duration::from_millis(1000))
    }
}

nros::node!(Talker);
```

```toml
# File: src/talker_pkg/Cargo.toml
[package]
name    = "talker_pkg"
version = "0.1.0"
edition = "2024"

[lib]
crate-type = ["rlib", "staticlib"]

[dependencies]
nros      = { workspace = true, default-features = false }
std_msgs  = { workspace = true }

[package.metadata.nros.node]
class = "talker_pkg::Talker"
```

### The Entry pkg

```toml
# File: src/talker_entry/Cargo.toml
[package]
name    = "talker_entry"
version = "0.1.0"
edition = "2024"

[[bin]]
name = "talker_entry"
path = "src/main.rs"

[dependencies]
nros                       = { workspace = true, default-features = false }
nros-board-rtic-stm32f4    = { workspace = true }
talker_pkg                 = { path = "../talker_pkg" }

[package.metadata.nros.entry]
deploy = "rtic-stm32f4"

[package.metadata.nros.deploy.rtic-stm32f4]
board     = "rtic-stm32f4"
rmw       = "zenoh"
domain_id = 0
locator   = "tcp/192.168.1.10:7447"
```

```rust
// File: src/talker_entry/src/main.rs
#![no_std]
#![no_main]

use defmt_rtt as _;
use panic_probe as _;

nros::main!();
```

That's the whole Entry pkg. The proc-macro reads `deploy =
"rtic-stm32f4"`, looks up the board crate's `framework = "rtic"`
metadata, and expands into a `#[rtic::app(device = ::nros_board_rtic_stm32f4::pac, dispatchers = [USART1, USART2])]` module with auto-generated `#[init]`,
`__nros_spin`, and per-Node state slots.

## `DispatchStrategy::Inline` vs `Deferred`

A Node pkg declares `DispatchStrategy` via the `Node::DISPATCH` const.
Two variants matter today; `FromIsr` is reserved as a design slot (see
[`DispatchStrategy::FromIsr`](#dispatchstrategyfromisr-not-yet)).

### `Inline` — pub-only or `tick`-only Nodes

```rust
impl Node for Talker {
    const NAME: &'static str = "talker";
    const DISPATCH: DispatchStrategy = DispatchStrategy::Inline;
    // ...
}
```

Inline means: callbacks (if any) fire from the executor's spin loop —
the same RTIC task that polls the network transport. On RTIC that's
`__nros_spin` running at `priority = 1`.

Pick Inline when:

- The Node has no subscriptions, no service handlers, no action
  handlers. Its only output is `Publisher::publish` calls from `tick`
  or from a custom RTIC task you spawn yourself.
- The Node has subscriptions, but the per-message work is so cheap
  (microseconds, no locks, no shared-state touches) that running it
  inline with the spin loop is acceptable.

The Talker above is the canonical Inline shape: it publishes from
`tick` at 1 Hz, never receives anything, and never blocks the spin
loop.

### `Deferred` — callback-driven Nodes

```rust
impl Node for Listener {
    const NAME: &'static str = "listener";
    const DISPATCH: DispatchStrategy = DispatchStrategy::Deferred;
    // ...
}
```

Deferred means: when a callback arrives, the spin loop enqueues a callback
token plus context into a `heapless::spsc::Queue` and returns immediately.
A separate `__nros_dispatch` RTIC task (typically `priority = 2`, one
above spin) drains the queue and calls `ExecutableNode::on_callback` from
its own task context.

Pick Deferred when:

- The Node's `on_callback` handler must hold an RTIC lock on a
  `#[shared]` resource — running it from the spin task would force
  every other dispatcher at the spin's priority to wait on the
  lock.
- The handler does non-trivial work (parses, integrates, posts to a
  hardware peripheral) and you want it scheduled independently from
  network polling.
- The Node sits at a different priority tier than the spin task.

### The tag-based registration API for Deferred Nodes

Deferred Nodes can't use the closure form `ctx.create_subscription("/chatter", |msg| { ... })` —
the closure captures state by value, and a no-alloc framework
runtime has no place to store an unknown closure type. Instead, you
register with `_static` and dispatch via tag match in `on_callback`:

```rust
// File: src/listener_pkg/src/lib.rs
#![no_std]

use nros::prelude::*;
use std_msgs::msg::Int32;

pub struct Listener;

pub struct ListenerState {
    sub_chatter: SubscriptionTag,
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
        // The macro-emitted glue overwrites the placeholder with the
        // real tag at register time.
        ListenerState { sub_chatter: SubscriptionTag::placeholder() }
    }

    fn on_callback(
        state: &mut Self::State,
        cb: CallbackId<'_>,
        ctx: &mut CallbackCtx<'_>,
    ) {
        if state.sub_chatter == cb {
            let msg: Int32 = ctx.downcast().unwrap();
            defmt::info!("Received: {}", msg.data);
        }
    }
}

nros::node!(Listener);
```

The same shape applies to services (`ServiceTag` /
`create_service_static`) and actions (`ActionTag` /
`create_action_static`). The macro lint (216.A.6) rejects mixing
closure registration into a Deferred Node — the error spans the
offending call and suggests switching to the `_static` form.

## The custom-task escape — `nros::main!(custom_tasks = ...)`

> **Coming in Phase 216.B.4.** The syntax below is the locked design;
> it lands after 216.B.3 (the RTIC routing branch of the macro) is in
> tree.

Real RTIC applications don't only have nano-ros tasks. You want a
dedicated `my_adc` task to poll an ADC into a `#[shared]` buffer, or a
`my_ui` task to drive an OLED. The `custom_tasks = [...]` form of
`nros::main!()` folds your extra task fns into the generated
`#[rtic::app]` module:

```rust
// File: src/talker_entry/src/main.rs (with custom tasks)
#![no_std]
#![no_main]

use defmt_rtt as _;
use panic_probe as _;

nros::main!(custom_tasks = [my_adc, my_ui]);

#[rtic_task(priority = 3, shared = [adc_data])]
async fn my_adc(mut ctx: my_adc::Context) {
    loop {
        let sample = read_adc();
        ctx.shared.adc_data.lock(|d| *d = sample);
        Mono::delay(10.millis()).await;
    }
}

#[rtic_task(priority = 2)]
async fn my_ui(_ctx: my_ui::Context) {
    loop {
        draw_screen();
        Mono::delay(33.millis()).await;
    }
}
```

The proc-macro extracts the user task tokens verbatim, splices them
into the generated `mod __nros_app`, and adds their dispatchers to the
RTIC `dispatchers = [...]` list. Signatures + attributes + priorities
are preserved.

Custom tasks can interact with nano-ros via the `#[shared]` resources
the macro exposes — typically `executor` (the spin executor) for raw
publisher access, or a Node's state slot for direct handler calls.
Specifics will be locked when 216.B.4 lands.

## `DispatchStrategy::FromIsr` — not yet

The third variant of `DispatchStrategy` is reserved for callbacks that
fire directly from an ISR handler (e.g. a timer pulse triggering a
publish without a scheduler hop). This is **a design slot only**;
the implementation is deferred to Phase 216.E.1.

Landing it requires:

- A reentrancy audit of the dispatch path.
- A lock-free SPSC variant tolerant of ISR-priority producers.
- A per-Node `#[isr_safe]` proof contract.

Until that work lands, `nros check` (Phase 216.D.1) rejects
`DispatchStrategy::FromIsr` deployments with a clear diagnostic.

## See also

- [Dispatch Strategy (internals)](../internals/dispatch-strategy.md) —
  the trichotomy and the per-Node-vs-per-callback rationale.
- [Embassy Integration](./embassy-integration.md) — the async sibling
  of this chapter.
- [Role reference](./component-and-entry-pkg.md) —
  the 3-pkg-role taxonomy in full.
- [Scheduling Models](../internals/scheduling-models.md#rtic-arm-cortex-m) —
  the RTIC scheduling model in real-time-systems terms.

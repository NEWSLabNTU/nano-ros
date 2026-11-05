# RTIC Integration

> **For implementers.** This page documents a feature with no support-level
> evidence row: RTIC is the `rtic` feature of the `nros-board-mps2-an385`
> crate, it has no row of its own in `packages/boards/board-support.toml`, and
> no probe track or book run exercises it. So the nano-ros Book does not teach
> it (RFC-0114 D10: the book claims only what the registry backs). What
> follows is the integration as built, for whoever maintains it or gives it a
> registry row.

RTIC ([Real-Time Interrupt-driven Concurrency](https://rtic.rs)) is a
concurrency framework for ARM Cortex-M that compiles tasks directly to
hardware interrupt handlers — no RTOS kernel, no task control blocks, no
software scheduler. nano-ros runs on top of RTIC by letting the framework
own `fn main`, the scheduler, and the dispatchers, while nano-ros
contributes one spin task and (optionally) one callback-dispatch task.

For the underlying design — why per-Node dispatch strategies, why tags
instead of closures, why no `AsyncNode` trait yet — see the user book's
[Dispatch Strategy](../../realtime/dispatch-strategy.html).

## The board key chooses RTIC

An RTIC program is a workspace like any other: a node package and the bringup
that runs it. The scaffold writes both:

```bash
nros setup mps2-an385-baremetal --rmw zenoh   # the same board; RTIC needs nothing extra
nros new rtic_ws --board rtic-mps2-an385 --template talker
cd rtic_ws
nros run rtic_node
```

```text
rtic_ws/
└── src/
    ├── rtic_node/            # the node: package.xml, Cargo.toml, src/lib.rs
    └── rtic_node_bringup/    # package.xml and system.toml: the board, so RTIC
```

`nros run` builds the image, boots it in QEMU and starts the router, as in
[First Project](../../get-started/first-project.html). The bringup's `system.toml`
is the only place RTIC is named:

```toml
# src/rtic_node_bringup/system.toml
[system]
name = "rtic_node"
rmw = "zenoh"
domain_id = 0

[[component]]
pkg = "rtic_node"
class = "rtic_node::Talker"
name = "rtic_node"

[image.baremetal]
board = "rtic-mps2-an385"
```

The framework is a property of the **board key**, never something you spell
in a manifest, and there is no image key that picks it. One board crate serves
the MPS2-AN385 under two keys:

| `[image.<id>] board =` | Entry the build generates |
|---|---|
| `"mps2-an385-baremetal"` | the plain entry: an owned spin loop in `fn main` |
| `"rtic-mps2-an385"` | the RTIC entry: a `#[rtic::app]` module |

`zephyr` resolves to Zephyr, `esp32-qemu` and `esp32-c3-baremetal` to the
ESP32 shape, and every other board — Linux, FreeRTOS, NuttX, ThreadX, non-RTIC
bare-metal — to the default owned-spin `fn main`. An out-of-tree board crate
declares its own framework in `[package.metadata.nros.board]`. So moving a
program between RTIC and a plain spin loop is a board change, not a code
change: change `board` and nothing else, not the node source and not a build
command.

## Where the pieces live

```text
rtic_ws/
└── src/
    ├── rtic_node/                     # Node pkg — board-agnostic
    │   ├── package.xml
    │   ├── Cargo.toml
    │   └── src/lib.rs                 # impl Component for Talker + nros::node!(Talker)
    └── rtic_node_bringup/             # Bringup pkg — no code
        ├── package.xml
        └── system.toml
```

- **Node pkg** — declares what the node does (publishers,
  subscriptions, services, actions). No `main`, no `#[rtic::app]`, no
  board choice. It is a library, linked into the generated entry.
- **Bringup pkg** — holds `system.toml` (and launch files, once it has
  more than one node). This is where the board, and therefore RTIC, is
  chosen.
- **Entry** — generated under `build/`, not authored. It is the
  `#[rtic::app(device = mps2_an385_pac, dispatchers = [UARTRX0, UARTTX0])]`
  module, with `#[init]`, the `__nros_spin` task, per-Node state slots and,
  when any node in the image declares `DispatchStrategy::Deferred`, the
  `__nros_dispatch` task. It depends on the board crate with its `rtic`
  feature, the RMW backend, `rtic` and the PAC, so the node package names none
  of them.

The split exists so the same `rtic_node/` can be deployed under RTIC
on bare-metal Cortex-M, under FreeRTOS on QEMU, and under POSIX on a Linux host
without any per-target Node-pkg fork.

### A minimal Node pkg — pub-only Talker

A pub-only Node declares `DispatchStrategy::Inline` (the default) — it
publishes from its own RTIC task and never enters `on_callback`.

```rust
// File: src/rtic_node/src/lib.rs
#![no_std]

use core::time::Duration;
use nros::prelude::*;
use std_msgs::msg::Int32;

pub struct Talker;

pub struct TalkerState {
    publisher: Publisher<Int32>,
    counter: i32,
}

impl Component for Talker {
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
# File: src/rtic_node/Cargo.toml
[package]
name    = "rtic_node"
version = "0.1.0"
edition = "2024"

[lib]
path = "src/lib.rs"

[package.metadata.nros.node]
class = "rtic_node::Talker"
name  = "rtic_node"

[dependencies]
nros     = { version = "*", default-features = false }
std_msgs = { path = "../../generated/std_msgs", default-features = false }  # nros-managed
```

Dependencies and the class registration, nothing else. The `std_msgs` row is
not even yours: the build writes it from `<depend>std_msgs</depend>` in
`package.xml` ([The manifests](../../build/growing-your-workspace.html#the-manifests)).
No board crate, no `rtic`, no PAC: those are the generated entry's.

## `DispatchStrategy::Inline` vs `Deferred`

A Node pkg declares `DispatchStrategy` via the `Node::DISPATCH` const.
Two variants matter today; `FromIsr` is reserved as a design slot (see
[`DispatchStrategy::FromIsr`](#dispatchstrategyfromisr-not-yet)).

### `Inline` — pub-only or `tick`-only Nodes

```rust
impl Component for Talker {
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
impl Component for Listener {
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

impl Component for Listener {
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
        cb: Callback<'_>,
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
`create_action_static`). The macro rejects mixing
closure registration into a Deferred Node — the error spans the
offending call and suggests switching to the `_static` form.

## Your own RTIC tasks: the escapes, smallest first

Real RTIC applications don't only have nano-ros tasks. You want a
dedicated `my_adc` task to poll an ADC into a `#[shared]` buffer, or a
`my_ui` task to drive an OLED, or another dispatcher priority. Take the
smallest step that does what you need; the generated entry stays generated
through the first three:

1. **An `[image.*]` key** — `panic`, `profile`, `features`: anything the
   image already declares. See
   [Customising an entry](../../build/growing-your-workspace.html#customising-an-entry).
2. **`before_boot`** — not on RTIC. RTIC's `#[init]` owns the first statement,
   so the build refuses `before_boot` on an RTIC image; put pre-boot work in
   the board package (step 3) or in your own `#[init]` (step 4).
3. **A board package** — hardware setup every image on the board needs
   (clocks, DMA, the monotonic) and the dispatcher list belong in the board's
   `init_hardware` and its RTIC support, in a
   [board package](../../extending/board-trait.html).
4. **`nros materialize <bringup>:<image>`** — the last resort, for hand-written tasks.

`nros materialize` writes the image's generated entry into
`src/<image>_entry/` as a package you own, and from then on `nros build` uses
it and does not regenerate it:

```bash
nros materialize baremetal       # writes src/baremetal_entry/
```

Its crate root holds the same `nros::main!(…)` call the build generated. Add
your tasks to it with the `custom_tasks = [...]` argument, which folds your
task fns into the generated `#[rtic::app]` module:

```rust
// File: src/baremetal_entry/src/main.rs (after `nros materialize`)
#![no_std]
#![no_main]

use panic_semihosting as _;

nros::main!(launch = "rtic_node_bringup", custom_tasks = [my_adc, my_ui]);

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
are preserved. Because the macro still reads the launch file, a node you add
to the bringup still reaches the materialized entry.

Custom tasks interact with nano-ros through the `#[shared]` resources
the macro exposes — typically `executor` (the spin executor) for raw
publisher access, or a Node's state slot for direct handler calls. The
exact resource names are not settled yet.

**Replacing the macro.** In the materialized entry you can also drop
`nros::main!` and write the `#[rtic::app]` module yourself: manual
`Executor::open` inside `#[init]` and a manual `spin_once(0)` polling task.
You then own the boot, node registration, executor sizing and dispatcher list,
and keep them in step with the bringup by hand. It looks like this:

```rust
// src/baremetal_entry/src/main.rs, written by hand
#[rtic::app(device = mps2_an385_pac, dispatchers = [UARTRX0, UARTTX0])]
mod app {
    #[init]
    fn init(cx: init::Context) -> (Shared, Local) {
        let syst = nros_board_mps2_an385::init_hardware(&config, cx.device, cx.core);
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

That is about 90 lines of glue for one node, and it is worth it only for
fine-grained control the steps above cannot give: hand-tuned dispatcher
priorities, monotonic setup, or `#[shared]` state of your own. Deleting
`src/baremetal_entry/` returns the image to generation.

## `DispatchStrategy::FromIsr` — not yet

The third variant of `DispatchStrategy` is reserved for callbacks that
fire directly from an ISR handler (e.g. a timer pulse triggering a
publish without a scheduler hop). This is **a design slot only**;
the implementation is deferred.

Landing it requires:

- A reentrancy audit of the dispatch path.
- A lock-free SPSC variant tolerant of ISR-priority producers.
- A per-Node `#[isr_safe]` proof contract.

Until that work lands, `nros check` rejects
`DispatchStrategy::FromIsr` deployments with a clear diagnostic.

## Embassy

**Embassy is not supported.** nano-ros supports a way of running only if it
can keep its real-time promise there: nano-ros decides which callback runs
next and at what priority, so it can state when a callback runs, how late it
can be, and that a low-priority callback never holds up a high-priority one.

Under Embassy the executor would run inside an Embassy task, and Embassy's
scheduler, configured by the application, would decide when that task and
every callback in it run. nano-ros cannot analyse that schedule, so it cannot
make the promise. The hand-driven spin task, `Executor::spin_async` /
`spin_async_with_sleep`, the readiness notifier and the host-owned
`nros::setup!` assembly that Embassy integration used are gone.

Use one of these instead:

- **An RTOS or bare-metal board through `nros::main!`.** Tiers become OS
  threads (or the bare-metal loop) at priorities nano-ros derives from your
  launch contracts.
- **RTIC**, as described on this page. RTIC's scheduling is analysable and
  `nros::main!` generates the task mapping, so the promise holds.

## The RTIC scheduling model

RTIC is not an RTOS — it is a concurrency framework that compiles directly
to hardware interrupt handlers. There is no scheduler, no task control
blocks, and no context switch overhead.

| Property | Value |
|----------|-------|
| **Model** | Interrupt-driven (NVIC hardware scheduling) |
| **Preemption** | Yes — hardware interrupt nesting |
| **Priority levels** | 4–16 (depends on Cortex-M variant) |
| **Priority direction** | Lower number = higher priority (ARM convention) |
| **Context switch** | 12 cycles (Cortex-M3 tail-chain) |
| **Mutual exclusion** | Stack Resource Policy (SRP) — compile-time deadlock-free |
| **Scheduling analysis** | Fully static — all priorities and resources known at compile time |

RTIC's key innovation is the **Stack Resource Policy (SRP)**: shared
resources are protected by ceiling-based priority elevation, not locks. The
compiler proves at build time that no deadlock or unbounded priority
inversion can occur. This gives interrupt-driven scheduling the safety of a
cooperative model.

```rust
#[rtic::app(device = stm32f4xx_hal::pac)]
mod app {
    #[task(priority = 2, shared = [sensor_data])]
    async fn read_sensor(mut ctx: read_sensor::Context) {
        ctx.shared.sensor_data.lock(|data| {
            *data = read_adc();  // runs at ceiling priority
        });
    }

    #[task(priority = 1, shared = [sensor_data])]
    async fn publish(mut ctx: publish::Context) {
        ctx.shared.sensor_data.lock(|data| {
            node.publish(*data);  // cannot deadlock — SRP guarantee
        });
    }
}
```

**nano-ros integration**: RTIC tasks call nano-ros directly — no executor
needed. Each RTIC task can own a publisher, subscription, or service handle.
The RTIC talker in the Rust language workspace, `examples/workspaces/rust`,
shows it: a node package of its own, built by a bare-metal image row of the
workspace's `pubsub_bringup`.

Shared resources are guarded by the Stack Resource Policy (Baker, T.P.
(1991), "Stack-Based Scheduling of Realtime Processes", *Real-Time Systems*
3(1), 67–99): a compile-time ceiling with zero runtime overhead, so priority
inversion is impossible by construction. Compared with the RTOS models in
[Scheduling Models](../../realtime/scheduling-models.html), RTIC gives the
best determinism (hardware scheduling, 12-cycle worst-case latency on a
Cortex-M3 tail-chain) and the lowest RAM overhead (no task control blocks),
at the cost of flexibility: ARM Cortex-M only, no dynamic task creation.

## See also

- [Dispatch Strategy](../../realtime/dispatch-strategy.html) —
  the trichotomy and the per-Node-vs-per-callback rationale.
- [Role reference](../../build/roles.html) —
  the package roles in full.
- [Bare-metal Cortex-M3](../../targets/bare-metal.html) — the MPS2-AN385 board
  and the direct-exec entry shape the book teaches.
- [Scheduling Models](../../realtime/scheduling-models.html) —
  the scheduling models of the supported targets; the RTIC one is
  [below](#the-rtic-scheduling-model).

## Design background

- [RFC-0113: RT unified wait](https://github.com/NEWSLabNTU/nano-ros/blob/main/docs/design/0113-rt-unified-wait.md)
- [RFC-0065: colcon-like workspace builder](https://github.com/NEWSLabNTU/nano-ros/blob/main/docs/design/0065-colcon-like-workspace-builder.md)
  D5: declarative escapes first, `nros materialize` last.
- [RFC-0121: workspace-first project layout](https://github.com/NEWSLabNTU/nano-ros/blob/main/docs/design/0121-workspace-first-project-layout.md):
  every project is a workspace, and the entry is generated.

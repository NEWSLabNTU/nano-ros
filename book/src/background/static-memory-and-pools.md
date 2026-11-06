# Static memory and pools

<!-- type: explanation · level: 3 -->

The memory report of a hosted zenoh image lists its largest RAM symbols
first:

```text
## top 5 RAM symbols

       131,072   25.3%  nros_rmw_zenoh::shim::subscriber::LARGE_PAYLOADS
        88,560   17.1%  linux_entry::__nros_entry_run::__NROS_SCHED_TASK_EXECUTOR_BACKING
        88,552   17.1%  nros_node::executor::backing::EXECUTOR_BACKING
        87,072   16.8%  g_sessions
        33,536    6.5%  nros_rmw_zenoh::shim::service::USER_SERVICE_INBOX
```

None of these is a heap. Each is a static array, sized when the image was
built: a ring of large-message slots, the executor's storage, the transport's
session table, a ring of service requests. Together they are most of the
image's RAM, and they are the same size whether the node creates one
publisher or twenty. This page explains why nano-ros keeps its memory this
way, where each size comes from, and what still allocates. The task of
reading your own image is
[Measure static memory](../guide/contracts/measure-static-memory.md).

## Why static

A microcontroller with 64 KB of RAM has no room for a heap that fragments,
and a `malloc` that can fail at run time is a failure a review has to
account for. So the
nano-ros core is `#![no_std]`: the executor, nodes, publishers,
subscriptions, services, actions, timers, parameters and CDR serialization
all work with no allocator. Their storage is fixed-size pools, carved at
build time.

A pool is an array of slots. Every slot costs its bytes whether an entity
uses it or not. So an over-provisioned pool is wasted RAM, and an
under-provisioned one is a create call that fails at run time, when the pool
is full. Neither failure shows up at the compiler. The design answer is to
size each pool from what the image declares, rather than from a guess.

## Where a pool's size comes from

The pools that dominate static RAM are **derived per image** from what the
image's components declare: the entity inventory of the contract (how many
publishers, subscribers, services, clients and nodes), the message types they
use and the parameters they declare. The build writes the answers into the
image's generated settings, so you state nothing for them:

- The entity counts size the transport's publisher, subscriber and queryable
  tables, the liveliness tokens and the executor's callback slots.
- The message types size the receive buffers: the largest small-class type
  the image subscribes to sets the small slot, and the large class gets its
  own pool, which is zero bytes when no subscribed type is large.
- The executor's arena, which holds the dispatched callbacks and their
  captures, is sized from the same inventory.

The transport's wire buffers, the network stack's socket pools and the
parameter store limits are not derived: they depend on your traffic, not on
your entities, so they are yours to set
([Tune throughput and latency](../guide/rmw/tune-throughput-and-latency.md)).

A value you state always wins over a derived one. The order is: an exported
environment variable, then the image's `[image.<id>.knobs]`, then the board
descriptor's `[knobs]`, then the derived value, and the crate's default only
when nothing is stated and nothing can be derived. `nros config explain`
shows which of these answered for each knob.

Two properties of the message-derived sizes follow from how they are found:

- **They are upper bounds.** The bounds come from every type in the interface
  closure the image links, not only the types it subscribes to, so a size is
  never too small but can be larger than you need. The build names the type
  that set it.
- **One unbounded type stops the derivation.** A `string` or a sequence with
  no bound has no largest size. The build then derives nothing, keeps the
  stated or default values, and names every unbounded type and the member
  that makes it so. Bounding the field in the `.msg` (`string<=64`), or
  capping it in `nros-codegen.toml`, restores it.

Today some pools are sized by the backend you link rather than by what your
node does: a publisher-only node still reserves the service pools. An image
that looks larger than its entities suggest is expected, and turning those
knobs down is the remedy. The [Static pool inventory](../reference/static-pool-inventory.md)
lists every knob and what it sizes.

## The three service inboxes (zenoh)

The zenoh backend shows how a pool is split so that each part is sized for its
own traffic. Every service server is a zenoh queryable, and each queryable
owns an inbox ring: requests wait there between the transport's read task and
the executor's spin. The rings sit in three tables, one per family, so a
family is sized for its own requests rather than for the largest request any
service could send:

| Symbol | Holds | One ring is | Knobs |
| --- | --- | --- | --- |
| `USER_SERVICE_INBOX` | your services | depth × (slot + 12 B) | `NROS_SERVICE_INBOX_BYTES`, `NROS_SERVICE_INBOX_DEPTH` |
| `ACTION_INBOX` | the four queryables of each action server | depth × (slot + 12 B) | `NROS_ACTION_INBOX_BYTES`, `NROS_ACTION_INBOX_DEPTH` |
| `BUILTIN_INBOX` | the runtime's own parameter and lifecycle services | depth × (slot + 12 B) | `NROS_PARAM_SERVICE_INBOX_BYTES`, `NROS_PARAM_SERVICE_INBOX_DEPTH` |

The 12 B is a ring entry's length, sequence and overflow flag. Each table
holds one ring per queryable of its family, for every session, and the
image's queryable pool is partitioned among the three.

A slot's size comes from, in order: a value you state; otherwise the largest
request type the image declares a server for (for user and action services),
or the largest `set_parameters` request the declared parameters can produce
(for the builtin family). With nothing declared, the user and action slots
keep 1,024 B and the builtin table is empty. A bringup that declares `params`
instead of `param_services` builds the parameter store but registers no
service, so the builtin family claims no queryable at all.

On one safety-island image on an S32K344, `BUILTIN_INBOX` measured 24
parameter queryables × 1 × (1,016 + 12) = 24,672 B, and `USER_SERVICE_INBOX`
fell from 3,744 B to 288 B when those 24 queryables left it. Read your own
figures off the memory report, not off the formula: the compiler sizes the
table, so the report is exact.

## The executor's storage

The executor's own backing is usually the largest single owner in an image
with several sched tasks: each sched task has an executor, and each executor
has its storage. The C and C++ entries reserve it as one static sized by a
generated header (`NROS_CPP_EXECUTOR_STORAGE_SIZE` for C++), the Rust entry
as `EXECUTOR_BACKING`, and each component's state has a generated slot of
its own. The executor carves more than callbacks from its backing: when the
image builds a parameter store, the store, the parameter services' state,
their request and reply buffers and the slot a remote set decodes into are
carved there too. A `Promise` is allocation-free: it borrows its client's
reply slot rather than owning a buffer, so a pending service call costs no
extra storage.

## What `alloc` adds

`alloc` is an opt-in feature. It adds what needs a heap:

| With `alloc` | Why |
| --- | --- |
| `Serialize` and `Deserialize` for `String` and `Vec<T>` | heap containers |
| boxed timer callbacks (`Box<dyn FnMut()>`) | a closure stored by the timer |
| `handle_request_boxed`, a service reply in a `Box` | a reply too large for the stack |
| the blocking `Executor::spin`, `spin_period` and `halt_flag` | a shared halt flag (`Arc<AtomicBool>`) |
| the heap fallback for parameter services, and `-p` / `--params-file` overrides | storage the backing did not carve, and owned override strings |

The core `ParameterServer` API works without alloc, and so do parameter
services whenever the build knows the image builds a store: without `alloc`,
a build that carves no store is refused at compile time, naming
`NROS_PARAM_STORE`. There is no `std` feature. A hosted program has `std`
itself; the `env` feature reads the process environment, and `log` routes
nano-ros's diagnostics to the `log` logger you install.

## The transport still allocates

Turning off `alloc` removes the Rust side's heap use (`Box`, `Vec`, `String`).
It does not remove allocation in the C libraries the RMW backends link. They
keep their own session state, stream buffers and protocol metadata, through
the platform's allocator:

| Backend | Allocates through |
| --- | --- |
| zenoh | `malloc` on POSIX, `k_malloc` on Zephyr, `pvPortMalloc` on FreeRTOS, a bump allocator on bare metal |
| XRCE-DDS | `malloc` on POSIX and NuttX, `k_malloc` on Zephyr, `pvPortMalloc` on FreeRTOS; only when an entity is set up |
| Cyclone DDS | per message |

So an image with a transport is not free of allocation, and a feature list is
evidence that an image is heap-free and not *proof* of it: a vendored C
dependency or a weak-symbol fallback can link an allocator without any Cargo
feature changing. The proof is the linked image's symbol table. nano-ros's
own executor path is heap-free in that checked sense: its CI links an image
of the executor, component install and spin, with no transport, and fails on
any allocation symbol in it. The shipped examples all enable `alloc`.

## Where nano-ros differs from ROS 2

- **The core needs no allocator, and entities fit the pools.**
  ROS 2 creates entities on the heap whenever you ask. A nano-ros image
  creates them in slots sized at build time, and a create past the pool's
  size fails.
- **Message fields have a fixed capacity.** A ROS 2 `string` or sequence
  grows on the heap. A nano-ros field is a fixed-capacity type: its bound, or
  a default capacity when the `.msg` states none. A field with no bound also
  stops the size derivation, so a bounded field is the norm.
- **Memory is a build output.** The sizes come from the contract, and the
  memory report reads them off the linked image.

## Design background

- [RFC-0100](https://github.com/NEWSLabNTU/nano-ros/blob/main/docs/design/0100-rmw-agnostic-sizing-model.md):
  the sizing model: pool sizes derived from what an image declares.
- [RFC-0034](https://github.com/NEWSLabNTU/nano-ros/blob/main/docs/design/0034-platform-layer-split.md):
  the platform layer split, and the platform's allocator.
- [RFC-0113](https://github.com/NEWSLabNTU/nano-ros/blob/main/docs/design/0113-rt-unified-wait.md):
  why no async runtime drives the executor.

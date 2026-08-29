---
id: 896
title: "Every C/C++ subscription takes the small size class regardless of its message type — nothing fills `rx_buffer_hint`"
status: open
area: rmw, api
severity: medium
found: 2026-08-29
related: [0841, phase-392, phase-380, RFC-0038]
---

# The receive-buffer hint reaches the backend from Rust and from nowhere else

## What was measured

`rmw_subscription_options_t.rx_buffer_hint` exists (`nros-rmw-abi/include/nros/
rmw_entity.h:543`), and `rust_adapter.rs:571` reads it into `TopicInfo`. The
zenoh shim then routes the payload block by it: above `SMALL_CLASS_CEILING` the
subscriber gets a `large`-class block.

Every source file in the tree that mentions `rx_buffer_hint`:

```
packages/core/nros-node/src/executor/spin.rs        <- sets it (Rust path)
packages/core/nros-rmw-abi/include/nros/rmw_entity.h  <- declares the field
packages/core/nros-rmw/src/traits.rs                <- the Rust struct
packages/rmw/cffi/src/generated.rs                  <- bindgen output
packages/rmw/cffi/src/lib.rs                        <- passes it through
packages/rmw/cffi/src/rust_adapter.rs               <- reads it from options
packages/rmw/cffi/tests/node_slot.rs                <- a test literal 0
packages/rmw/zenoh/nros-rmw-zenoh/src/shim/subscriber.rs  <- consumes it
```

No file under `packages/api/nros-c`, `packages/api/nros-cpp`, `packages/cli/
rosidl-*` or `examples/` sets it. **The only producer in the tree is the Rust
executor.**

## Why this matters now

phase-392 W3a wired the Rust path: `create_subscription::<M>` passes
`subscription_rx_hint::<M>(RX_BUF)`, which is the TYPE's own
`max(MAX_SERIALIZED_SIZE_XCDR1, XCDR2)` computed from its schema. A Rust
subscription to a 4 KiB type now routes to the large class instead of raising
the global knob — the saving W3's own table prices at 98,304 B on
`SMALL_PAYLOADS`.

A C or C++ subscription to the same type does not. It hints 0, routes small,
and the only remedy left is the global `ZPICO_SUBSCRIBER_BUFFER_SIZE` — which
is charged to every subscriber in the image, and again to every executor arena
slot through `NROS_SUBSCRIPTION_BUFFER_SIZE`.

So W3a's saving is real and applies to half the tree. The phase doc records W3a
without the asymmetry, which is what this issue exists to correct.

## What makes this harder than the Rust side

The bound is a PROVIDED const on `nros_serdes::Message`, computed from
`Self::FIELDS` by `size::max_serialized_size`. A C/C++ message is a generated C
struct with no such trait, so the number has to reach the call site some other
way.

The constraint that decides the design: **it must not become a second
computation of the bound.** Two implementations of "how big can this type get"
is precisely the class this campaign keeps finding (the sizes-header mirror,
0088 -> 0114 -> 0122 -> 0123 -> 0245 -> 0268), and a serialised-size rule is
exactly the kind that looks right until an encoding rule changes under one copy.

Options, none surveyed in depth yet:

1. **Codegen emits the number into the generated C/C++ header**, computed by
   calling `nros_serdes::size::max_serialized_size` — the same function, from
   the CLI, which already compiles `packages/core/nros-serdes` (it is in
   `cli-source-dirs.txt`). Today `rosidl-codegen` renders `FIELDS` as a
   pre-rendered STRING of Rust source rather than constructing `Field` values,
   so this needs codegen to build the real values first. That refactor is the
   bulk of the work and it is the option that keeps ONE implementation.
2. **The `*_OPAQUE_U64S` channel** (`nros_config_generated.h`) is the
   established Rust-computes / C-declares seam, but it carries sizes of *our*
   types. Message types are per-workspace and user-defined, and a C-only image
   has no Rust message crate to compute from — so this does not obviously
   extend.
3. **Have the C API ask at runtime.** Rejected on sight: the point is to size a
   static block before it is allocated.

## Not to be confused with

Issue 0841, fixed: a hint landing between the small block size and the size
threshold got a block that could not hold it. That is about routing a hint that
exists. This is about there being no hint at all.

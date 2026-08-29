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

## Surveyed 2026-08-29 — two things this issue got wrong when filed

**"Rejected on sight: the point is to size a static block before it is
allocated."** Wrong reason. `alloc_payload_block(hint)` runs at
SUBSCRIPTION-CREATION time and only CHOOSES between two already-static pools, so
a runtime hint is fine in principle. The real reason a runtime answer is
unavailable is different: the RMW C ABI passes `type_name` and `type_hash` as
STRINGS (`rmw_vtable.h:170`) and no schema descriptor, so the C side has nothing
to compute a bound from at runtime either.

**"Codegen renders `FIELDS` as a string, so it does not have the nested
fields."** Half wrong. It renders the string, but it ALSO already resolves the
nested types transitively, in-process, for the RIHS type hash:
`rosidl_resolve::rihs::build_type_description(type_name, msg, resolve)` does a
BFS over nested refs and errors rather than guessing when one will not resolve.
Every generated message goes through it. So the full recursive schema IS
available at generation time.

That makes option 1 viable, and it is the one to take.

## The shape of the fix

`nros_serdes::size::size_bound` never reads `Field::offset` (checked: zero
references). The offsets are the only part of a `Field` that codegen cannot
know, so codegen can construct real `Field` values with `offset: 0` and call
`max_serialized_size` — THE function, not a copy of its rule. `&'static`
recursion (`FieldType::Nested(&NestedType)`, `Array(_, &FieldType)`) is
satisfiable by leaking in a short-lived CLI process.

The one hazard is the mapping. `render_field_type_expr` maps a rosidl
`FieldType` to a STRING today. Adding a second mapping to a VALUE, beside it, is
the sizes-header mirror defect being written on purpose. The mapping must go
VALUE-FIRST — build the `nros_serdes::FieldType`, then render the string from
it — so there is one mapping with two outputs. That refactor is the bulk of the
work and it lands in a heavily-tested emitter.

## Layers, in order

1. **Value-first field mapping in `rosidl-codegen`**, rendering the existing
   string from the value. No behaviour change; the existing emitter tests are
   the check.
2. **The bound, computed by `max_serialized_size` over those values**, nested
   types resolved by the same closure the RIHS path already uses. Emitted into
   the generated C/C++ message header. A test must assert it equals the Rust
   `MAX_SERIALIZED_SIZE_XCDR*` const for the same type — same input, same
   function, so a disagreement means the value mapping is wrong.
3. **`nros_subscription_options_t` grows an `rx_buffer_hint`.** Note this is an
   ABI CHANGE: the struct documents itself as extensible through
   `_reserved[2]`, and a `uint32_t` does not fit in two bytes. `generated.rs`
   must be regenerated (`scripts/gen-abi-bindings.sh`) and `check-abi-bindings`
   gates it.
4. **`nros-c` forwards it** into `rmw_subscription_options_t`, and **`nros-cpp`**
   through `options.hpp`.
5. **A user-facing spelling**, the C analogue of W3b's `nros::rx_buffer_for!`:
   the caller names the type once and gets a number that cannot drift.

Steps 3-5 are the ones that decide whether this is ergonomic or merely possible,
and they are worth designing before writing.

## Not to be confused with

Issue 0841, fixed: a hint landing between the small block size and the size
threshold got a block that could not hold it. That is about routing a hint that
exists. This is about there being no hint at all.

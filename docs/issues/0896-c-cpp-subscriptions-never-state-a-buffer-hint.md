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

## Surveyed again 2026-08-29 — the registry shortcut does not exist

Considered: skip the emitter work by having the C side look the bound up in a
generated `&[(type_name, bound)]` table sourced from the Rust message crates'
own `MAX_SERIALIZED_SIZE_*` consts. Same consts, no second computation, no ABI
change. **A C image does not link those crates, so there is nothing to source
the table from.**

* `packages/api/nros-c/Cargo.toml` deps are `nros`, `nros-node`, `nros-rmw`,
  `nros-core`, `nros-platform`, `nros-log`, `nros-platform-cffi`, the optional
  RMW backends, `paste`, `panic-halt`, `critical-section`. **No message crate.**
* The Rust packages cmake builds for the pure-C workspace are exactly three:
  `packages/rmw/cffi`, `packages/api/nros-c`, `packages/api/nros-cpp`. There is
  no `nros_ws_runtime` umbrella — that crate is generated only for a workspace
  that has Rust nodes (`examples/workspaces/mixed` has one; `c` does not).
* No generated Rust message crate exists anywhere under
  `examples/workspaces/c`.

So the number must be EMITTED at generation time. Layers 1-2 stand.

## The emit point is a template line, not a new emitter

C message headers are rendered from a minijinja pack —
`packages/cli/rosidl-codegen/packs/c/message.h.jinja`, registered in
`render.rs:93`, context built as `MessageCHeaderTemplate` at
`generator/msg.rs:406`. The header already emits `#define {constant_prefix}_…`
rows for message constants (`message.h.jinja:29`), so a
`#define {constant_prefix}_MAX_SERIALIZED_SIZE_XCDR2 <n>` is one template line
plus one context field. C++ has the sibling pack.

That is cheap. The cost stays where this issue already put it: computing `<n>`
through `nros_serdes::size::max_serialized_size` rather than a second walk, and
the value-first mapping refactor that makes that possible.

## The publisher side has the same defect, and the same number fixes it

Every generated typed publish helper stacks a buffer of a GLOBAL guessed size
(`message.h.jinja:116-121`):

```c
#ifndef NROS_PUB_BUFFER_SIZE
#define NROS_PUB_BUFFER_SIZE 256
#endif
static inline nros_ret_t std_msgs_msg_int32_publish(...) {
    uint8_t buf[NROS_PUB_BUFFER_SIZE];
```

One `#define` for every message type in the image, default 256, checked against
nothing. A type larger than it fails to serialize and the helper returns
non-zero, which the call sites in `examples/workspaces/c` do not distinguish
from a transport failure. Per-type `MAX_SERIALIZED_SIZE_*` replaces the guess
with the exact number and costs less stack on every type under 256 bytes.

## The C subscription is type-erased BY DESIGN, so layer 5 is the delivery

Not an accident of the ABI. RFC-0043 typed components subscribe RAW, carrying
the ROS type name as a string — `examples/workspaces/c/src/listener_pkg/
CMakeLists.txt` says so in as many words ("raw `/chatter` subscription carries
the type name as a string, so no generated C bindings are needed"), and
`nros-c/src/subscription.rs:487` builds the `TopicInfo` from those bytes.

Consequence: nothing in the C path can infer which `#define` belongs to a given
subscription, because the type is only ever a string there. **The hint has to be
supplied at the call site.** That makes layer 5 the actual delivery mechanism,
not ergonomic polish on top of a working feature — and it is the layer to design
first, because it decides what layers 3-4 must carry.

Also unnamed until now: the same call site allocates
`RawSubscription::<{ config::MESSAGE_BUFFER_SIZE }>`, a const generic off global
config. That is a SECOND per-subscription buffer on the C path, charged
identically to every subscription in the image, and it is not addressed by
`rx_buffer_hint` at all.

## Bounds in the `.msg` already work — this is a diagnostics job

`sequence<T,N>`, `string<=N`, `wstring<=N` and the combined forms already parse
and map to `nros_serdes::FieldType::{BoundedSequence, BoundedString,
BoundedWString}` (`generator/common.rs:1371-1425`), straight into the schema
`size_bound` walks, and `SizeBound.bounded` goes false only on a genuinely
unbounded member. So "bound the field in the `.msg`" is the first thing to tell
a user and it needs no code.

What it needs is a diagnostic: when the bound comes back `None`, name the
offending FIELD, not just the type. Today the caller gets `None` and no way to
learn which member cost them the bound. Cheapest useful change in this issue.

## An out-of-band bound is a different guarantee — do not conflate them

A bound stated somewhere other than the `.msg` (nano-ros config, or a launch /
contract declaration) is NOT the same object as an IDL bound, and the two must
not share a spelling:

* **Bounded in the `.msg`.** The publisher honours it too; an over-long message
  cannot be constructed. A real bound.
* **Bounded only out-of-band.** The publisher knows nothing about it. An
  oversize sample arrives and is dropped. That is a TRUNCATION CONTRACT, and it
  has to be loud at runtime — same class as `report_dropped_take`.

Where both exist they must not silently disagree: clamp to the smaller, or
refuse. A whole-type "max size" convenience knob is fine, but it must be an
`rx_buffer_hint` OVERRIDE, never a claimed schema bound — the one-computation
rule is the point of this issue.

## Discovering subscribers from the model is AUTHORING, not discovery

The spec models exactly what a per-subscription sizing pass would want
(`ros-launch-manifest/model/src/lib.rs:646`):

```rust
pub struct TopicWiring {
    pub msg_type: String,        // required, not Option
    pub publishers: Vec<String>,
    pub subscribers: Vec<String>,
}
```

and `Structure.topics` holds them (`lib.rs:166`). `msg_type` being non-optional
is a real advantage: a wiring row cannot exist without its type.

But **0 of the 144 model files in this tree carry any topic wiring.** The field
is `skip_serializing_if = "BTreeMap::is_empty"`, so an absent `topics:` key
means an empty map, not an omission — the count is exact. `structure` is
`scopes` + `nodes` and nothing else, Autoware-derived models included. The
`contracts` layer holds `node_paths.input`/`output` (topic NAMES) and
`PubContract { min_rate_hz, .. }` — no `msg_type` — and no contracts are
authored anywhere in the tree either.

The reason is structural, not an oversight: **a launch file does not declare
topics or their types.** It declares nodes, remaps and parameters. Which topic a
node subscribes to, and with what type, lives in the node's code. The resolver
cannot invent it.

This is the same trap phase-392 W5.b2 hit for service servers — the spec models
`ServiceWiring.server`, and the verb built on it returned 0 for all 115 models
including `service_server_model.yaml`. Recorded here so the third instance is
recognised before code is written.

So the model route works, but only once someone AUTHORS the wiring. Warning on
absent wiring is therefore the primary UX for as long as that stays true, not an
edge case.

## Layers, in order

0. **Name the field that cost the bound.** When `max_serialized_size` returns
   `None`, report which member is unbounded. No new surface; makes the existing
   `.msg` fix actionable.
1. **Value-first field mapping in `rosidl-codegen`**, rendering the existing
   Rust schema string from the value. No behaviour change; the existing emitter
   tests are the check.
2. **Both bounds, computed by `max_serialized_size` over those values**, nested
   types resolved by the same closure the RIHS path already uses. Emitted as two
   `#define`s (XCDR1 and XCDR2) from `packs/c/message.h.jinja` and the C++
   sibling, plus the `_get_rx_buffer_hint` accessor returning their max. An
   unbounded type emits NEITHER, so the accessor is absent and a call site
   naming it fails to compile. A test must assert each equals the Rust
   `MAX_SERIALIZED_SIZE_XCDR*` const for the same type — same input, same
   function, so a disagreement means the value mapping is wrong. Retarget
   `NROS_PUB_BUFFER_SIZE` onto the XCDR1 constant in the same change.
2b. **Thread the RFC-0033 `cap` into the schema emit** so a field bounded only
   in `nros-codegen.toml` stops reading as unbounded. Same test, extended with a
   capped field.
3. **Design the call-site spelling** — the C analogue of W3b's
   `nros::rx_buffer_for!`. Do this BEFORE 4-5: the C subscription is raw by
   design, so this is how the number reaches the runtime at all, and it decides
   what the ABI must carry.
4. **Carry it on BOTH C-facing subscription paths** (see below) — they are
   independent, and the examples use the one the issue did not name.
5. **`nros-c` forwards it** into the `TopicInfo` it already builds
   (`subscription.rs:487`, one line), and the component path likewise.

Out-of-band bounds (config / model) are a SEPARATE track that reuses layer 3's
spelling. They are not needed for any type whose `.msg` already bounds it.

## Correction: this is cbindgen, not bindgen

Earlier text here said `nros_subscription_options_t` needs
`scripts/gen-abi-bindings.sh` and is gated by `check-abi-bindings`. Wrong
direction. That pair covers HAND-WRITTEN RMW/platform C headers consumed by
bindgen into `nros-{rmw,platform}-cffi/src/generated.rs`.
`nros_subscription_options_t` is a Rust `#[repr(C)]` struct
(`nros-c/src/subscription.rs:145`) emitted OUT to the committed
`packages/api/nros-c/include/nros/nros_generated.h` by cbindgen. Regenerate
with `just regen-c-headers` — THE single writer (issue 0452) — and
`check-cbindgen-headers` gates staleness.

The layout note stands: `sched_context` + `message_info: u8` + `_reserved: [u8; 2]`,
so a `uint32_t` does not fit in the reserved bytes and the struct grows.

## There are TWO C-facing subscription paths, and the examples use the other one

* **`nros-c`** — `nros_subscription_init_with_options` takes
  `nros_subscription_options_t` and calls `session.create_subscription`
  directly (`subscription.rs:501`). This is the path the issue described.
* **`nros-cpp`** — `nros_cpp_subscription_register` (`nros-cpp/src/
  subscription.rs:144`), which is what RFC-0043 typed components call, and what
  every `examples/workspaces/c` node uses. **Ten flat arguments and no options
  struct**, plus a `_register_with_info` twin that duplicates the whole list.

The second is the one that must carry the hint for the examples to benefit, and
its argument list cannot grow. That is precisely the problem issue 0808 solved
for `create_session`, and its resolution is already written down in
`rmw_entity.h`: take a NULLable trailing options struct, because
`rmw_publisher_options_t` / `rmw_subscription_options_t` had already solved it
for entities. Same answer here, with `rx_buffer_hint` as the first occupant and
`sched_context` / `callback_group` / the `_with_info` split as the cleanup that
comes free.

## Layer 3 is smaller than feared: the call site already names the type

The C subscription is type-erased at the ABI, but the CALL SITE is not:

```c
nros_cpp_subscription_register(node, "/chatter",
                               std_msgs_msg_int32_get_type_name(), "", ...);
```

The token `std_msgs_msg_int32` is right there. So the hint needs no new macro
vocabulary — a sibling accessor in the family the header already emits
(`_get_type_name`, `_get_type_hash`, `_get_type_support`) reads identically and
cannot drift from the type name, because both come from one token:

```c
static inline uint32_t std_msgs_msg_int32_get_rx_buffer_hint(void);
```

The `#define` from layer 2 is what it returns. A macro form that expands to BOTH
arguments at once remains an option, but it is no longer load-bearing.

## Decided 2026-08-29

**1. An unbounded type is a COMPILE ERROR at the call site.** No bound, no
constant, no accessor — the call site naming that type fails to build. The
alternative (return 0, fall back to the global knob) reproduces today's silent
defect exactly: a subscription that quietly takes the small class and drops
samples at runtime on a target with no console. The escape hatch is to bound the
field, in the `.msg` or via `cap` (below); both are one line and both are the
thing we actually want the user to do.

**2. Size the RX hint from `max(XCDR1, XCDR2)`; size the TX buffer from XCDR1
alone.** The wire question has a live-verified answer already in this repo —
RFC-0055's 2026-07-26 correction:

> A default Jazzy peer serializes FINAL/XCDR1 on the wire (verified live for
> both fastrtps and cyclonedds; guard
> `nros_serdes::cdr::tests::xcdr1_header_matches_live_jazzy_wire_bytes`).
> "Modern ROS 2 defaults types to `@appendable`/XCDR2 so nano-ros must too" is
> wrong — and DDS-XTypes REJECTS an appendable writer against a FINAL reader, so
> emitting appendable-by-edition BREAKS default interop. Extensibility is a
> per-type property, never an edition property.

So XCDR1 is what the LTS editions put on the wire on the default path, and
nano-ros writes XCDR1 exclusively (`CdrWriter`'s constructors hard-wire it;
`EncodingVersion::default()` is `Xcdr1`). The TX buffer therefore needs XCDR1
only.

The RX side is not symmetric. `CdrReader` dispatches on the encapsulation id and
accepts XCDR2, and RFC-0055's machinery is "built + tested but parked" pending
per-type `@appendable` re-activation — so a peer's XCDR2 sample can arrive.
Neither encoding dominates the other in size (XCDR2 adds a 4-byte DHEADER per
struct, but aligns 8-byte primitives to 4 instead of 8), so the receive bound
must be the max. This matches what the Rust path already computes
(`subscription_rx_hint`, `rmw_type_registry.rs:168`).

**Consequence: emit BOTH constants, not one pre-maxed value.** The two consumers
want different numbers, and a single `MAX_SERIALIZED_SIZE` would be silently
wrong for one of them — the same reason `pad_to` takes a version rather than a
constant.

**3. The out-of-band bound already exists: `nros-codegen.toml` `cap` (RFC-0033).**
No new config file. Per-field `mode` + `cap`, deep-merged, `deny_unknown_fields`,
and already resolved per-field for the struct rendering
(`generator/common.rs:415-419`, `583-619`, `891-903`) — `cap = 64` on an
unbounded `string` renders `heapless::String<64>`.

The gap is one hop: the schema emit path
(`build_nros_schema_for_struct_with_path`) walks the RAW rosidl AST and has no
storage resolver in scope, so that same field still emits
`::nros_serdes::FieldType::String` and the type's bound comes back `None`. The
capacity is applied to the STORAGE and not to the SCHEMA. Threading the resolved
`cap` into `render_field_type_expr` is the entire out-of-band mechanism.

Semantics stay distinct, and this is why `cap` sizes a HINT rather than
asserting a bound: an IDL `string<=64` constrains the publisher, whereas a `cap`
is local to this image and a remote ROS node may still send 200 bytes. So a
capped field bounds our buffer and creates a truncation contract — which is
exactly what (4) handles.

**4. Oversize is explicit and NEVER fatal.** An app that dies on an embedded
target is worse for the user than one that drops a sample and says so — there is
often no console, no debugger and no way to restart it.

The receive path already has the right shape to copy, in
`executor/arena.rs:386`: `#[cold]`, a `DROPPED_TAKES` counter, first-then-every-
64th rate limiting (so one misconfigured subscription in a 40-participant graph
cannot flood the log — issue 0371's shape), and `nros_log` rather than stdio,
because a Rust `std` stdio call is FATAL inside Zephyr `native_sim` (issue 0589).
No panic, no abort.

The transmit path needs the same treatment plus a distinct status. There is no
too-large code today — the closest is `NROS_RET_FULL` (-6), which means a full
queue — so the generated publish helper returns a bare non-zero that callers
cannot tell from a transport failure. Add one (cbindgen: `just regen-c-headers`),
and report it once per site rather than per sample.

## RawSubscription: decided 2026-08-29

`RawSubscription<const RX_BUF: usize>` (`executor/handles.rs:1306`) holds its
receive buffer INLINE, so `RX_BUF` is a monomorphisation, not a parameter — a
runtime hint cannot select between `RawSubscription<1024>` and
`RawSubscription<4096>`. Worse, the size is baked into the APPLICATION's own
struct: `SUBSCRIPTION_OPAQUE_U64S` is
`u64s_for::<RawSubscription<{ MESSAGE_BUFFER_SIZE }>>()`
(`opaque_sizes.rs:29`), and a C caller declares
`uint8_t sub[NROS_C_SUBSCRIPTION_STORAGE_SIZE]` at their own compile time.

Three options were weighed:

* **Size classes plus a compile-time storage macro** — a small ladder of
  instantiations, dispatched at init against the caller's declared size.
  REJECTED: pays code size for N monomorphisations of the whole subscription
  path, gives a coarser answer than the exact bound, and leaves the arena
  coupling untouched.
* **Decouple the buffer (`&'a mut [u8]`)** — one type, no monomorphisation,
  exact per-subscription bytes, `_opaque` shrinks. This is the real per-type
  fix and matches the hint model directly. DEFERRED, not rejected: it adds a
  lifetime contract across FFI, and C has TWO subscription paths (the L1
  polling one here and the arena callback path at `nros-c/src/executor.rs:817`),
  so both must move together or half the tree keeps the old sizing.
* **Fix the arena instead** — TAKEN FIRST, and split out as **issue 0900**.
  It turned out not to be part of this issue at all: every arena slot is
  budgeted at the ActionClient worst case, so `ARENA_SIZE` is 74,240 bytes on
  every image measured, a talker included, where a pub/sub-only image needs
  ~16 KiB. It also explains why per-type receive sizing cannot help that
  number: the slot is sized from the GLOBAL knob, amplified 3 x MAX_CBS = 12x,
  regardless of what any individual subscription asks for.

Order: **0900 first** (largest saving, no ABI change, independent of everything
here), then the buffer decoupling as the per-type fix.

## Not to be confused with

Issue 0841, fixed: a hint landing between the small block size and the size
threshold got a block that could not hold it. That is about routing a hint that
exists. This is about there being no hint at all.

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

## Correction: "value-first" is not possible; "one traversal, two outputs" is

Layer 1 said: build the `nros_serdes::FieldType` VALUE, then render the existing
string FROM it, so one mapping has two outputs. Reading the emitter shows the
string is not derivable from the value, in two independent ways.

**The nested arm defers instead of inlining.** It emits a helper const that
points at the nested type's OWN `Message` impl:

```rust
pub const X_NESTED_HEADER: ::nros_serdes::NestedType = ::nros_serdes::NestedType {
    type_name: <std_msgs::msg::Header as ::nros_serdes::Message>::TYPE_NAME,
    fields:    <std_msgs::msg::Header as ::nros_serdes::Message>::FIELDS,
};
```

so the generated crate resolves the child at ITS compile time. The string
therefore needs a RUST PATH (`nested_path_resolver(package, name, ...)`), which
a `NestedType` value does not carry — it carries the ROS type name. Meanwhile a
value that can be SIZED must have its nested fields resolved in-process, which
the string deliberately does not do. Neither is a superset of the other.

**Helper-const identifiers are keyed on the field, not the type.** The recursive
arms hoist `{const_prefix}FT_{FIELD}_ELEM`, so rendering needs the field name
and prefix as well as the type.

**The fix that achieves the same safety property.** The hazard was never
"value-first" as such — it was a SECOND, INDEPENDENT walk from
`rosidl::FieldType` that can drift from the first. One traversal emitting BOTH
outputs in the same `match` arm has exactly the same property and is
implementable:

```rust
fn lower_field_type(
    rosidl_ty, field_name, prefix, resolver, helper_consts: &mut String,
) -> (String, &'static nros_serdes::FieldType)
```

Every arm produces its string and its value together, so adding a `FieldType`
variant that one output handles and the other forgets is a compile error rather
than a silent divergence. The `&'static` recursion is satisfied by leaking in a
short-lived CLI process, as already noted.

**Not started.** Landing a half-applied mapping refactor is precisely the defect
this issue exists to prevent, so the shape is recorded rather than begun —
layer 1 below is restated in these terms.

## The delivery shape, rewritten 2026-08-30 (the first one was wrong)

The original plan put the hint in an options struct on
`nros_cpp_subscription_register` and had the user set it. Judged against the
real call site in `examples/workspaces/c/src/listener_pkg/src/Listener.c`, that
design is worse than doing nothing, in four ways:

1. **It is an opt-in fix for a silent-default bug.** Anyone who does not read
   this issue keeps the defect.
2. **It names the type TWICE with nothing checking they agree** —
   `..._get_type_name()` and `..._get_rx_buffer_hint()` are independent
   arguments. Copy a subscription, change the type name, forget the hint, and
   the buffer is sized for the WRONG type, silently. **That failure mode does
   not exist today; the fix would have created it.**
3. **11 arguments instead of 10**, plus two setup lines, at every call site.
4. **The deliberate compile error arrives as "undeclared identifier"** — for an
   unbounded type there is simply no accessor, so the error never names
   `String` and never names the field `data`.

The right shape is already in the tree, on the publish side. A generated helper
KNOWS ITS OWN TYPE, so it should use its own constant and ask the user for
nothing:

```c
static inline nros_ret_t std_msgs_msg_int32_publish(...) {
    uint8_t buf[STD_MSGS_MSG_INT32_MAX_SERIALIZED_SIZE_XCDR1];  /* was NROS_PUB_BUFFER_SIZE, 256 */
```

Nobody edits a line; types under 256 get smaller stack frames and types over 256
stop failing. Subscribe should mirror it, and the missing sibling IS the problem
— codegen emits `_publish` and no `_subscribe`, which is why the raw call is
hand-written and type-erased at all:

```c
int32_t rc = std_msgs_msg_int32_subscribe(node, "/chatter", on_raw, self, &handle);
```

ONE token. Type name, type hash and buffer hint all derive from it, so they
cannot drift, because there is only one. Shorter than today's call rather than
longer, and correct for a user who never heard of this issue.

**This deletes the ABI work.** Layers 3-4 of the old plan — the options struct
and the `nros_subscription_options_t` break — stop being the delivery mechanism.
The raw path may gain a hint later for callers who need one, but it is no longer
how the fix reaches anybody.

## Correction: the real resolver already exists, on every edition

The previous version of this section claimed nested types cannot be resolved on
Humble, and it was wrong. It read `no_cross_pkg_resolver`'s doc comment —
"Humble (placeholder hash, resolver never consulted)" — as a statement about the
RESOLVER's capability. It is a statement about the HASH consumer.

The real one is already built and already threaded:

* `cargo-nano-ros/src/lib.rs:273` constructs a working cross-package closure off
  the ament index (`index.packages().get(pkg)?.get_message_path(name)`), and
  passes it to `generate_package` **unconditionally** — no edition gate;
* `rosidl-bindgen/src/generator.rs:194` composes it with same-package resolution
  from the package's own `share_dir` into `self_resolve`, which is a COMPLETE
  recursive resolver.

The edition only decides whether the type-HASH path consults it. Nothing about
it is Humble-specific.

So layer 1 is not blocked. What is actually missing is one hop: `self_resolve`
reaches `compute_msg_type_hash` and the generators, and does not reach the
schema emit in `rosidl-codegen`. Threading it there is the work.

`Unresolved` as a third outcome still earns its place — a dependency genuinely
absent from the ament index must not read as "unbounded" — but it is the RARE
case it was always supposed to be, not the common one.

## Decided 2026-08-30 — the three open questions

**Q1 — the hint goes in an OPTIONS STRUCT, and `_with_info` is retired.**
`nros_cpp_subscription_register` takes ten flat arguments and
`_register_with_info` duplicates the whole list to add one thing; a hint
parameter would make eleven and twelve. That is the trajectory issue 0808
already called out on `create_session`, and its resolution is recorded in
`rmw_entity.h`: take a NULLable trailing options struct, on the precedent that
`rmw_publisher_options_t` / `rmw_subscription_options_t` solved exactly this for
entities. One break, then every future axis is a field — and the `_with_info`
twin folds back in as `want_info` instead of being extended in parallel.

**Q2 — the compiler error names the TYPE and the FIELD. DONE.** The poison
identifier was `NROS_NO_SIZE_BOUND__see_the_reason_above_in_this_header`, which
named neither. It now carries both, and keeps the two failure kinds apart:

```c
#define FINGERPRINT_CORPUS_MSG_SHAPES_MAX_SERIALIZED_SIZE_XCDR1 \
    NROS_UNBOUNDED__fingerprint_corpus_msg_shapes__field_text

#define FINGERPRINT_CORPUS_MSG_NESTED_MAX_SERIALIZED_SIZE_XCDR1 \
    NROS_UNRESOLVED__fingerprint_corpus_msg_nested__nested_type_Shapes
```

A C identifier cannot hold `.` or `(`, so a nested path flattens and the prose
reason above it in the header carries what the identifier cannot.

**Q3 — TX and RX DO differ, on two independent axes, so SPLIT them.**

The question was whether they will genuinely have different values or treatment.
They already do, before caps enter:

1. **Encoding.** This stack WRITES XCDR1 only, so a transmit buffer needs the
   XCDR1 number. A receive buffer must hold whatever arrives, and `CdrReader`
   dispatches on the encapsulation id and accepts XCDR2, so it needs
   `max(XCDR1, XCDR2)`. Measured on the corpus: **68 vs 64 for one type** — a
   transmit buffer sized 68 wastes 4 bytes, a receive buffer sized 64 DROPS a
   sample.
2. **Caps.** `cap = 32` emits `char label[32]`, so this image cannot serialize
   more than that field holds — a real TRANSMIT bound. A remote ROS publisher is
   bound by the `.msg` and may send 200 bytes, so it is NOT a receive bound.
   Under caps the two numbers diverge further, and one of them stops existing:
   a capped unbounded field has a TX bound and NO RX bound at all.

So they are different questions with different answers and different failure
modes — an oversized TX buffer wastes stack, an undersized RX buffer drops
samples silently. `..._TX_MAX_SERIALIZED_SIZE_XCDR1` (caps honoured, XCDR1) and
`..._RX_MAX_SERIALIZED_SIZE` (IDL bounds only, max of both encodings), with the
current `MAX_SERIALIZED_SIZE_XCDR*` pair becoming the TX side.

## Layers, in order

1. **One traversal, two outputs** in `rosidl-codegen` (see the correction above
   — rendering the string FROM the value is not possible). Every `match` arm
   emits its expression string and builds its `nros_serdes::FieldType` value
   together, so a new variant that one output handles and the other forgets is a
   compile error rather than a silent divergence.

   **The trap to avoid here:** nested types must be RESOLVED to build a sizeable
   value, and resolution can fail (a dependency not on the search path). A
   failure to resolve is NOT the same as "no bound exists" — phase-380's rule is
   that `None` means UNBOUNDED and never UNKNOWN. An unresolvable nested type
   must fail the generate, not silently emit no constant, or this issue's own
   defect comes back wearing a different hat.

2. **Both constants emitted**, computed by `max_serialized_size` over those
   values — `<PREFIX>_MAX_SERIALIZED_SIZE_XCDR1` and `_XCDR2` — from
   `packs/c/message.h.jinja` and the C++ sibling. A test asserts each equals the
   Rust `MAX_SERIALIZED_SIZE_XCDR*` const for the same type: same input, same
   function, so a disagreement means the traversal is wrong.

3. **Retarget the publish helper** onto the XCDR1 constant. Complete, shippable,
   invisible: no API change, no opt-in, strictly smaller for every type under
   256 bytes. **This is the whole win on the publish side and it lands with
   layer 2.**

4. **Emit the missing `_subscribe` helper**, passing the hint. This is the
   subscribe-side delivery, and the reason the C path was type-erased in the
   first place.

5. **Unbounded types get a message that names the field.** No plain
   `_subscribe`; a `_subscribe_sized` taking an explicit byte count, plus a
   poisoned macro so the error says what is wrong:

   ```c
   #define std_msgs_msg_string_subscribe(...) \
     NROS_UNBOUNDED__std_msgs_String__field_data__use_subscribe_sized_or_bound_it
   ```

   The field name comes from `nros_serdes::size::first_unbounded`, which is what
   that function exists for.

6. **Thread the RFC-0033 `cap` into the schema emit** so a field bounded only in
   `nros-codegen.toml` stops reading as unbounded. Same test, extended.

## Layer 6 is TWO bounds, not one — correction found while implementing

Layer 6 said: thread the RFC-0033 `cap` into the schema emit so a field bounded
only in `nros-codegen.toml` stops reading as unbounded. That is half right and
the wrong half is dangerous.

A `cap` bounds OUR STORAGE. `"…/Shapes.text" = { cap = 32 }` emits
`char text[32]` in the generated C struct. So:

* **Transmit** — we physically cannot serialize more than 32 bytes of that
  field, so the cap IS a real bound on what this image emits. Legitimate input
  to a publish-buffer size.
* **Receive** — a remote ROS publisher is bound by the `.msg`, not by our
  config, and may send 200 bytes. The cap says nothing about incoming wire size.
  Sizing a receive buffer from it reintroduces exactly the drop this issue
  exists to stop.

**Consequence: `MAX_SERIALIZED_SIZE_XCDR*` cannot serve both consumers once
caps are honoured.** They are currently one constant used by the publish helper,
which is safe only because caps are NOT yet threaded. The moment they are, the
transmit number and the receive number are genuinely different values and need
different names — `..._TX_MAX_SERIALIZED_SIZE_XCDR1` (caps honoured) and
`..._RX_MAX_SERIALIZED_SIZE_*` (IDL bounds only).

This is the same mistake as emitting one maxed value across encodings, one
level up: a single number serving two consumers with different questions. It is
recorded before implementation rather than after, because the failure it
produces is a silently undersized receive buffer, which is the original defect.

## Layer 4 cannot pass the hint yet, and should ship anyway

`nros_cpp_subscription_register` takes ten flat arguments and has **no slot for
a buffer hint**. So a generated `_subscribe` helper can deliver the one-token
ergonomics — type name and type hash bound to a single spelling, a shorter call,
no drift — but not the buffer fix, until the register gains an options struct
(issue 0808's precedent, and the ABI change the UX rewrite was pleased to avoid;
it turns out to be needed after all, just not for the reason first given).

It should still land: the drift hazard it removes is real and independent of the
hint, and the helper is where the hint goes once the slot exists. But the issue
title is about the buffer, and a `_subscribe` helper alone does not fix it —
saying otherwise would overclaim.

## Not to be confused with

Issue 0841, fixed: a hint landing between the small block size and the size
threshold got a block that could not hold it. That is about routing a hint that
exists. This is about there being no hint at all.

# Phase 403 — the type's bound sizes every receive buffer, and third parties can say what they need

**Status (2026-08-30). Design, nothing landed.** Opened because
[phase 402](phase-402-c-subscription-options-struct.md) delivered the PLUMBING
for a per-type receive hint and stopped there: the hint now reaches the backend
and changes nothing's size. Depends on
[issue 0896](../issues/0896-c-cpp-subscriptions-never-state-a-buffer-hint.md)
for the bound itself and overlaps
[issue 0900](../issues/0900-arena-slots-budgeted-at-action-client-worst-case.md)
for the arena.

## There are TWO receive buffers per subscription, and neither is precise

This is the fact the earlier work kept eliding, and it is why "we wired the
hint" did not make anything smaller.

**1. The RUNTIME-OWNED take buffer.** `rmw_vtable.h`'s `take` slot says it
outright — "the payload is bytes and **the caller owns the buffer**, so it needs
the length back". The runtime hands `buf`/`buf_len` to the backend. That buffer
is `RawSubscription<RX_BUF>`'s inline `[u8; RX_BUF]`, or the arena's buffered
region. `RX_BUF` is a CONST GENERIC.

**2. The BACKEND-INTERNAL staging block.** zenoh-pico keeps its own payload
pools and picks between them with `alloc_payload_block(hint)`:

```rust
if rx_buffer_hint > SMALL_CLASS_CEILING { /* LARGE_PAYLOADS */ } else { /* SMALL_PAYLOADS */ }
```

Two statically sized classes. A 68-byte type and a 1000-byte type get the same
small block.

So today: buffer 1 is one global size for every subscription in the image, and
buffer 2 is a two-way choice. The hint improves ROUTING — it stops a 4 KiB type
being silently small-classed and dropped, which is real — and it sizes nothing.

## Why buffer 1 cannot simply take the hint

`RX_BUF` is a const generic, so a runtime value cannot reach it. The Rust
generic path CAN monomorphise per type (`rx_buffer_for!` already computes the
number). The C path cannot: `create_subscription` there is type-erased by
design — RFC-0043 components subscribe raw with the type name as a STRING — so
there is no `M` to monomorphise on.

That is the whole reason this phase exists rather than being a parameter change.

## The C++ component path still has `M`, and that changes W3's scope (2026-08-31)

"The C path cannot monomorphise" is true of `create_subscription_raw`, and it is
NOT true of the path the embedded C++ images actually take. Traced on
mr-canhubk344:

```
NROS_SUBSCRIBE(Msg, method, topic)                         component_node.hpp:632
  -> ComponentNode::create_subscription<M, C, Method>      component_node.hpp:287
    -> bind_subscription<M, C, Method>                     component.hpp:98
      -> create_subscription_raw(node, topic, M::TYPE_NAME, trampoline, self, qos)
```

`M` is a template parameter the whole way down and is erased only in that last
call, in a C++ header, where `M` is still in scope. The generated C++ type
already carries the number:

```cpp
static constexpr const char* TYPE_NAME = "nav_msgs::msg::dds_::Odometry_";
static constexpr size_t SERIALIZED_SIZE_MAX = 1804;
```

Two consequences for the wave plan:

* **W3's lifetime contract is not needed for this path.** `bind_subscription`'s
  own doc says it "registers a RAW subscription (so the executor arena owns it,
  no C++ `Subscription<M>` storage object)". The ARENA owns the buffer, not the
  caller, so there is nothing to hand across FFI and no `'a` to get wrong. The
  change is a runtime `size_t` threaded from `bind_subscription` into
  `create_subscription_raw` and on to the slot allocation. W3 as written -- the
  caller supplies the bytes -- remains the answer for the hand-written C API,
  which genuinely has no `M`.
* **It collapses into W5.** Once a per-subscription byte count reaches the arena,
  sizing the slot from it IS W5. For the C++ component path W3 and W5 are one
  change, and it needs no ABI slot.

### The measurement case, with numbers

The island entry (4 RFC-0043 components, zenoh over serial). Subscribed bounds,
read from the generated headers rather than from wire observation:

| type | `SERIALIZED_SIZE_MAX` |
| --- | ---: |
| `Control` | 2052 |
| `Odometry` | 1804 |
| `OperationModeState` | 572 |
| `VelocityReport` | 549 |
| `SteeringReport` | 527 |
| `RouteState` | 524 |
| `GearCommand` | 524 |

33 handles, 13 of which receive. Today every slot is charged the largest
subscription's buffer, so the correct global value (2052, set by `Control`) gives
`36 * (3 * 2052 + 512) + 2048 = 242096 B` against 77968 B of DTCM. Sizing each
slot from its own type is roughly 47 KiB and fits with room.

That is an 5x reduction on a real image, and it is the before/after this phase
demands. The image is named: `src/zephyr_entry` on `mr_canhubk3/s32k344`, serial
transport, in the simple-autoware-safety-island superproject.

## The third-party contract, which is the part with no answer today

`rmw_subscription_options_t.rx_buffer_hint` is in the ABI and reaches every
backend through the vtable's `create_subscription`. What is missing is both
halves of an actual contract:

* **What is a backend OBLIGED to do with the hint?** The field's doc says a
  "size-classing backend (zenoh-pico) can pick a small/large receive buffer" —
  `can`, not `must`, and nothing says what a backend that ignores it must
  guarantee instead. A third party reading only the header cannot tell whether
  ignoring it is conformant.
* **A backend cannot say what it NEEDS or what it CHOSE.** The flow is
  one-directional. The runtime cannot ask "given this type, how big must my take
  buffer be?", and cannot learn that a backend rounded 68 up to 1024. So the
  runtime sizes buffer 1 blind, which is exactly why it uses a global constant.

Both are ABI questions, and getting them wrong is expensive to undo, so they are
W1 rather than an afterthought.

## Two decisions that shrink this phase (2026-08-31)

**1. The ABI may be broken.** nano-ros is not released. "Adding a mandatory
vtable slot breaks every out-of-tree backend at once" was the stated reason W1's
`required_rx_bytes` had to be OPTIONAL and NULLable, and the reason W3's change
to `nros_subscription_t`'s size looked expensive. Neither is a constraint now.
Shape both on the merits.

**2. Every message type has a derived upper bound, by requirement.** A user MUST
bound it in the `.msg` (`string<=64`) or cap it in the codegen config. An
unbounded type is a BUILD ERROR, not a fallback to a configured default.

The second is the larger of the two, because the fallback is what forced a global
constant to exist at all. Consequences:

* The "unbounded" branch stops being a runtime size question and becomes a
  codegen diagnostic. The C pack already does exactly this -- `message.h.jinja`
  emits an `unbounded_token` so naming the size constant of an unbounded type is
  a deliberate compile error, with `unbounded_reason` naming the member that cost
  the bound. That precedent is the shape to follow, not a new one.
* Phase-380's rule is untouched and still governs: `None` means "no bound
  EXISTS", never "unknown", and the code must never invent a number. Erroring
  honours that rule; substituting a default is what it forbade.
* `DEFAULT_RX_BUF_SIZE` may have no remaining purpose once every type is bounded.
  That is a W5 question, not a W2 one -- the constant is load-bearing in the
  arena derivation and in the C API's `MESSAGE_BUFFER_SIZE` welding.

## The arena already allocates variable-size, which is why W5 is small

`Executor::arena_alloc<T>` is a BUMP allocator (`executor/spin.rs`), with a
`trailing_bytes` variant. Allocation is already per-entry and variable; nothing
about the allocator has to change to give one subscription a different buffer
from another.

So `MAX_CBS * per_entry` is not the shape of the allocations. It is a build-time
ESTIMATE of how large the arena must be, and it is the estimate -- not the
allocator -- that budgets every slot at the worst case. W5 is therefore two
things, neither of them structural:

1. Pass the per-type byte count to the allocation site (see the C++ component
   path section above -- `M` is in scope there).
2. Make the arena SIZE estimate honest. Issue 0900's W1 already landed the
   measurement half: `arena_used()` / `arena_capacity()` plus a first-spin
   advisory naming the value to set. An image can be built once, read its own
   advisory, and pin the number -- so a build-time entity inventory is NOT a
   prerequisite for the mechanism, only for deriving the estimate automatically.

Two stale doc comments claim the arena is inline on the task stack and invisible
to the linker: `executor/arena.rs` (in `report_arena_headroom`) and
`executor/spin.rs` (on `arena_capacity`). Both predate phase-271 and both should
go with this wave; see the W5 note.

## Waves

**W0 -- an unbounded type stops being sizeable (landed 2026-08-31).** The
precondition for every other wave: decision 2 above, made real at the one site
that silently substituted a number. `rx_buffer_for!` expanded to
`DEFAULT_RX_BUF_SIZE` for a type with no bound; it now fails the build, in an
inline `const` block so the refusal does not depend on whether the macro was
written in const-generic argument position or as a plain expression. The
diagnostic names both remedies and points at the codegen diagnostic for the
member. `packages/api/nros/tests/rx_buffer_for.rs::an_unbounded_type_keeps_the_configured_default`
is RENAMED, not deleted, into `an_unbounded_type_is_refused_rather_than_defaulted`,
and the compile-time half is a `compile_fail` doctest with a compiling positive
control beside it.

### W0's blocker: the `cap` escape hatch does not reach the bound

Decision 2 says a user MUST bound a type "in the `.msg` or as a `cap` in
`nros-codegen.toml`". **The second half is not true today**, and until it is, the
rule has only one remedy -- editing a `.msg` you may not own.

The `cap` mechanism EXISTS and works, for STORAGE.
`rosidl-lower/src/config.rs` parses `[fields] "pkg/Msg.field" = N` (or
`{ cap = N, mode = "..." }`) into `CapacityResolver`, and the emitted container
becomes `heapless::String<N>` / `heapless::Vec<T, N>`. Its own module doc says
what it is for: "Only **unbounded** fields consult the resolver."

The bound is computed on a different path that never sees it.
`schema_value::build_schema` lowers IDL `String` -> `SerdeFieldType::String`
and `Sequence` -> `SerdeFieldType::Sequence` unconditionally -- `bound_message`
takes `(owner, msg, version, lookup)` and no resolver -- so
`max_serialized_size` returns `None` however the field is capped. The emitted
Rust `Message::FIELDS` says the same (`generator/common.rs`'s
`render_field_type_expr`).

Proven by the tree's own goldens, not by reading:

* `fingerprint-corpus/nros-codegen.toml` caps `Shapes.text` at 32, and
  `expected/configured/Shapes.h` still says
  `Reason: unbounded member: text (string)` and emits the poison token.
* `diagnostic-msgs/nros-codegen.toml` caps `KeyValue.key` at 32, and the
  committed `key_value.rs` stores `heapless::String<32>` while its `FIELDS` say
  `FieldType::String`.

The C header at `packs/c/message.h.jinja` already prints the advice ("or give it
a `cap` in `nros-codegen.toml`"), so the diagnostic currently sends users to a
knob that will not help them.

Wiring cap -> bound is NOT a mechanical fix and is deliberately not done here.
`Message::FIELDS` is also what backends that build wire-type descriptors at
runtime read (Cyclone dynamic types), and DDS treats `string` and `string<32>`
as different types for type-consistency matching. So "the cap narrows the bound"
is a decision about whether a cap is a claim about OUR STORAGE (true: a longer
sample already fails with `CapacityExceeded`, so a buffer sized to the cap loses
nothing) or a claim about THE WIRE (false, and asserting it could stop a
subscription matching). That belongs to the owner, not to W0.

Until it is settled, three consequences:

1. Codegen cannot be made to REFUSE an unbounded type. It would break every
   in-tree type carrying a `string`, with no remedy short of forking the `.msg`.
2. The remaining fallback in codegen stays: `{Msg}_publish` sizes its stack
   buffer from `NROS_PUB_BUFFER_SIZE` (256) for an unbounded type
   (`packs/c/message.h.jinja`). Poisoning it like its RX sibling is right under
   the rule, and is blocked on the same gap -- the helper is `static inline`, so
   the error would fire for anyone merely INCLUDING the header.
3. The C++ pack is worse than a fallback and should be looked at on its own:
   `types.rs::compute_serialized_size_max` never reports unboundedness at all.
   It ADDS 512 per nested message and 256 per string and always emits
   `SERIALIZED_SIZE_MAX`, which is what every `uint8_t buf[M::SERIALIZED_SIZE_MAX]`
   in `nros-cpp`'s headers is sized from -- including the per-type numbers in
   this document's own measurement table. Correcting it requires changing
   `nros-cpp`, so it is named here rather than attempted.

**W1 — the contract, in the ABI and in prose.** Two additions to
`nros/rmw_vtable.h` + `rmw_entity.h`:

**W1 — the contract, in the ABI and in prose. LANDED 2026-08-31.** Two additions
to `nros/rmw_vtable.h` + `rmw_entity.h`:

1. `rx_buffer_hint`'s doc becomes normative: a backend MUST NOT deliver a sample
   larger than the hint into a caller buffer smaller than it, and MUST report
   the size it settled on. Ignoring the hint stays legal — the guarantee is
   about not lying, not about honouring it.
2. An OPTIONAL vtable slot `required_rx_bytes(type_name, type_hash, hint) ->
   size_t` so a backend can answer "for this type at this hint, a take buffer of
   N is enough". `NULL` means "the hint is the answer", which is what every
   current backend would return. The runtime uses it to size buffer 1 instead of
   guessing.

   Optional and NULLable because `check-rmw-abi-shape` treats a vtable slot as a
   contract: adding a mandatory one breaks every out-of-tree backend at once,
   and the campaign's own rule is that a gap goes in the ledger with an issue id
   rather than being forced.

   **The `-> size_t` return above is what landed as an out-parameter**, because
   the header's own preamble makes it one rule with no exceptions: every slot
   returns `rmw_ret_t`, every ANSWER is an out-parameter, and no slot may
   multiplex a count with a status — gated by `scripts/check-rmw-ret-sign.py`.
   A `size_t` return also has no way to say "I cannot size this type", which is
   a PER-TYPE answer that NULLing the whole slot cannot express.
   The landed slot is

   ```c
   rmw_ret_t (*required_rx_bytes)(const char *type_name,
       const char *type_hash, size_t hint, size_t *out_bytes);
   ```

   appended LAST in `nros_rmw_vtable_t` (slot 75), NULL in every in-tree
   backend, `NROS_RMW_RET_UNSUPPORTED` reserved for "cannot size this type"
   with the same fallback as a NULL slot.

   **The optionality argument above is also retired.** nano-ros is unreleased
   and this ABI may be broken, so "breaks every out-of-tree backend at once" is
   not a reason for anything in this phase. The slot is still OPTIONAL, on
   three arguments that survive without it: a slot cannot be REQUIRED before
   something dispatches it (`check-rmw-required-slots.sh` holds the required set
   equal to the `.expect()`ed set — requiring one nothing calls is issue 0349);
   mandatory does not delete the "no opinion" answer, it relocates it into five
   identical backend bodies and a defaulted `RustBackend` method; and it is slot
   75, while uORB's C++14 positional initialiser stops at slot 17 and cannot
   skip. **W4 should promote it to required in the same commit that adds the
   dispatch site** — that is a registration-check change, not an ABI change, so
   nothing is foreclosed by leaving it optional here.

   **`rx_buffer_hint`'s `0` also changed meaning** and W2/W3 inherit it: every
   message type now has a derived upper bound (`.msg` bound or a
   `nros-codegen.toml` cap; unbounded is a BUILD ERROR), so the runtime always
   has a number. `0` means "this caller stated nothing", never "this type is
   unbounded". The "falls back to a configured default" framing is gone with
   it.

**W2 — buffer 1, Rust path.** `create_subscription::<M>` already knows `M`;
size `RX_BUF` from `M::MAX_SERIALIZED_SIZE_*` instead of the global default.
This is the cheap half and it needs no ABI.

**W3 — buffer 1, C path: decouple the storage.** The const generic cannot take a
runtime number, so the buffer stops living inside the entity:

```rust
pub struct RawSubscriptionRef<'a> {
    handle: RmwSubscriber,
    buffer: &'a mut [u8],   // runtime length
    event_regs: EventRegs,
}
```

The caller supplies the bytes, which is what `{Msg}_subscribe` is already
positioned to do — it knows the type's `RX_MAX_SERIALIZED_SIZE` and currently
passes it only as a hint. Issue 0896 records this as "buffer decoupling",
deferred there, owned here.

**Cost to be honest about:** this introduces a lifetime contract across FFI. A
`static` buffer is fine; a stack array that outlives nothing is a footgun, and
the C API cannot express the lifetime. The mitigation is that the generated
`{Msg}_subscribe` macro owns the declaration, so the common path never writes it
by hand — but a hand-rolled caller can still get it wrong, and that must be said
in the header rather than discovered.

**W4 — buffer 2, exact classes.** With W1's `required_rx_bytes`, a backend can
be asked for an exact size rather than choosing a class. zenoh-pico keeps its
pools (a target has no allocator) but the CLASS BOUNDARIES stop being two
arbitrary constants and become the distinct sizes an image's types actually
need. Requires the entity inventory, so it is gated on the same thing issue 0900
is gated on.

**W5 — arena slots.** Issue 0900's remaining half. Once W2/W3 make a
subscription's buffer a known per-type number, an arena slot can be sized from
the entities that will occupy it rather than from `MAX_CBS x ActionClient`.
**The "arena is on the task stack" correction is itself stale (measured
2026-08-31).** 0900 says the arena is inline on the task stack, so `nm` and
`mem-report` cannot see it and `NROS_ARENA_REQUIRED` cannot work. That is not
what the code does any more, and it changes what W5 has to measure.

Placement is CALLER-DETERMINED, and has been since phase-271 (issue 0110) moved
the six sized tables off build-time consts:

* `Executor` holds `arena: &'s mut [MaybeUninit<u8>]` -- a borrowed slice
  (`executor/spin.rs`). Nothing arena-sized is inline in it.
* `ExecutorInlineStorage` (`executor/storage.rs`) DOES hold `backing` inline, and
  the C FFI sizes its `_opaque` from that type. A stack-declared
  `nros_executor_t` therefore does put the arena on the stack -- that is the case
  0900 saw.
* The C++ component entry does NOT take that path. `main` ->
  `ZephyrBoard::run_components` -> `nros::init()` -> `Node::GlobalStorageHolder`,
  whose `static uint8_t storage[NROS_CPP_EXECUTOR_STORAGE_SIZE]` is `.bss`.

Measured on mr-canhubk344 (RFC-0043 components, zenoh, serial): DTCM tracked
ARENA_SIZE one-for-one across a MAX_CBS change of 24 -> 36, +26992 B observed
against +24576 B predicted. The arena was in `.bss` on that image, is
linker-visible, and `NROS_ARENA_REQUIRED` would work for this path.

So W5 needs BOTH: a linker-symbol check for the `.bss` placement (the C/C++
component path, which is where the embedded images are) and a stack probe for the
`ExecutorInlineStorage` placement. Sizing against either one alone reports the
wrong number for half the images. `arena.rs`'s doc comment states the stack case
as though it were the only one and should be fixed with this wave.

Not to be confused with a separate finding on the same board: the C++ init call
chain needs more than 16 KiB of MAIN stack (16384 overflows in `open_in`, 32768
does not) at a CONSTANT arena size of ~51 KiB. That is the depth of the init
chain, not the arena, and the two were briefly conflated during bring-up.

## W6 -- the derived bound must leave codegen (2026-08-31, owner's direction)

Codegen is the right place to DERIVE a bound. It is the wrong place for the
bound to STOP, and today it stops there: the number is emitted as a per-type
constant inside a generated header and nothing downstream can ask for it.

The distinction is the one RFC-0049 already draws for platform config
-- capabilities are facts, knobs are policy:

* A **cap** (`string<=64` in the `.msg`, or a `cap` in the codegen config) is
  POLICY. It is an author's declaration about their interface and belongs with
  the interface.
* A **bound** is a DERIVED FACT. Codegen computes it once, and no later stage
  should re-derive it, re-guess it, or fall back past it.

Every consumer that needs the fact and cannot get it currently invents a
substitute:

| consumer | what it does instead |
| --- | --- |
| arena derivation (`nros-node/build.rs`) | `MAX_CBS * worst case` |
| zenoh payload classes | two hand-set constants, `SUBSCRIBER_BUFFER_SIZE` / `SUBSCRIBER_LARGE_SIZE` |
| `NROS_MAX_LARGE_SUBSCRIBERS` | a human counts which types exceed the ceiling |
| the C API's `MESSAGE_BUFFER_SIZE` | welded equal to `DEFAULT_RX_BUF_SIZE` |

That last row is not hypothetical. Bringing the island up on
mr-canhubk344 required reading `Control 2052` and `Odometry 1804` out of
generated C++ headers BY EYE and copying them into a board `.conf` to set
`NROS_MAX_LARGE_SUBSCRIBERS=2` and `NROS_SUBSCRIBER_LARGE_SIZE=2560`. Then the
arena had to be pinned by guesswork, and the first guess (40960) was too small --
the image failed at `create_subscription` with the arena exhausted. Each of those
is a number the build already knew and could not say. This is the
"a knob nobody can enumerate is a knob nobody sets" failure that issues 0271 and
0739 record, reproduced end to end.

**W6: export the derived bounds as build metadata.** The channel already exists
for the executor's own numbers -- `DEP_NROS_NODE_RX_BUF_SIZE` and friends reach
the C API through Cargo `links` -- plus a manifest for the CMake/Kconfig side
that the Zephyr lane reads. With the inventory available:

* the arena sizes from the entities actually registered, rather than
  `MAX_CBS * worst case`;
* W4's "class boundaries become the distinct sizes an image's types actually
  need" becomes expressible -- it is blocked today precisely on this missing
  inventory, which is why the phase notes W4 is "gated on the same thing issue
  0900 is gated on";
* `NROS_MAX_LARGE_SUBSCRIBERS` and `NROS_SUBSCRIBER_LARGE_SIZE` stop being
  numbers a human reads off a header.

It also answers this phase's standing open question about where an entity
inventory comes from: it is codegen's output, currently discarded.

## What a `cap` may and may not claim (2026-08-31, owner's direction: follow ROS 2)

W0 found that the `cap` in `nros-codegen.toml` reaches the STORAGE container and
not the bound, so the C header's advice ("bound it in the `.msg`, or give it a
`cap`") currently names a knob that does not help. The question was whether to
wire `cap` into the bound. The owner's ruling is to follow ROS 2, for interop.

**What ROS 2 does**, read from the Humble installation rather than from memory:

```c
/* rosidl_typesupport_introspection_c/message_introspection.h */
uint8_t type_id_;             /* ROS_TYPE_STRING (16), bounded or not */
size_t  string_upper_bound_;  /* the bound, carried separately */
bool    is_upper_bound_;      /* same shape for arrays and sequences */
```

There is no `BOUNDED_STRING` type id: a bound is an ATTRIBUTE of the field, not a
different type. And the bound is not on the wire -- a bounded and an unbounded
string serialize to identical CDR (length, bytes, terminator). Our own generated
`Odometry` shows it: `child_frame_id` is `[u8; 256]` in storage and goes out
through `write_string(fixed_str(..))`, the same encoding an unbounded field uses.

**What follows.** A cap changes our storage and our derived bound. It changes
nothing a remote participant can observe -- not the type name, not the encoding,
not endpoint matching. But that is exactly why it cannot silently become the
bound we size a RECEIVE buffer from:

| | where it lives | may a receive buffer be sized from it? |
| --- | --- | --- |
| `.msg` bound (`string<=64`) | the interface, shared by every participant | YES -- contractual |
| `cap` in our codegen config | our build only | NO -- unilateral |

A `.msg` bound is part of the contract every participant compiled against. A
config cap is a claim only we made; a conforming remote publisher never agreed to
it and may legitimately send more, and we would then take the sample and discard
it. That is the silent-shortfall failure this phase exists to remove, so
promoting `cap` to a bound would reintroduce it through the config file.

**Therefore:** codegen enforces "every type must be bounded" against `.msg`
bounds. `cap` stays storage-only. Where an image needs a bound that the `.msg`
does not give, the honest fix is to bound the interface -- which is also the fix
ROS 2 would require, and keeps the two stacks describing the same type the same
way.

## Measurement, first not last

Every wave here claims bytes, and this campaign has twice published a number
that turned out to describe a path the mechanism did not run on (phase-392 W5.d
measured on a pure-cargo leaf; W5.g then found the figure reaches no workspace
checked). So:

**No wave in this phase is done until a before/after exists on an image that
actually runs the changed code**, and the doc names which image. Compile-tier
green is not evidence of a saving — phase 402 is explicitly marked that way.

## Explicitly not in this phase

The publish side. `{Msg}_publish` already stacks the type's exact TX bound; that
half of issue 0896 is finished and needs nothing here.

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

## Waves

**W1 — the contract, in the ABI and in prose.** Two additions to
`nros/rmw_vtable.h` + `rmw_entity.h`:

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

# Phase 408 — a C/C++ subscription sizes its buffer from its own message type

**Status (2026-08-31). Opened from a design review; survey done, no code yet.**
Scope is the C and C++ path only. The other obstacles to
[phase 392](phase-392-static-memory-space-campaign.md)'s "W3 remainder" — Rust
being opt-in (W3c), Cyclone not consuming the hint (W3f), unbounded diagnostics
and the TX/RX split (W3g) — are deliberately out of scope here and stay where
they are.

Carries [issue 0896](../issues/0896-c-cpp-subscriptions-never-state-a-buffer-hint.md).

## The finding that changes the plan

**phase-392 W3e and issue 0896 both describe a design fork — monomorphise the
const generic per type, or make the arena entry runtime-sized. On the path the
C/C++ subscription actually takes, the second option is already built.**

`add_arena_subscription_c_callback<const RX_BUF: usize>` never stores a
`[u8; RX_BUF]`. It computes

```rust
let (_slot_count, trailing_bytes) = buffered_region_size(qos.depth, RX_BUF);
let (entry_offset, trailing_offset) =
    self.arena_alloc_with_trailing::<SubBufferedRawCEntry>(trailing_bytes)?;
```

and `SubBufferedRawCEntry` holds `buffer: BufferStrategy` — a `TripleBuffer` or
`SpscRing` **initialised over trailing arena bytes at runtime**:

```rust
pub(crate) struct SubBufferedRawCEntry {
    pub(crate) handle: session::RmwSubscriber,
    pub(crate) buffer: BufferStrategy,
    pub(crate) callback: RawSubscriptionCallback,
    pub(crate) context: *mut core::ffi::c_void,
}
```

`RX_BUF` is consumed only as a VALUE — by `buffered_region_size`, and by
`TripleBuffer::init(ptr, RX_BUF)` / `SpscRing::init(ptr, RX_BUF, depth)`. A
const generic carrying a number that is never used in a type is a runtime
parameter that has not been spelled as one.

So this campaign is not "design a mechanism". It is: **produce the number, get
it to the call site, and stop passing it as a const.**

**The distinction is real and worth keeping.** The typed Rust entries
(`SubInfoEntry`, `SubSafetyEntry`) genuinely do hold `buffer: [u8; RX_BUF]`, so
for them the const generic is load-bearing and `rx_buffer_for!` is the right
answer. The buffered entries — which is what every C/C++ subscription and the
depth>1 Rust subscriptions use — are trailing-allocated. One tree, two entry
shapes; a claim about "the arena buffer" that does not say which is not a claim
about anything.

## What already exists, verified

* `nros_cpp_subscription_options_t.rx_buffer_hint` (phase-402) on all three
  register variants, read by `read_subscription_options`, passed to
  `add_arena_subscription_c_callback` at `subscription.rs:301`. **Issue 0896's
  closing statement that there is no slot for a hint was true when filed and is
  not now.**
* The hint currently routes only the BACKEND size class. It does not reach
  `RX_BUF`, so the arena's trailing region is still sized from
  `config::DEFAULT_RX_BUF_SIZE` on every C/C++ subscription in the image.
* `_with_info` discards the hint entirely (`_rx_buffer_hint`).
* `nros_serdes::size::max_serialized_size` computes the bound from `FIELDS`;
  phase 380 provides it as `MAX_SERIALIZED_SIZE_XCDR{1,2}` on the Rust trait.
  Nothing emits it for C or C++.

## What has to be true when this is done

1. A C or C++ subscription to a bounded type sizes its arena trailing region
   from that type, not from `DEFAULT_RX_BUF_SIZE`.
2. The number is computed ONCE, by `max_serialized_size`, and asserted equal to
   the Rust const for the same type. Two implementations of "how big can this
   get" is the class this campaign keeps finding (0088 → 0114 → 0122 → 0123 →
   0245 → 0268).
3. An unbounded type still gets the configured default, and the diagnostic
   names the FIELD. `None` means unbounded, never unknown.
4. The saving is shown by `mem-report --json --baseline` on a named image, not
   asserted from a table.

## Waves

**W1 — emit the constant.** Issue 0896 layers 1–2: one traversal in
`rosidl-codegen` that builds the `nros_serdes::FieldType` value alongside the
existing expression string, so a new variant handled by one output and forgotten
by the other is a compile error. Then
`<PREFIX>_RX_MAX_SERIALIZED_SIZE_XCDR{1,2}` from `packs/c/message.h.jinja` and
the C++ sibling.

*Trap, from 0896:* nested types must be resolved to build a sizeable value, and
resolution can fail. **A failure to resolve must fail the generate**, not emit
"no bound" — otherwise this campaign's own defect returns wearing a different
hat.

Acceptance: emitted constant equals the Rust const for every type in the message
corpus, both encodings, one test.

**W2 — retarget the publish helper.** 0896 layer 3, lands with W1 and is
independent of everything after it. The generated helpers stack
`NROS_PUB_BUFFER_SIZE` (global, default 256, checked against nothing); per-type
is exact and strictly smaller for every type under 256 B.

Acceptance: no generated publish helper references the global.

**W3 — `RX_BUF` becomes a runtime argument on the buffered C path.** The
mechanical half of the finding above. `add_arena_subscription_c_callback` and
its two siblings take `rx_buf: usize` instead of `const RX_BUF: usize`; the
const generic remains only where an entry really is `[u8; N]`.

Acceptance: the three C++ register variants no longer name
`DEFAULT_RX_BUF_SIZE`, and a subscription created with a hint allocates trailing
bytes proportional to it — asserted in the executor tests, which already
construct these entries directly.

**W4 — deliver the number at the call site.** A generated `_subscribe` helper
per message type that fills `rx_buffer_hint` from W1's constant, so the C/C++
consumer gets the derived size without naming a number. This is 0896 layer 4,
now unblocked by phase-402's options struct.

Acceptance: a C example subscribing to a bounded type shows a smaller arena in
`mem-report --baseline` **without its source being edited**, beyond switching to
the generated helper.

**W5 — `_with_info` stops discarding the hint.** Small, and it is the variant a
component with `MessageInfo` uses.

## Explicitly out of scope

Rust-path defaults (W3c), Cyclone consuming the hint (W3f), unbounded
diagnostics and the TX/RX cap split (W3g). All three remain in phase 392.

**One consequence worth stating for whoever measures this:** the motivating
consumer (the ASI an536 lane) is a **Cyclone** image, and
`grep rx_buffer_hint packages/rmw/cyclonedds/` returns nothing. W3 here shrinks
its arena regardless — that is the executor's own allocation, not the backend's
— but the backend-class routing W1/W4 also feed will show no effect there until
phase-392 W3f exists. Measure the arena, not the backend, when validating this
phase on that lane.

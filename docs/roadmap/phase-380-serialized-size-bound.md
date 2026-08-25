# Phase 380 — a message's serialized size, computed instead of guessed

**Status (2026-08-26). W1 LANDED. W2-W5 open, and W0 below is a newly measured
PREREQUISITE.** Deferred deliberately from phase-376 W4, which established that
this is not an ABI question. Issue 0776 is the gap; this doc is the plan.

## Why

`NROS_SUBSCRIPTION_BUFFER_SIZE` defaults to 1024 bytes and is a GUESS. Nothing
checks it against the messages an image actually subscribes to, so when a sample
does not fit it is dropped — after the transport already ACKed it — and
`report_dropped_take` can only say "raise the knob", because the runtime does not
know what value would have worked. On a target that knob is static RAM nobody can
spare, so the loop is guess, drop, guess again. Issue 0757 is the same shape one
layer over: 13.4 KiB trajectories silently discarded, attributed only by a
consumer-side tshark session.

## What was already settled (phase-376 W4)

**This is not a vtable slot.** Nothing about a size bound varies by backend, and
upstream proves it by not varying at all: `librmw_cyclonedds_cpp.so` and
`librmw_fastrtps_cpp.so` both set the error string
`"rmw_get_serialized_message_size: unimplemented"` and return
`RMW_RET_UNSUPPORTED`, and NOTHING in a ROS 2 Humble install calls the symbol.

Upstream can afford that because its serialized buffer RESIZES — its bound is a
performance hint that saves a realloc. Ours cannot resize, so the same number is
load-bearing here. That is the argument for building it, and it is stronger than
"upstream has it and we do not".

**The input already exists.** `nros_serdes::schema` gives every generated message
a `Message::FIELDS: &'static [Field]`, and `FieldType` already distinguishes
fixed / bounded / unbounded. Nothing needs to reach into codegen — codegen
already emitted the schema. (This corrects issue 0776's first framing, which
called it a codegen capability.)

## Design

Studied against Fast-CDR, Fast-DDS and rosidl; the long form with evidence is in
`docs/issues/0776-no-serialized-size-bound.md`. In brief:

**Thread the offset, do not sum the maxima.** Fast-CDR's
`alignment(current, dataSize) = (dataSize - (current % dataSize)) & (dataSize - 1)`
says padding is a function of WHERE a field starts. A calculation that sums
per-field maxima is wrong whenever a variable-length field shifts what follows.

**Take rosidl's signature.** Its generated Fast-RTPS support declares
`max_serialized_size_T(bool &full_bounded, bool &is_plain, size_t current_alignment)`
— offset in, size out (so nested structs compose with no special case),
`full_bounded` cleared the moment an unbounded member appears, and `is_plain` as
a free by-product that decides loan / zero-copy eligibility.

```rust
pub struct SizeBound {
    pub bytes: usize,   // given the starting offset handed in
    pub bounded: bool,  // false once an unbounded member is reached
    pub plain: bool,    // no variable-length member: EXACT, and loan-eligible
}

pub const fn size_bound(
    fields: &'static [Field],
    version: EncodingVersion,
    current_alignment: usize,
) -> SizeBound;

pub trait Message {
    const MAX_SERIALIZED_SIZE: Option<usize>;  // None when !bounded
}
```

**Two questions, one walk.** Fast-DDS's runtime path is instance-based
(`getEstimatedSerializedSize(ros_message, impl)`), not type-based. Both are
wanted and they are not the same question:

| question | asked by | shape |
| --- | --- | --- |
| how large can this TYPE ever be? | build-time buffer sizing | `const MAX_SERIALIZED_SIZE` |
| how large is THIS message? | a publisher before publishing; a drop report | `fn serialized_size(&self)` |

The second is the only honest answer for an unbounded type. They share one
implementation or they will drift.

## Work items

**W0 — `FIELDS` does not exist everywhere (measured 2026-08-26).** This doc
claimed above that "the input already exists … every generated message". It does
not: **27 of 64** generated message structs in `packages/interfaces` carry no
`const FIELDS`, including `builtin_interfaces/Time`, `builtin_interfaces/Duration`
and every `rcl-interfaces` srv type. Only three crates emit it
(`nros-diagnostic-msgs`, `nros-std-msgs-diag`, `nros-builtin-interfaces-diag`);
`time.rs` was last generated 2026-03-18, before the schema emitter that
`generator/common.rs` now carries.

This gates the items that follow, and gates them asymmetrically:

* W1 works wherever `FIELDS` exists and is unaffected.
* W2's "for every type in `packages/interfaces`" can cover 37 of 64 today, so
  a green W2 would be reporting on a subset without saying so.
* **W4 is the one that matters.** A build-time assertion that cannot see a
  type's schema does not fail — it has nothing to check. A subscription to a
  `FIELDS`-less type would pass the build and keep dropping samples, which is
  precisely the silent-coverage shape this repo treats as worse than a red.

So W0 is: regenerate the affected crates (or find why that path skips the
emitter), and make a missing schema LOUD at the point W4 consults it rather
than absent. Do it before W4, not after.

**W1 — the calculator. LANDED (`nros-serdes::size`).** `size_bound()` over
`FIELDS`, both encoding versions.
Per-field contributions are tabulated in issue 0776. Three things that are easy
to get wrong and must be tested, not assumed:

* **The encoding version changes the answer.** `CdrWriter::align` caps alignment
  at 4 under XCDR2 while XCDR1 aligns 8-byte primitives to 8, so a message
  containing an `int64` has TWO bounds. A single constant is silently wrong for
  one encoding.
* **XCDR2 adds a 4-byte DHEADER per appendable struct.** Missing it
  UNDER-reports, which is the dangerous direction: an under-reported bound sizes
  a buffer too small and reintroduces the exact drop this phase exists to stop.
* **`+ 4` encapsulation is top-level only**, not per nested struct.

Landed as `packages/core/nros-serdes/src/size.rs`: `SizeBound { bytes, bounded,
plain }`, `size_bound(fields, version, current_alignment)` threading the offset,
and `max_serialized_size(fields, version) -> Option<usize>` which adds the
top-level encapsulation header and answers `None` when unbounded.

Every rule was read out of `CdrWriter` rather than the CDR spec, and the tests
compare the bound against bytes the writer actually produced — a
self-consistent bound proves nothing. Both encodings, 8 tests.

Two things the measurement corrected:

* **A single `int64` is NOT a discriminator between the encodings.** For
  `[u8, i64]` both come to 20 bytes: XCDR2's DHEADER costs exactly the 4 bytes
  its alignment cap saves. My first version of that test asserted they differ,
  failed, and would have kept passing with the cap deleted had I written it the
  other way round. The test now uses two `int64`s (the saving scales, the
  DHEADER does not) and the coincidence is pinned by its own test so nobody
  simplifies it back.
* **Negative controls, both dangerous directions.** Deleting the per-struct
  XCDR2 DHEADER fails 4 tests; removing the alignment cap fails the encoding
  test. Checked by injecting each defect, not by inspection.

**W2 — the property test.** Self-consistency proves nothing. For every type in
`packages/interfaces`, serialize a MAXIMAL instance and assert
`buf.len() <= MAX_SERIALIZED_SIZE`, with equality where `plain`. `compat_tests.rs`
already asserts byte offsets, so the machinery exists. Both encodings, or W1's
first defect passes the suite.

**W3 — the instance size.** `serialized_size(&self)`, sharing W1's walk. This is
what lets `report_dropped_take` name the number instead of saying "raise the
knob", and what lets a publisher check before it publishes.

**W4 — use it.** A build-time assertion that a subscription's buffer can hold its
own message type, so a too-small buffer fails the BUILD rather than dropping
samples in the field. This is the item that pays for the other three; W1–W3
without it just compute a number nobody consults.

**W5 — `plain` for loans.** `is_plain` falls out of W1 and answers a question two
existing vtable slots already ask (`borrow_loaned_message`,
`subscription_supports_in_place`). Wire it there rather than letting a second
notion of "fixed layout" grow.

## What this does NOT fix

An unbounded message has no bound, and that includes common ROS types —
`std_msgs/String` is `FieldType::String`. For those `MAX_SERIALIZED_SIZE` is
`None` and the runtime keeps whatever knob-sized buffer the integrator chose. W3
still improves the failure MESSAGE there, but no build-time check is possible.
This phase helps the messages that have a bound, which on a real embedded graph
is most sensor and control traffic — not all of it. Claiming otherwise would be
the more comfortable summary and the wrong one.

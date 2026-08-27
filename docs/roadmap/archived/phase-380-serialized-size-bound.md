# Phase 380 — a message's serialized size, computed instead of guessed

**Status: COMPLETE (2026-08-26). W0-W5 all landed.** The status line said "W4
landed CONDITIONALLY" for a while; the correction section below resolved that —
`bound_fits` (not `buffer_fits`) is the right predicate at a subscription site,
and with it W4 landed without blocking the unbounded types the first attempt
would have refused. Deferred deliberately from phase-376 W4, which established
this is not an ABI question. Issue 0776 was the gap and is resolved/archived.

Scope note, kept because it is easy to overstate: this helps messages that HAVE
a bound. `std_msgs/String` and friends are unbounded, get `None`, and keep
whatever knob-sized buffer the integrator chose — see "What this does NOT fix".

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

**W0 — `FIELDS` does not exist everywhere. LANDED (2026-08-26).** This doc
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

**Done. 64 of 64 now carry `FIELDS`.** The cause was vintage, not a conditional
emitter: `nros-builtin-interfaces` (0 of 2) and `nros-builtin-interfaces-diag`
(2 of 2) are the SAME two messages, and the difference is that the `-diag` crate
was regenerated 2026-07-17 while the other dates to 2026-03-18. Three crates had
simply never been regenerated since the emitter landed. `just
generate-rcl-interfaces` + `just generate-lifecycle-msgs` closed it.

Regenerating brought two things this phase did not ask for, both wanted:

* **XCDR2 support these crates did not have.** Their `serialize` /
  `deserialize` had no DHEADER wrap at all (phase-303 W4 / #0267), so they could
  not speak XCDR2 — and W1's bound distinguishes the two encodings for exactly
  those types.
* **A wire-visible name fix.** `TYPE_NAME` was
  `nros_builtin_interfaces::msg::dds_::Time_` — the `--rename` this repo applies
  to avoid crate-name collisions had leaked into the DDS type name peers match
  on at discovery. It is now `builtin_interfaces::msg::dds_::Time_`, matching the
  July-generated crates and `nros-lifecycle-msgs`, which never had the leak.
  Checked that nothing pins the old spelling.

Two traps worth naming for whoever regenerates next:

* The recipes REPLACE the generated `Cargo.toml`, dropping workspace inheritance
  (`version.workspace`, `edition.workspace`), `license` / `repository` /
  `description`, and `[package.metadata.ros]`. The lifecycle recipe says so in a
  closing NOTE; the rcl one does not. Restore the committed manifests after
  regenerating — this landing kept sources only.
* Codegen output is not rustfmt-clean; `check-workspace-fmt` catches it.

Guarded by `check-generated-schema-coverage` on the fast line, so the 27-of-64
state cannot return silently. Its message names the regeneration recipes and the
`Cargo.toml` trap, because a gate that only says "no" costs the next person the
same hour.

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

**W2 — the property test. LANDED.** Self-consistency proves nothing. For every type in
`packages/interfaces`, serialize a MAXIMAL instance and assert
`buf.len() <= MAX_SERIALIZED_SIZE`, with equality where `plain`. `compat_tests.rs`
already asserts byte offsets, so the machinery exists. Both encodings, or W1's
first defect passes the suite.

**W3 — the instance size. LANDED.** `serialized_size(&self)`, sharing W1's walk. This is
what lets `report_dropped_take` name the number instead of saying "raise the
knob", and what lets a publisher check before it publishes.

**W4 — use it. LANDED.** A build-time assertion that a subscription's buffer can hold its
own message type, so a too-small buffer fails the BUILD rather than dropping
samples in the field. This is the item that pays for the other three; W1–W3
without it just compute a number nobody consults.

**W5 — `plain` for loans. LANDED.** `is_plain` falls out of W1 and answers a question two
existing vtable slots already ask (`borrow_loaned_message`,
`subscription_supports_in_place`). Wire it there rather than letting a second
notion of "fixed layout" grow.

## What landed, 2026-08-26

**W2** — `packages/testing/nros-tests/tests/serialized_size_bound.rs` walks the
REAL `FIELDS` of the committed generated corpus and checks the bound against
`writer.position()`, both encodings: 14 bounded (type, encoding) pairs, 22
unbounded correctly answering `None`. The walk drives the writer from the SCHEMA
rather than from constructed values — 64 hand-written maximal literals would rot
and silently shrink coverage — and
`xcdr2_dheader_matches_generated_serialize` pins the one structural thing that
could differ, against the type's own `Serialize`.

Its floor (`checked >= 14`) is MEASURED, not chosen: it was 20 on first writing,
which was a guess, and the guess failing is what prompted counting. Most ROS
types reach a `string` and are unbounded.

**W3** — `size::serialized_size(&value, version)` runs the real writer with its
stores disabled (`CdrWriter::measuring`). Exact by construction: same alignment,
same DHEADERs, same `len + 1` string prefix, because it is the same code rather
than a second walk that could not see actual string lengths anyway. Six store
sites plus the padding loop are guarded; a sweep asserts none is left
unguarded, after `write_u8`'s single-byte store was missed by a
`copy_from_slice` grep and found by the test panicking.

**W5** — `size::is_loan_eligible::<M>()` is `M::IS_PLAIN`, so eligibility and
size-exactness cannot disagree. Wiring it into `borrow_loaned_message` /
`subscription_supports_in_place` is left to whoever owns those slots; the point
of W5 was to stop a second notion of "fixed layout" growing, and there is now
one definition to reference.

Also on `schema::Message`, as PROVIDED consts computed from `FIELDS`:
`MAX_SERIALIZED_SIZE_XCDR1`, `MAX_SERIALIZED_SIZE_XCDR2`, `IS_PLAIN`. Two sizes
because there are genuinely two.

## W4 correction (2026-08-26) — the bound DID block it, and broke a user pattern

The section below is kept because its reasoning is the record, but its
conclusion was wrong and was reverted the same day.

Tightening `MessageForRmw` to require `schema::Message` on every backend broke
`examples/native/rust/custom-msg`, whose `SensorReading` is a HAND-WRITTEN
message type implementing `RosMessage` + `Serialize` + `Deserialize` and no
schema. That example exists precisely to demonstrate that pattern, so the change
would have made codegen mandatory for anyone subscribing to a message they wrote
themselves — a far larger decision than a build assertion justifies.

It was found by a fixture build, not by my checks, and the reason is worth
recording: I verified the tightening with `cargo clippy --all-targets` over
`packages/`, which does not compile `examples/`. Twice in one session I called
this change free on the strength of a sweep that could not see the thing it
would break.

**What is landed instead.** `MessageForRmw` is unchanged. The assertion calls
`nros_node::rmw_type_registry::subscription_buffer_ok::<M>()`, which computes the
bound where the backend's `MessageForRmw` already carries a schema
(`rmw_needs_type_descriptors`) and returns `true` where it does not. So the
guarantee is automatic on descriptor backends, absent elsewhere, and no user
pattern is withdrawn. `size::bound_fits` stays public for an explicit assertion.

The cfg branch lives in `rmw_type_registry` beside the trait it describes rather
than at the call site, so "does this backend have schemas" keeps one spelling.

**What that leaves open.** A hand-written message type gets no build-time buffer
check on any backend, and no backend without descriptors gets one at all.
Closing that means either giving `schema::Message` to hand-written types (a
codegen/ergonomics question) or accepting the assertion as descriptor-only. That
is a real decision and it is not this phase's to force.

## W4 — landed, and the bound did not block it after all

`size::bound_fits::<M>(rx_buf)` is const, and `create_subscription_with_qos` —
the site every `create_subscription*` form funnels through — now carries

```rust
const { assert!(nros_serdes::size::bound_fits::<M>(DEFAULT_RX_BUF_SIZE), "...") }
```

so a subscription whose receive buffer provably cannot hold its own message type
fails the BUILD instead of dropping ACKed samples in the field.

**`bound_fits`, not `buffer_fits`, and the distinction is load-bearing.**
`buffer_fits` answers "is this GUARANTEED to fit", so an unbounded type is
`false` — no finite buffer fits a `String`. Asserting THAT at a subscription
site was my first attempt and would have refused the most common message in ROS.
`bound_fits` answers "can we PROVE it will not fit", so an unbounded type
passes. Both are kept, named, because the wrong one is silently plausible at
either call site. Both encodings are checked and the larger taken: the peer
picks the encoding at runtime, so sizing from XCDR1 alone is a trap.

**The trait-bound obstacle was real, and smaller than this doc claimed.** An
earlier revision said hand-written `RosMessage` types with no schema exist and
that tightening `MessageForRmw` would break them. Both halves needed correcting,
in opposite directions:

* A first re-check said the tightening was FREE. That was wrong — it did not
  pass `--all-targets`, so it never compiled the tests where those types live.
* The real cost is **seven** `#[cfg(test)]` fixtures in `nros-node` and `nros`,
  and six of them **already had a schema impl**, gated
  `#[cfg(rmw_needs_type_descriptors)]` with a comment explaining it exists "so
  `TypedPublisherBuilder::build` resolves under the cyclonedds-tightened bound".
  The codebase had already solved this for one backend.

So `MessageForRmw` now requires `schema::Message` on EVERY backend rather than
only where a descriptor registry needed it, and those six impls simply lose
their cfg gate; the seventh (`nros/src/node.rs`'s empty `TestMsg`) gains a
two-line one. No production type was affected — generated messages have carried
a schema since W0.

Verified by injecting `assert!(false)` and building a real subscriber
(`examples/native/rust/listener`): the constant IS monomorphised at genuine
call sites. With the real predicate that same build passes, which is the case
that matters — its `StringMsg` is unbounded, so it must NOT be refused.

## What this does NOT fix

An unbounded message has no bound, and that includes common ROS types —
`std_msgs/String` is `FieldType::String`. For those `MAX_SERIALIZED_SIZE` is
`None` and the runtime keeps whatever knob-sized buffer the integrator chose. W3
still improves the failure MESSAGE there, but no build-time check is possible.
This phase helps the messages that have a bound, which on a real embedded graph
is most sensor and control traffic — not all of it. Claiming otherwise would be
the more comfortable summary and the wrong one.

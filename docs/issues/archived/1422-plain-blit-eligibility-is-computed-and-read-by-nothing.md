---
id: 1422
title: "The plain (POD-blit, zero-copy eligible) flag is computed on every size
  bound and read by a test and a const - no dispatch path and no gate consumes
  it"
status: resolved
type: tech-debt
area: [codegen, memory, sizing]
severity: low
found: 2026-09-21
resolved: 2026-09-27
resolved_in: "phase-460 W3 second half — the codegen IR's copy deleted, the runtime's kept and corrected; RFC-0068 Amendment 3"
related: [issue-1369, issue-1368, issue-1400, issue-0814, phase-460, rfc-0033, rfc-0068, phase-380]
---

> **RESOLVED 2026-09-27. Two producers, two different answers.** There were not
> one flag and no consumer but TWO flags under one name, disagreeing on three of
> the commonest message shapes, neither with a consumer. The codegen IR's copy is
> **deleted**; the runtime's is **kept**, because it pins a property the walk
> really has, and its doc comment — which claimed a wiring nobody did — now says
> what is true and names the issue that would consume it. Detail below; the
> original filing is kept underneath it, unedited.

## Outcome

**Not wired.** A blit / typed-loan fast path is not close, and the tree's own
record says so on purpose:

* **issue 0814 step 5** is the wiring, and 0814's recommendation is *"Do not add a
  typed dual now: its natural consumer is `IS_PLAIN` over Cyclone-with-iceoryx,
  which no target we ship can reach"* — step 5 reads *"if and when a typed loan is
  ever built"*. 0814 also measures the byte-span loan surface it would sit beside
  as **strictly worse than `publish_raw` on three of four backends** (issue 1400,
  folded into 0814 the day it was filed).
* **phase-460 W3's nominated consumer is the wrong predicate.** W3 proposed
  selecting the arena's borrowed-view dispatch per type from an
  `NROS_ENTITY_PLAIN_TYPES` list instead of "the current runtime probe". Read at
  `4d439a115`: the borrowed-view path
  (`nros-node/src/executor/arena.rs:1438-1521`) exists *for* types with unbounded
  sequence/string members — the opposite population — and the mode is chosen by
  the user's codegen (`borrowed`), not probed. The in-place path
  (`:1256-1330`, `:1850-1891`) is selected by
  `handle.supports_process_in_place()`, a per-BACKEND property, and it hands the
  callback raw CDR to deserialize, which needs no fixed layout at all. Issue
  1369's option 1 spelled out the same wiring and named it correctly: it is a
  TIGHTENING that would take the in-place path away from every type with a
  `String`, i.e. most of them. That is a pessimisation, not a fast path.

**So: delete one producer, keep and correct the other.**

## Producer/consumer sets, re-verified at `4d439a115`

phase-465 reshaped `packages/interfaces`; it did not touch this. Nothing had
gained a consumer. What is there:

| site | role | consumers outside itself |
| --- | --- | --- |
| `packages/cli/rosidl-lower/src/lowered.rs:211` | `LoweredField::plain` | **1**: `lower()`, to compute `LoweredType::plain` |
| `packages/cli/rosidl-lower/src/lowered.rs:205` | `LoweredField::align` | **1**: the same line |
| `packages/cli/rosidl-lower/src/lowered.rs:278` | `LoweredType::plain` | **0** |
| `packages/cli/rosidl-lower/src/lowered.rs:275` | `LoweredType::align` | **0** |
| `packages/core/nros-serdes/src/size.rs:59` | `SizeBound::plain` | `schema.rs:171` + 6 test assertions |
| `packages/core/nros-serdes/src/schema.rs:170` | `Message::IS_PLAIN` | **1**: `size::is_loan_eligible` |
| `packages/core/nros-serdes/src/size.rs:569` | `is_loan_eligible::<M>()` | **0 non-test** (3 assertions in `serialized_size_bound.rs:387-400`) |

Two things the original filing did not have:

* **`rosidl_lower::lower()` and `LoweredType` have no non-test caller either.**
  Every real codegen path calls `lower_fields` directly
  (`rosidl-codegen/src/generator/{common,cpp}.rs`). So `LoweredType::plain` was
  unread on a struct nothing in production constructs. Residue, deliberately not
  acted on here — see below.
* **The two flags are different predicates.** Measured, not read:

  | shape | `rosidl-lower` `plain` | `nros-serdes` `plain` |
  | --- | --- | --- |
  | `bool flag` | **false** (CDR bool is a constrained `u8`) | **true** |
  | nested all-`float64` (`geometry_msgs/Pose`) | **false** (nested is hardcoded non-plain) | **true** |
  | `uint8 a; uint32 b` | **false** (`repr(C)` pads between them) | **true** (wire length still fixed) |

  `nros-serdes`' flag means "no variable-length member, so the wire length is
  EXACT". `rosidl-lower`'s meant "the memory image may be blitted". A consumer
  wired to "the plain flag" would have got a different answer depending on which
  producer it asked — which is exactly the second notion of "fixed layout" that
  phase-380 W5 shipped the first one to prevent.

## Cost, measured

`size_bound`'s `plain` term against a byte-for-byte clone of the walk with it
removed, 200 000 iterations × 7, best-of, `--release`, same binary:

| schema | full walk | plain-free clone | delta |
| --- | --- | --- | --- |
| `builtin_interfaces/Time` (2 scalars) | 2.68 ns | 2.37 ns | **0.31 ns** |
| 10 fields / 1090 leaf elements (arrays, bounded seq, nested, nested array) | 331.1 ns | 285.2 ns | **45.9 ns (16 %)** |

And **zero in any shipped image**: every non-test consumer in a shipped crate is a
const item or a `const fn` in a const context —
`Message::{MAX_SERIALIZED_SIZE_XCDR1,_XCDR2,IS_PLAIN}` are associated consts,
`rmw_type_registry::subscription_buffer_ok` is a `const fn` reached from
`const { assert!(…) }` at `packages/api/nros/src/node.rs:1061`. The only place the
walk runs as ordinary code is the host CLI at codegen time
(`rosidl-codegen/src/bounds.rs` → `generator/msg.rs:454`), twice per type, so a
30-type package pays ~20 µs total.

**That weakens the delete case for the runtime flag** — "computed on every size
bound at every encoding version" is true and costs nothing measurable — and leaves
the argument for the codegen IR's copy resting where it should: on being a second,
disagreeing definition rather than on being expensive.

## What changed

1. **Deleted** from `packages/cli/rosidl-lower/src/lowered.rs`: `LoweredField::{align,plain}`,
   `LoweredType::{align,plain}`, `NESTED_ALIGN_STANDIN`, `CdrOp::{cdr_size,is_plain_scalar}`.
   `element_facts` became `element_op_of` (the CDR op was its only surviving
   return). No generated output changes — none of this reached a renderer, and
   `packages/interfaces/**` is untouched.
2. **Kept** `SizeBound::plain`, `Message::IS_PLAIN` and `is_loan_eligible`, and
   rewrote all three doc comments. The old `size.rs:56-58` said phase-380 W5
   "wires this to `borrow_loaned_message` / `subscription_supports_in_place`",
   which it never did (issue 1369's whole point). They now state: diagnostic
   today, one definition on purpose, issue 0814 step 5 is the consumer, do not
   gate the in-place path on it, and it is not "POD-blit".
3. **Added** `plainness_is_version_independent_and_the_exact_size_is_not`
   (`size.rs`), because the property this flag pins was being described wrongly.
   The FLAG is version-independent; the bound is NOT — `Time` measures 8 bytes
   under XCDR1 and 12 under XCDR2 (issue 0776's defect), so "plain" never meant
   "one number serves both encodings".
4. **RFC-0068 Amendment 3** records the IR change; the Stage 2 diagram's `plain`
   and `align` lines and two rows of Amendment 1's table are marked superseded in
   place (one of them named a test that no longer exists).

## Acceptance

The flag is gone from one producer, every reader updated, and the properties it
pinned still asserted:

* **exactness** — `size::tests::plain_struct_bound_is_exact` and
  `serialized_size_bound.rs:207` (the whole generated corpus, both encodings);
* **version-independence of the flag, and version-DEPENDENCE of the bound** —
  the new `plainness_is_version_independent_and_the_exact_size_is_not`;
* **the IR claims no layout fact** —
  `lowered::tests::lowering_states_the_fields_and_no_layout_claim` and
  `a_nested_field_carries_no_target_fact` (successors to
  `struct_not_plain_when_mixed_alignment_or_strings` and
  `a_nested_field_is_never_plain_so_its_align_cannot_matter`).

`cargo test -p rosidl-lower --lib` 49/49; `cargo test -p nros-serdes --lib`
83/83; `cargo check -p nros-cli-core --workspace --all-targets` clean.

## Residue

`rosidl_lower::lower()` and `LoweredType` have no non-test caller: every codegen
path calls `lower_fields`. Not deleted here — `LoweredType` is RFC-0068's Stage 2
output and removing it is a design change, not the removal of a field the RFC's
own diagram already over-claimed. Whoever next touches Stage 2 should decide
whether the struct earns its place.

No gate was added, deliberately. A gate forbidding a consumer would fire exactly
when 0814 step 5 finally lands, which is the outcome we want; and a gate grepping
a doc comment for an issue number is an AUTHORED map of the kind that drifts
toward OK. The comment carries the claim instead, at the definition.

---

## What exists (verified at 783cdfa14)

* `packages/cli/rosidl-lower/src/lowered.rs:211` - `LoweredType::plain`,
  documented as "POD-blit eligible: every field plain AND all fields share
  one alignment", the property "a blit fast path would ask".
* `packages/core/nros-serdes/src/size.rs:59` - `SizeBound.plain`, computed by
  `size_bound` for every type at every encoding version, with the fixed-array
  and nested rules at `:132-143`.

## Who reads it

`grep -rn '\.plain\b' packages` outside the two producers finds
`packages/core/nros-serdes/src/schema.rs:171` (a per-type const) and
`packages/testing/nros-tests/tests/serialized_size_bound.rs:207` (a test
asserting the bound is version-independent when plain). No arena dispatch, no
inventory fact, no cmake carrier, no gate. The island's deployment report
(section 4 and its section 10 gap list) records the same: "computed on every
bound and consumed by nothing".

> **Correction (2026-09-27).** `serialized_size_bound.rs:207` asserts the bound is
> EXACT when plain, not that it is version-independent — a plain type has two
> bounds, one per encoding. The surviving flag is version-independent; the number
> it qualifies is not.

## Why file it

A computed fact with no reader is the shape issue 0196 and issue 1233 keep
finding one layer down: it costs nothing today and it is the first thing a
later change will silently break, because no test of a consumer will go red.
It also reads, in the size-bound docs, as though a fast path existed. Issue
1400 already found the loan API's value claim false on three backends; a
"zero-copy eligible" flag beside it invites the same misreading.

## What would fix it

phase-460 W3, second half: give it one consumer or delete it.

1. The inventory emits `NROS_ENTITY_PLAIN_TYPES`, the subscribed types that
   are blit-eligible.
2. The arena's borrowed-view dispatch (`packages/core/nros-node/src/executor/arena.rs`,
   zero-copy tests from line 3645) selects per type from that list rather
   than by a runtime probe.
3. Measure on one in-tree image. If no dispatch differs, delete the flag from
   both producers and the const, and say so in the wave.

> **Outcome on (1)/(2): declined, with the measurement above.** The borrowed-view
> path serves the opposite population and its mode is authored, not probed; the
> in-place path's probe is about the BACKEND and its dispatch needs no fixed
> layout. (3) was taken for one producer and refused for the other, because the
> runtime flag has a property beyond a hypothetical fast path.

## Acceptance

Either a dispatch test that changes behaviour with the flag flipped, or the
flag gone. Not the status quo.

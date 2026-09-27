---
id: 1369
title: "`SizeBound::plain` is computed on every bound walk and read by nothing
  but its own tests, and the comment beside it names a wiring that phase-380 W5
  explicitly declined to do"
status: resolved
type: tech-debt
area: [core, rmw]
severity: low
found: 2026-09-18
resolved: 2026-09-27
resolved_in: "issue 1422 — option 2 landed, and the second producer this file did not know about was deleted"
related: [issue-1422, issue-0814, issue-0781, issue-0776, phase-380, phase-460, rfc-0038, rfc-0068]
---

> **RESOLVED 2026-09-27 by the work recorded in
> [issue 1422](1422-plain-blit-eligibility-is-computed-and-read-by-nothing.md).**
> Same surface, and this file is the better-argued half of it: everything measured
> here re-resolved at `4d439a115`, including the zero non-test callers of
> `is_loan_eligible`, the single non-test reader of `IS_PLAIN`, the discard at
> `size.rs:423`, and the corrections about phase-403's unrelated W5.
>
> **Option 2 is what landed**, in the words this file asked for: `plain` is kept —
> it does pin a real property of the walk — and the doc comment at `size.rs:56-58`
> no longer asserts a wiring nobody did. It names issue 0814 step 5 as the consumer
> that would consume it, says in as many words that no slot reads it, and warns
> against the tightening option 1 described (gating the in-place path on
> `IS_PLAIN` takes it away from every type with a `String`, and that path
> deserializes from raw CDR, which needs no fixed layout).
>
> **Option 1 was declined with a measurement**, recorded in 1422: phase-460 W3's
> nominated consumer — the arena's borrowed-view dispatch — serves the OPPOSITE
> population (types with unbounded members) and its mode is authored by codegen
> rather than probed.
>
> **What 1422 added that this file did not have.** A SECOND producer:
> `rosidl-lower`'s `LoweredField::{align,plain}` and `LoweredType::{align,plain}`,
> "POD-blit eligible", also read by nothing — and a DIFFERENT predicate. Measured,
> the two disagreed on `bool`, on a nested all-`float64` struct and on
> `{uint8, uint32}`. This file's own framing turns out to be the reason that
> mattered: the growth W5 existed to prevent had already happened, one workspace
> over. That copy is deleted (RFC-0068 Amendment 3).
>
> One thing this file's cost paragraph left open, now measured: the `plain` term
> costs **0.31 ns** per walk on `builtin_interfaces/Time` and **45.9 ns (16 %)** on
> a 1090-element schema, and **zero in any shipped image** — every non-test
> consumer in a shipped crate is const-evaluated, so the only place the walk runs
> as ordinary code is the host CLI at codegen time. "The cost is small" was right;
> it is now a number.

## What the comment promises

`packages/core/nros-serdes/src/size.rs:47-60` declares the walk's result type.
Its third field carries a promise:

```rust
    /// No variable-length member anywhere, so the layout is fixed: `bytes` is
    /// EXACT rather than an upper bound, and the type is loan-eligible
    /// (phase-380 W5 wires this to `borrow_loaned_message` /
    /// `subscription_supports_in_place` rather than letting a second notion of
    /// "fixed layout" grow).
    pub plain: bool,
```

Read as written, `size.rs:56-58` says the wiring exists. It does not, and
phase-380 says so in its own record.

## What the record actually says

`docs/roadmap/archived/phase-380-serialized-size-bound.md:200-203` is the PLAN
for W5, and it is an instruction, not a report:

> **W5 - `plain` for loans. LANDED.** `is_plain` falls out of W1 and answers a
> question two existing vtable slots already ask (`borrow_loaned_message`,
> `subscription_supports_in_place`). Wire it there rather than letting a second
> notion of "fixed layout" grow.

`phase-380-serialized-size-bound.md:228-232` is what landed, and it withdraws
the second half:

> **W5** - `size::is_loan_eligible::<M>()` is `M::IS_PLAIN`, so eligibility and
> size-exactness cannot disagree. Wiring it into `borrow_loaned_message` /
> `subscription_supports_in_place` **is left to whoever owns those slots**; the
> point of W5 was to stop a second notion of "fixed layout" growing, and there
> is now one definition to reference.

So W5 shipped a DEFINITION and deferred the CONSUMER. The doc comment in
`size.rs` reports only the first sentence of the plan, in the present tense.

## The consumer count, measured

Every reference to the flag in nano-ros's own packages:

| site | what it does |
| --- | --- |
| `packages/core/nros-serdes/src/size.rs:59` | the field itself |
| `packages/core/nros-serdes/src/size.rs:162,193` | the walk propagating it |
| `packages/core/nros-serdes/src/schema.rs:170-171` | `Message::IS_PLAIN`, a provided const |
| `packages/core/nros-serdes/src/size.rs:569-571` | `is_loan_eligible::<M>()`, which is `M::IS_PLAIN` and nothing else |
| `packages/core/nros-serdes/src/size.rs:966,987,1064,1096` | `#[cfg(test)]` assertions |
| `packages/testing/nros-tests/tests/serialized_size_bound.rs:207,389-400` | integration assertions |

`is_loan_eligible` has ZERO non-test call sites. `IS_PLAIN` has exactly one
non-test reader, which is `is_loan_eligible`. Neither
`borrow_loaned_message` nor `subscription_supports_in_place` mentions either
name anywhere in `packages/`.

## It is not free, and the discard site is one line

`packages/core/nros-serdes/src/size.rs:416-429`, `max_serialized_size`, is the
function the build path actually calls:

```rust
    let bound = size_bound(fields, version, 0);
    if bound.bounded {
        Some(ENCAPSULATION_HEADER_BYTES + bound.bytes)
    } else {
        None
    }
```

`bound.plain` is computed by the walk and dropped at `size.rs:423`. That walk
is not hypothetical: `bound_fits` (`size.rs:547`) const-evaluates
`MAX_SERIALIZED_SIZE_XCDR1` / `_XCDR2`, and `bound_fits` has a real caller in
`packages/core/nros-node/src/rmw_type_registry.rs:277`. Every message type
whose bound is checked at build time therefore has `plain` computed and thrown
away in the same expression.

The cost is small. The cost is not the complaint. A flag whose doc comment
states a wiring that does not exist is a reader telling you that
`supports_process_in_place` consults the schema, and it does not: on zenoh it
is `fn(&self) -> bool { true }` (issue 1340's measurement), on XRCE it is an
unconditional `true`.

## Overlap, and what this issue adds

Issue 0814 already records the gap at
`docs/issues/0814-lending-never-exercised-on-hardware.md:237-238` and asks for
the wiring as its step 5 (`0814-lending-never-exercised-on-hardware.md:352`).
0814 is about the LENDING SURFACE being unexercised on hardware. This issue is
narrower and is about the COMMENT: 0814 can close with the lending surface
still behind `feature = "lending"`, and `size.rs:56-58` would go on asserting a
wiring nobody did.

## What would fix it

Either direction closes it; both are small.

1. **Land the consumer.** `supports_process_in_place` on the Rust typed path
   has one call site (`register_subscription_buffered_on`, per issue 1340), and
   it is the only place an `M` is in hand. Making it `M::IS_PLAIN &&
   handle.supports_process_in_place()` is the wiring W5 described. Note this is
   a TIGHTENING: it would take the in-place path away from every type with a
   `String`, which is most of them, so it is a measurable change and not a
   comment fix.
2. **Say what is true.** Keep `plain` (it earns its place: `size.rs:966-972`
   and `serialized_size_bound.rs:207-212` use it to assert that a plain type's
   bound is EXACT rather than an upper bound, which is a real property of the
   walk), and rewrite `size.rs:56-58` to say the definition exists and no slot
   reads it, citing this issue.

Doing 2 now costs nothing and stops the comment lying while 1 waits on
issue 0814.

## Corrections to the report that opened this

* The claim was that the only consumers live in vendored rclrs under
  `packages/cli/third-party/play_launch/src/vendor/ros2_rust/`. That path does
  not exist. `packages/cli/third-party/play_launch` is a gitlink with no
  `.gitmodules` entry, and it is empty in a fresh checkout, so there are no
  consumers there either. The accurate statement is stronger: outside
  `nros-serdes` and the tests listed above, there are none anywhere.
* The claim was that phase-403's header records W5 as unlanded.
  `docs/roadmap/phase-403-type-bound-rx-sizing.md:4` does say "W2, W3 (the
  caller-supplied-buffer half) and W5 remain", but phase-403's W5 is a
  different wave: `phase-403-type-bound-rx-sizing.md:473` defines it as "arena
  slots. Issue 0900's remaining half." It says nothing about `plain`. The
  relevant record is phase-380's, which marks ITS W5 **LANDED** and defers only
  the wiring.

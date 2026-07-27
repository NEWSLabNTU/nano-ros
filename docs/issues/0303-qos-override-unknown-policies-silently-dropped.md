---
id: 303
title: "A `qos_overrides.*` policy the bake doesn't model (deadline, lifespan, liveliness) is silently DROPPED — the opposite of the stated design"
status: open
type: bug
severity: medium
area: codegen, rmw
related: [issue-0241, rfc-0050]
---

## Finding (2026-07-28, while porting QoS overrides to the C and Rust paths)

`QosOverrideValue` (`packages/core/nros-rmw/src/traits.rs:396`) models four
policies:

```rust
pub enum QosOverrideValue {
    Reliability(..), Durability(..), History(..), Depth(u32),
}
```

`QosSettings` itself carries more — `deadline_ms`, `lifespan_ms`,
`liveliness_kind`, `liveliness_lease_ms` (phase-301 / issue 0241) — and ROS 2's
`qos_overrides.<topic>.<role>.<policy>` convention covers them too. A model
that says

```yaml
params:
  qos_overrides./chatter.publisher.deadline: 100
```

is therefore **dropped on the floor**, in every language: each lowering
(`emit_cpp::qos_override_codes`, which `emit_c` shares, and the proc-macro's
`qos_override_codes_for`) returns `None` for an unmodelled policy, and the
callers `filter_map` it away. No warning, no error, no override. The user's
deadline declaration simply does not exist in the built image.

## Why this is worse than a missing feature

The type's own doc comment states the opposite guarantee:

> A typed enum (not a string) so the codegen that bakes these from the plan
> catches an unknown policy / mistyped value **at generation time rather than
> silently no-op-ing at runtime**.

The typed enum does make the *runtime* safe, but nothing at generation time
reports the rejection — the skip happens inside a `filter_map`. So the design
intent is written down and unimplemented, and a reader of the code has no
reason to doubt it.

The same `return None` also swallows a genuine typo: `qos_overrides./t.pub.reliability`
(`pub` instead of `publisher`) or `.reliablity` produces no override and no
diagnostic. For a QoS setting, silence is the wrong failure mode — the system
runs with different delivery semantics than the model declares, which is
precisely the launch-vs-image divergence the SystemModel exists to prevent.

## Impact

- Any `qos_overrides` policy outside the four modelled ones is inert.
- Any misspelled topic/role/policy is inert and unreported.
- Both are invisible until someone measures the wire.

## Direction

1. **Fail loud at generation time** (the cheap half, and what the doc already
   promises). The lowering should return a Result, and an unrecognised role or
   policy should be a codegen ERROR naming the offending parameter key and the
   accepted spellings — not a filtered-out `None`. Do this even before (2): it
   converts every case below into a build failure with a message instead of a
   silent behavioural difference.
2. **Model the remaining policies.** Extend `QosOverrideValue` with
   `Deadline(u32)`, `Lifespan(u32)`, `Liveliness(..)`, `LivelinessLease(u32)`,
   the code table (a shared numbering across `nros_qos_override_t`,
   `nros_cpp_qos_override_t`, and `NodeRecord::QosOverrideCode`), and the fold.
   Note `DURATION_INFINITE_MS` (issue 0241) is the explicit-infinite spelling
   for the three duration fields — an override must be able to express it.
3. **One lowering, not three.** The role/policy → code mapping is currently
   spelled twice (the CLI emitters share one copy; the proc-macro has its own
   because it cannot depend on the CLI). Adding four policies doubles that
   drift surface. Per RFC-0050 §"Semantics ship with the schema", the mapping
   belongs in one crate both consumers already depend on — `nros-rmw` is the
   natural home, beside the enum it decodes into.

Step 1 alone closes the silent-failure half and is independently shippable.

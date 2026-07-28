---
id: 323
title: "Parameter wire values are silently truncated: from_rcl_value/to_rcl_value discard every capacity error and report success; unknown type_ becomes NotSet"
status: open
type: bug
severity: high
area: core
related: [issue-0223, issue-0224]
---

## Finding (audit 2026-07-28, P1 — lead-verified)

`packages/core/nros-node/src/parameter_services.rs:122-172` — `from_rcl_value`
converts an inbound rcl_interfaces `ParameterValue` into the internal value, and
**every capacity result is discarded**:

```rust
4 => { let mut s = heapless::String::new();
       let _ = s.push_str(value.string_value.as_str());      // over-long → truncated
       InternalValue::String(s) }
7 => { let mut v = heapless::Vec::new();
       for &i in value.integer_array_value.iter() { let _ = v.push(i); }  // tail dropped
       InternalValue::IntegerArray(v) }
…
_ => InternalValue::NotSet,                                   // unknown type_ swallowed
```

The truncated value is then stored as the parameter's real value, and the
`SetParameters` reply still reports success — so a ROS 2 client believes it set
`"abcdef…"` or a 40-element array while the node holds a prefix. An unrecognised
`type_` code becomes `NotSet` rather than an error.

Same class on the reply side and in the hosted API:

- `parameter_services.rs:82-114` — `to_rcl_value` discards `push_str`/`push`
  results, so a stored value that exceeds the response message capacity is
  returned to `GetParameters` as a truncated string / short array.
- `packages/core/nros-params/src/types.rs:471,487,503,519` — the
  `feature = "std"` `ParameterVariant` impls use `unwrap_or_default()`: an
  over-long `String` silently becomes `ParameterValue::NotSet` (a **type**
  change, via `#[derive(Default)]` at :47) and an oversized `Vec<i64>/<f64>/<bool>`
  becomes an **empty** array. `Vec<String>` (:533-542) silently skips elements
  that don't fit.

This is the #223/#224 class (swallowed CDR/capacity errors turning malformed
input into a plausible business value) on the parameter surface rather than the
action surface.

## Not part of this issue

`handle_set_parameters_atomically:337` was initially flagged for reporting
`successful = true` while discarding `set()`/`declare()` Results. That was
**refuted**: a pre-check loop at :309-336 validates read-only, type, range and
fullness, mirroring `set()`'s failure modes. The discarded Results and the
absent rollback remain defence-in-depth debt only — noted as P3 in
`docs/development/audit-findings-2026-07-28.md`, not tracked here. Note however
that the truncation above happens *inside* that pre-check (`from_rcl_value` is
called at :310), so an over-capacity value passes validation and applies as a
shortened value with `successful = true`.

## Fix

1. `from_rcl_value` → `Result<InternalValue, ParamError>`; propagate
   `push_str`/`push` failures as an out-of-range / invalid-value rejection and map
   unknown `type_` to an explicit error, so `SetParameters` replies
   `successful=false` with a reason naming the parameter.
2. `to_rcl_value` → `Result`; reply with an explicit error rather than shipping a
   shortened value.
3. `nros-params::types` — add `try_to_parameter_value` (or make the conversion
   fallible) so oversize hosted input is rejected at the declare/set boundary
   instead of becoming `NotSet` / empty.

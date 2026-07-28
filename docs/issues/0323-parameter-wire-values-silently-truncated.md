---
id: 323
title: "Parameter wire values are silently truncated: from_rcl_value/to_rcl_value discard every capacity error and report success; unknown type_ becomes NotSet"
status: resolved
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

## Resolved (2026-07-28)

All three items, each with a test that documents the OLD behaviour alongside
the new one — the legacy shape is asserted explicitly so a future reader can
see what was wrong, not just that something changed.

### Items 1 + 2 — `parameter_services.rs`

`from_rcl_value` and `to_rcl_value` are now
`Result<_, ValueConversionError>`, with `CapacityExceeded` and
`UnknownType(u8)`. Every discarded `push` / `push_str` propagates.

Call sites:

- **SetParameters** — a failing conversion pushes
  `conversion_failure_result(e)`: `successful = false` with a reason naming the
  cause, instead of storing a prefix and replying success.
- **SetParametersAtomically** — the pre-check now fails the whole batch. This
  is where the issue's sharpest observation landed: the truncation happened
  *inside* the pre-check, so an over-capacity value passed validation and
  applied as a shortened value with `successful = true`. The apply loop skips
  rather than unwraps, so a future divergence between the two loops cannot
  panic mid-batch.
- **GetParameters** — replies `NOT_SET` when a stored value does not fit the
  response message. The service has no per-parameter error channel, so this is
  the only in-protocol way to avoid handing back a plausible-looking
  truncation; a client can distinguish "not set" from a real value, which it
  could not do before.

### Item 3 — `nros-params::types`

Added `ParameterVariant::try_to_parameter_value() -> Result<ParameterValue,
CapacityExceeded>`, defaulted to the infallible conversion (so impls whose
values cannot overflow are unaffected) and **overridden by the five hosted
`std` impls**: `String`, `Vec<i64>`, `Vec<f64>`, `Vec<bool>`, `Vec<String>`.

`Parameter::set()` uses it and maps failure to the existing
`ParameterError::StringConversion`, so oversize hosted input is rejected at the
declare/set boundary rather than becoming `NotSet` (a *type* change) or an
empty array.

The infallible `to_parameter_value` is deliberately kept: it is a public trait
method, and making it fallible would break every implementor. The named
`CapacityExceeded` type rather than `()` keeps clippy's `result_unit_err`
satisfied and reads better at the call site.

### Receipts

- New tests in `parameter_services`: a 40-element array (the issue's own
  example — the wire message holds 64, `MAX_ARRAY_LEN` is 32) is rejected;
  `type_ = 99` is `UnknownType(99)` not `NotSet`; the rejection reaches the
  wire as `successful = false` with a reason.
- New tests in `nros-params::types` assert BOTH behaviours: that
  `to_parameter_value` still yields `NotSet` / an empty array (documenting the
  legacy path) and that `try_to_parameter_value` rejects.
- `cargo test -p nros-node --features param-services`: 246 pass.
  `cargo test -p nros-params --features std`: 46 pass.
- `just check` green.

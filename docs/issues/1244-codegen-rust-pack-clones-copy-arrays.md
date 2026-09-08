---
id: 1244
title: "the `packs/rust` idiomatic pack calls `.clone()` on `Copy` arrays — four `clippy::clone_on_copy` sites in every message with an array field"
status: open
type: tech-debt
area: codegen
severity: low
found: 2026-09-09
related: [1230]
---

# What was measured

`packages/cli/rosidl-codegen/packs/rust/message.rs.jinja` emits, in both
directions of the idiomatic ↔ rmw conversion:

```jinja
{% elif field.kind == "PrimitiveArray" %}
// [primitive; N] arrays can be cloned directly
{{ field.name }}: idiomatic.{{ field.name }}.clone(),
```

and the same for `LargeArray` (lines 67, 85, 173, 191). An array of primitives
is `Copy` for every `N`, so the clone is a no-op the compiler already inserts,
and clippy says so:

```
error: using `clone` on type `[i32; 5]` which implements the `Copy` trait
  --> …/messages.rs:455:26
    |
455 |             small_array: idiomatic.small_array.clone(),
    = note: `#[deny(clippy::clone_on_copy)]` implied by `#[deny(clippy::all)]`
```

Four sites for a single two-field message (`int32[5]` + `int32[32]`, both
conversion directions). Measured 2026-09-09 against rust-1.98.0 clippy, via the
`generated_message_crate` compile-check fixture issue 1230 added.

The other `.clone()` calls in that template are NOT this: lines 88 and 116 clone
a `Vec` on the way into `BoundedSequence` / `Sequence`, which is a real copy.
Only the two array branches are `Copy`.

# Why it was invisible until now

`rosidl-codegen`'s `test_clippy_no_warnings` could not see it, twice over. It
generated `TestMsg` (`int32 value` / `string name`), which has no array field
at all; and it failed only when clippy's stderr contained the substring
`"error"`, so under `-W clippy::all` — which can emit warnings and nothing else
— any lint it did find would have passed anyway. Issue 1230 replaced that test
with a build-stage fixture that denies the lints in the crate under check, over
four message shapes including the array one, and this is the first thing the
new verdict found.

# Why it is not fixed in 1230's change

Editing an emitter template moves emitted BYTES, which is a different review
than "move a compile out of a test":

* `codegen_golden` (on the fast line, via `check codegen-size-bound-golden`)
  diffs the emit corpus against committed expected output. The change is a
  deliberate re-record (`NROS_UPDATE_GOLDEN=1`), and that diff is the review
  artifact.
* `packs/rust` is the **ros2_rust-shaped** pack, and `comparison_test.rs` /
  `parity_test.rs` compare our output against ros2_rust's. Whether ros2_rust's
  own generator writes the same `.clone()` decides whether dropping it is a
  parity DIVERGENCE or a parity fix, and that has to be checked rather than
  assumed.

So the fixture allows exactly this lint, at the narrowest scope that works (a
`#[allow(clippy::clone_on_copy)]` on each emitted idiomatic module, carrying
this issue id), and the rest of `clippy::all` stays denied — the repo's rule for
a gap that is real but open.

# Blast radius

`generate_message_package` and its `packs/rust` siblings have **no production
caller**: the `nros` CLI emits through `generate_nros_*` / `packs/nros` and the
C/C++ packs. So this is dead-lint on an emitter nobody ships today, which is why
it is `severity: low` — and also why the parity question above is the only real
cost of fixing it.

# What would close it

Drop `.clone()` from the `PrimitiveArray` and `LargeArray` branches in both
conversion directions, re-record the golden corpus, confirm the parity ledger,
and remove the `#[allow]` (and this issue id) from
`packages/testing/nros-tests/fixtures/generated_message_crate/build.rs`. The
allow's presence is the reminder: with the lint denied, forgetting the last step
is a red.

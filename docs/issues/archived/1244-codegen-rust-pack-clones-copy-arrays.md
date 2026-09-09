---
id: 1244
title: "the `packs/rust` idiomatic pack calls `.clone()` on `Copy` arrays — four `clippy::clone_on_copy` sites in every message with an array field"
status: resolved
type: tech-debt
area: codegen
severity: low
found: 2026-09-09
resolved: 2026-09-09
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

# Resolved 2026-09-09

All four branches now emit a plain copy (`{{ field.name }}: idiomatic.{{ field.name }},`),
and the `#[allow(clippy::clone_on_copy)]` is gone from the fixture's `build.rs`
with no replacement — `clippy::all` is denied over the emitted idiomatic layer
with nothing exempted.

**What the golden re-record actually moved.** Two files, four hunks, eight
lines, in `tests/fixtures/rust-surface-golden/`: `Bounded.idiomatic.rs`
(`fixed`, both directions) and `Shapes.idiomatic.rs` (`arr_fixed`, both
directions). Each hunk is one field line losing `.clone()` plus the one comment
line above it. Nothing else in either file moved and no other golden file
changed. (The emitted comment deliberately does NOT carry this issue's number —
that text ships to a user's `generated/` tree; the rationale lives on
`FieldKind::LargeArray` in `templates.rs` and in the regression test.)

`codegen_golden` — the corpus this issue named — did **not** move, and that is
the right answer rather than a miss: `emit_corpus()` renders the `nros`, `c` and
`cpp` surfaces only. The `rmw` and idiomatic Rust surfaces are covered by
`rust_surface_golden` (phase-432 W2.5a), which is deliberately NOT wired into
`emit_corpus` so that a Rust-surface edit does not stale every workspace fixture
in the tree. (The `codegen_fingerprint` *does* move anyway, because it hashes
every bundled pack's TEXT — but no committed byte tracks that.)

**The fixture is a real gate, measured in both directions.** With the emitter
fixed, `cargo clippy` on the staged `generated_message_crate` exits 0 with no
`#[allow(clippy::clone_on_copy)]` anywhere. Putting the `.clone()` back (one
`sed` on the template, restored afterwards) makes the same command exit 101 with
`using \`clone\` on type \`[i32; 5]\`` and `\`[i32; 32]\`` under
`#[deny(clippy::clone_on_copy)] implied by #[deny(clippy::all)]`. A green here
is therefore a verdict, not a lane that could not fail.

**Parity: toward the reference, not away.** Two mechanisms, and neither objects.
`parity-expected-failures.txt` is a parse/generate ledger, still empty — removing
a `.clone()` adds no rejection. `comparison_test.rs`'s byte comparison against
`tests/fixtures/reference_outputs/` covers exactly three messages —
`std_msgs/Bool`, `std_msgs/String`, `geometry_msgs/Point` — and **none of them
has an array field**, so no reference byte is in play either way. On the
direction question: the reference outputs convert BY VALUE
(`fn from(rmw: crate::msg::rmw::Point) -> Self`) and write plain field copies
(`x: rmw.x,`), with no `.clone()` anywhere. Our pack converts by reference, which
is a pre-existing structural divergence this change does not touch; within it,
`arr_fixed: rmw.arr_fixed,` is the reference's own spelling and
`arr_fixed: rmw.arr_fixed.clone(),` was not. So the move is toward parity.

**One coverage gap this exposed, and how it is covered now.** The `LargeArray`
branch (N > 32) is emitted by NOTHING in either golden or in the compile-check
fixture: the fingerprint corpus's largest array is `int32[4]`/`float64[3]`, and
`ArrayMsg` stops at exactly `int32[32]`, which is `PrimitiveArray`. Extending
the shared corpus would have moved the C/C++/nros goldens and the fingerprint
for a reason unrelated to this fix, and extending `ArrayMsg` would need a
`serde_big_array` dependency the fixture does not carry (the emitter writes
`#[serde(with = "serde_big_array::BigArray")]` for a large array). So the guard
is a string assertion instead:
`edge_case_test::copy_arrays_are_not_cloned_in_either_direction` generates
`int32[5]` + `int32[40]` and asserts both directions of both fields emit the
copy and not the clone.

**One thing measured on the way that is NOT this issue.** `FieldKind::LargeArray`
is assigned for `len > 32` *whatever the element is*, and the branch has no
element conversion — so `string[40]` emits `field: idiomatic.field` between
`[std::string::String; 40]` and `[crate::rosidl_runtime_rs::String; 40]`, which
does not compile. It did not compile before this change either (the `.clone()`
was a type error there, not a `clone_on_copy`), so this is pre-existing and
untouched; it is recorded here rather than silently inherited.

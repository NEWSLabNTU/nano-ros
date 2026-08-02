---
id: 319
title: "`cyclonedds-ci` red on main for two days: the #267 regression test hand-builds a `kinds[]` table in the pre-#267 flat layout, which the preorder walker cannot express"
status: resolved
type: bug
area: testing
related: [issue-0267, issue-0316]
---

## Finding (2026-07-28)

`just cyclonedds-ci` fails, so `just ci` cannot pass on main:

```
10/16 Test #10: nros_rmw_cyclonedds_dynamic_bridge_seq_nested ...***Failed
FAIL dynamic_bridge_seq_nested.cpp:357  bridge returned NULL, err=-1002
94% tests passed, 1 tests failed out of 16
```

`-1002` is `UnsupportedFieldType` (`bridge.rs:134`). The failing case,
`test_nested_msize_covers_large_member`, guards the autoware
`Control` heap-overrun from issue 0267: a top-level struct whose second field
is a nested struct **larger than 16 bytes**, where the old 16-byte placeholder
made `m_size` come out 32 instead of 48 and `dds_stream_read_sample` then wrote
past the sample buffer.

Both the test and the fix it guards landed together in
`0a8f30ccb fix(#267): correct Cyclone descriptor for depth-2 nested types`
(2026-07-26), so **the test has been red since it was committed** — two days.
It was not filed.

## Cause: two child-indexing conventions in one file

`bridge/dynamic_type_builder.cpp` reaches a nested node's children two
different ways:

| function | first child | steps to next sibling by |
| --- | --- | --- |
| `emit_nested_body` (:585) | `k.inner` — an explicit index | `kind_span(child)` |
| `kind_span` (:456) | `idx + 1` — **strict preorder**, ignores `inner` | recursion |

These agree only when `inner == idx + 1`. That invariant does hold for every
real table: `dynamic_type.rs::push_field_type` allocates `my_idx`, then
immediately pushes the children, so `first_child` *is* `my_idx + 1`
(`dynamic_type.rs:597-605`). Real producers emit preorder, and `kind_span` is
correct for them.

The test's hand-written table is not preorder. It uses the older flat layout —
one entry per top-level field first, children appended afterwards:

```c
// [0] Small (top field "a")   inner=2  -> children [2],[3]
// [1] Large (top field "big") inner=4  -> children [4]..[9]
// [4],[5] Large.{a,b} — each a Small, both inner=2  ← ALIASED onto [0]'s children
```

Neither node satisfies `inner == idx + 1`, and `[4]`/`[5]` *share* the subtree
at `[2],[3]` — a DAG, which a preorder table cannot represent at all.

Walking it: `emit_nested_body` starts at `inner = 4`, then advances by
`kind_span(4)`. `kind_span` assumes `[4]`'s children are at `[5],[6]`, computes
a span of 5, and lands on `[9]`; the next step runs past `kind_count = 10` and
trips the bounds check that reports `UnsupportedFieldType`.

`kind_span`'s own doc comment anticipates exactly this and is why the bug is
narrow:

> *(For leaf children span==1, which is why hand-built leaf-only tables — and
> the 1-level top-level path that walks `fields[]` instead — never exposed
> this.)*

Every other hand-built table in the suite is leaf-only or depth-1, so this is
the only one that trips it.

## Fix

Rewrite the table in preorder, which is what the API actually accepts:

- `[0]` Small with children `[1],[2]`; `[3]` Large with children `[4]`, `[7]`,
  `[10]..[13]`.
- The two `Small` members of `Large` are **spelled out separately** (`[4]` and
  `[7]`) rather than both pointing at `[1],[2]` — a preorder table cannot share
  a subtree.
- `fields[]`'s entry for `"big"` becomes kind index **3, not 1**: `[1],[2]` are
  `"a"`'s children.
- `kind_count` 10 → 14.

**The production code is unchanged.** The `m_size >= 48` assertion — the whole
point of the test, and the thing the 0267 fix delivers — passes once the
descriptor builds. The fix was right; only its regression test was written
against the layout the same commit replaced.

## Receipts

- `ctest -R seq_nested` → Passed, including `m_size >= 48`.
- `just cyclonedds-ci` → rc=0, **16/16** (was 15/16).
- Reproduced on a **wiped** build dir before diagnosing, so this was not the
  incremental-build false red of issue 0268.

## Why it survived

`cyclonedds-ci` is a `just ci` step but not a `just check` step, and `just
check` is what most work runs. A red landed in the heavier lane and sat there.
Same shape as issue 0314: the gate that would have caught it is not in the loop
people actually use.

## Follow-up: the two conventions are now one (2026-07-28)

The first fix left `emit_nested_body` reading `k.inner` while `kind_span`
assumed `idx + 1` — safe only because every producer happened to satisfy both.
That latent trap is now closed.

### Why `inner` cannot be the authority

The obvious unification — make everything trust `inner` — is impossible, and
the reason is in the format rather than the code. An entry records a child
COUNT (`bound`) and a FIRST-child index (`inner`), but never the index of child
*i+1*. Locating child *i+1* therefore requires child *i*'s subtree SIZE, and a
size is only well defined when the subtree is contiguous. An arbitrary `inner`
is **unrepresentable, not merely unimplemented**.

So preorder is forced by the format, and `inner` is redundant: it always equals
`idx + 1`. That is exactly what `push_field_type` emits — and, confirmed while
doing this, at the TOP level too: the caller loop appends each field's whole
subtree before the next field, so a top-level field's kind index is *not* its
ordinal.

### What changed

Deleting `inner` would be an ABI break across the hand-kept Rust/C++ mirror and
every producer, so it stays — but it can no longer disagree:

- `kind_first_child(idx)` is the single expression of the rule; `kind_span` and
  both child-walking loops call it instead of spelling `idx + 1` or `k.inner`.
- `validate_kind_table()` runs at the entry point, before any walk, and rejects
  any aggregate whose `inner != idx + 1` with a NEW distinct code,
  `NROS_BRIDGE_ERR_MALFORMED_KIND_TABLE` (-1005), mirrored into Rust as
  `BridgeError::MalformedKindTable` / `BuildError::MalformedKindTable`. A
  malformed table now says so instead of being reported as an unsupported field
  type from a bounds failure deep in the walk.
- A dead block in `emit_nested_body` went with it — a ternary whose two
  branches both yielded `child_idx`, assigned to a variable immediately
  discarded with `(void)`.

### The validation immediately earned its keep

Adding it turned a second test red: `test_ext_three_word_emission` carried the
SAME obsolete flat layout (`[0]` nested claiming a child at `[2]`, the second
top-level field at `[1]`). It had always built, because a depth-1 EXT never
reaches the span-stepping walk — precisely the "leaf-only tables never exposed
this" case `kind_span`'s comment predicted. Rewritten in preorder.

Two of the three hand-built tables in this file were malformed, and one had
been latent since long before #267.

### Receipts

- `just cyclonedds-ci` → rc=0, 16/16, with all six cases in this binary
  reporting OK — including `nested_msize_covers_large_member (m_size=48 >= 48)`,
  so #267's property still holds.
- New `test_non_preorder_table_rejected` asserts the SPECIFIC code
  (`err == -1005`), not merely that the build failed: the pre-0319 behaviour
  also failed, but as -1002, which sent the reader looking at field kinds.
- **Mutation-checked.** With the `validate_kind_table` call disabled, that test
  FAILS; restored, 16/16. So the gate detects the thing it claims to.

## Follow-up: the lane stops being special (2026-07-28)

The remaining half of "why it survived". `cyclonedds-ci` was a named step on
the top-level `ci` line — the only RMW with one — and `just check` never ran
it, so a red sat on main for two days.

That slot is gone. The suite is now `check-rmw-cyclonedds`, a private lane in
`check-build` beside `check-c`, `check-cpp` and `check-cli-tests`, which is
where one backend's native test suite belongs. `just ci` is back to
`check rust-rtos-link-check test-all`.

Placement was chosen by measurement, not preference: a warm run is **~22s**,
which the `check` tier can absorb, and `check` is the recipe people actually
run. The best-effort skip when the Cyclone submodule is uninitialised is
carried over unchanged, so contributors who do not touch the DDS backend are
unaffected.

Receipt: `just check` → rc=0 with `100% tests passed, 0 tests failed out of 16`
in its output, i.e. the suite really runs there now.

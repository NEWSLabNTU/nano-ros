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

Worth noting for whoever revisits: the two conventions still coexist in
`dynamic_type_builder.cpp`. `emit_nested_body` reading `k.inner` while
`kind_span` assumes `idx + 1` is a latent trap — it is safe only because every
producer happens to satisfy both. Making `kind_span` consult `inner`, or
dropping `inner` in favour of the positional rule, would remove the ambiguity;
neither is done here because this issue is about the red test, not a redesign.

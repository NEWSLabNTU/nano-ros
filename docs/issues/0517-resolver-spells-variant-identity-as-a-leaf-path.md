---
id: 517
title: "The fixture resolver spells a row's variant identity as a leaf path literal, so the manifest's `target_dir` column cannot be deleted without silently mis-resolving 17 roots"
status: open
type: tech-debt
area: testing
related: [phase-340, issue-0482, rfc-0070]
---

## Symptom

There is no symptom today. This is a blocked deletion, filed because the block
is not visible from either side alone and because acting on the obvious reading
would produce a FALSE GREEN rather than a failure.

phase-340 W2.d wants the manifest's `target_dir` / `build_subdir` column gone:
RFC-0070 R2 says a coordinate is "platform, lang, rmw, feature-sig — and nothing
else", and `target_dir = "target-zenoh"` is exactly the ad-hoc suffix R2 calls a
bug. The predicate half landed (`cf8d2d18e`) — `--core-only` now derives
"variant row?" from authored configuration instead of reading the column. The
column's remaining consumer looked mechanical. It is not.

## Why the column is still load-bearing

`fixtures::groups::attribute()` maps a leaf artifact path to a build GROUP by
longest match on `row_artifact_root`, which is `<dir>/<target_dir or "target">`.
The test-side resolvers (`fixtures/binaries/mod.rs`) hand-spell the leaf half of
that — `target-tls`, `target-fixtures/nuttx-riscv`, `target-large-buf`,
`rmw.target_dir()` — and the redirect turns it into the shared group dir.

So the authored suffix is not decoration: **it is the only thing that tells the
resolver which VARIANT a hardcoded path means.** Measured over the 124 rows
`fixtures-manifest.py fixture-groups` emits:

| | artifact roots mapping to >1 group slug |
| --- | --- |
| today | **0** (injective) |
| with the column deleted | **17**, up to 5 slugs each |

```
examples/native/rust/talker/target
  -> linux, linux-1147932602, linux-3000917972, linux-3263301353, linux-553222167
```

`attribute()` returns exactly one row (longest match, then first), so an
ambiguous root does not fail — it resolves to a real binary built with DIFFERENT
features, and the test runs against it and passes. That is the failure mode
phase-340's acceptance rule exists for: a build, never a gate, because
build/probe/resolver disagreement is invisible to gates.

## The fix, and its order

The resolver must name a COORDINATE, not a path. This is the same handle issue
0482 named as missing ("the resolver has no link back to the manifest row"); 0482
was resolved on its own symptom (lane build and run disagreeing on the fixture
set) and the handle itself was never built.

Order, which is the reverse of what W2.d assumed:

1. Give the resolver a coordinate-keyed lookup — `(platform, lang, rmw,
   feature-sig)` → artifact dir, derived from the same `nros_fixture_group` call
   the BUILD uses (`nros_fixture_row_artifact_dir` is already that inverse on the
   shell side; the Rust side has no equivalent).
2. Convert the hand-spelled resolver literals to it, one family at a time. Each
   conversion is verifiable on its own: the resolved path must be byte-identical
   before and after.
3. Only then delete the column, with a `lane=all` rebuild as acceptance.

Doing (3) first is the tempting order because it is one mechanical edit over 39
rows, and it is the one that produces the false green.

## Note

`row_artifact_root`'s docstring already anticipates the ambiguity — "a row that
authors no `target_dir` shares `<dir>/target` with every sibling row of the same
dir … the consumer resolves that by preferring the LONGEST match and by treating
an ambiguous match as *not attributable* (fail closed — never skip)". The
fail-closed half is NOT implemented in `attribute()`, which silently prefers the
longest and ties-break by order. Implementing it would convert this issue's
false green into a loud failure, which is worth doing independently of the
deletion.

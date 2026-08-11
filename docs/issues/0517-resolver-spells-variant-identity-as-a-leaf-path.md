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

## Design exploration (2026-08-11)

### The precedent already in the tree

**`[[workspace_fixture]]` rows do not resolve by path — they resolve by ID.**
`lane::attribute_path` skips every row whose `kind != "fixture"`, and
`lane::attribute_workspace_id(fixture_id)` answers for those instead. So this
issue is not a new mechanism: it is extending the half that already works to the
half that does not.

That distinction is also the answer to a scarier-looking number. Measured over
the 341 `coords` rows, **3 artifact roots map to more than one coordinate
today** — `examples/workspaces/{mixed,features,safety}` — and all three are
`workspace_fixture`, where one workspace legitimately builds many entries into
one dir and the rows differ by `lang` (c vs cpp) or `platform` (linux /
threadx-linux / freertos). Path attribution never sees them, so they are not a
live bug. Restricted to `kind == "fixture"`:

| | plain-row artifact roots |
| --- | --- |
| mapping to >1 coordinate | **0** |
| shared by >1 row at all | **0** |

`(dir, target_dir)` is a perfect key today. **The column IS the row id** — which
is precisely why removing it without a replacement key is not a refactor.

### What the call site can supply

The variant cannot come from the authored dir string, because that string is
what a shared group strips. It has to come from the row. But the call site does
know the variant in structured form already:

* `build_example_rmw(name, bin, Rmw::Xrce)` — carries the rmw.
* `build_example(name, bin, features, target)` — **already takes `features`, and
  the parameter is `_features`, ignored.** The information was there the whole
  time; the literal `target-tls` is a second, weaker spelling of it.

Blast radius, measured: all **40** call sites of the two funnels are inside
`fixtures/binaries/mod.rs`. The public per-fixture wrappers the tests call do not
change. The leaf-path literals to convert are ~12: `target-tls`,
`target-large-buf`, `target-zero-copy`, `target-fixtures/{nuttx,nuttx-riscv,
threadx-linux}`, `Rmw::target_dir()`'s three arms, and the two esp32/zenoh
formats.

### Shape

* **Phase A — fail closed on ambiguity.** `lane::attribute_path` and
  `groups::attribute` both take the longest match and break ties by iteration
  order. Make a tie return `None` (lane) / an error (groups). Measured above,
  this is a **strict no-op today** — 0 plain-row roots are shared — so it is a
  tripwire, not a change, and it is what turns every later step's mistake into a
  red instead of a wrong binary. Testable with synthetic rows, which
  `lane_run_narrowing.rs` and `fixture_group_resolution.rs` already do (the
  `path_under` docstring notes the real manifest hides the difference — the same
  applies here).
* **Phase B — structured variant at the call site.** Export the row FIELDS
  (`dir, platform, lang, rmw, features, env, artifact_root, slug, shared`) and
  select on them, so no hash is recomputed in Rust (R3: the slug stays the
  shell's single computation). `row_artifact_dir(dir, variant)` fails closed on 0
  or >1 match. Convert the literals one family at a time; per-family acceptance
  is that the resolved path is byte-identical before and after, which is cheap
  and does not need a rebuild.
* **Phase C — delete the column, and DERIVE the isolation it was providing.**
  All 41 `target_dir` rows sit on shared platforms today (linux 29,
  threadx-linux 6, threadx-riscv64 6), so their isolation comes from the group
  and the column is inert for the build. That is a property of the current
  shared list, not an invariant: a variant row on a non-shared platform would
  collide in one `target/`. So the non-shared branch of
  `nros_fixture_row_artifact_dir` / `row_artifact_root` should become
  `<dir>/target-<slug>` for a variant row — a DERIVED suffix, which is what R2
  asks for, and which keeps plain-row roots injective for anything still
  resolving by path. Acceptance: `lane=all`.

Phases A and B are independently landable and neither needs a fixture rebuild.
Only C does.

## Note

`row_artifact_root`'s docstring already anticipates the ambiguity — "a row that
authors no `target_dir` shares `<dir>/target` with every sibling row of the same
dir … the consumer resolves that by preferring the LONGEST match and by treating
an ambiguous match as *not attributable* (fail closed — never skip)". The
fail-closed half is NOT implemented in `attribute()`, which silently prefers the
longest and ties-break by order. Implementing it would convert this issue's
false green into a loud failure, which is worth doing independently of the
deletion.

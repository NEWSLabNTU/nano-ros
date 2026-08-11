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

## Status (2026-08-11)

**Phase A — DONE.** Both inversions fail closed on an ambiguous longest match:
`lane::attribute_path` (extracted as `attribute_path_in` so synthetic rows can
drive it) and `groups::attribute`. A tie at different coordinates / different
slugs returns `None`; a tie at the SAME coordinate does not, and a longer match
still wins outright. No behaviour change on today's tree — it is the tripwire
that makes the rest of this issue red instead of wrong. Both tests verified to
fail with the old tie-breaking restored.

**Phase B — DONE. Every resolver names a configuration or an id; none spells a
variant as a path.**

`row_selector(entry)` exports `(dir, rmw, features, no_default_features, env)`,
injective over all 248 rows; `groups::row_artifact_dir(dir, variant)` selects on
it and fails closed on 0 or >1 matches; `FixtureVariant` is a closed set of four
constructors because the manifest holds exactly four shapes (37/144/64/3 rows).
Converted: `build_example_rmw`, the feature-variant resolvers (`target-tls` ×2,
`target-zero-copy`, `target-large-buf`), and the ~8 workspace resolvers via
`workspace_artifact_dir(id)`. `leaf_has_rows()` carves out leaves the manifest
never describes (`px4/rust/companion`, its own lane).

Two defects surfaced during the conversion, both worth keeping:

* **`row_artifact_root` was wrong for every `mixed` workspace row.** 13 rows
  whose builds write to `build_subdir` were told `<dir>/target`, because
  `is_cargo_row` read `lang = "mixed"` as cargo. A mixed workspace is DRIVEN by
  cmake (corrosion invokes cargo underneath). Fixed with `builder = "cmake"`,
  phase-344 W2's precedent. Invisible until now because nothing consumed the
  value: `attribute_path` skips workspace rows and `fixture-groups` carries only
  plain rows. It is also why `examples/workspaces/mixed/target` appeared in this
  issue's original "3 ambiguous roots today" — it was ambiguous because the
  value was wrong, not because rows shared a dir. Two remain (`features`,
  `safety`), and those are the legitimate many-entries-one-build-dir case.
* **My own first `row_artifact_dir` returned the GROUP dir.** That breaks lane
  narrowing: `require_prebuilt_binary` attributes on the leaf root FIRST and
  redirects SECOND, so a pre-redirected path matches no row and every out-of-lane
  fixture on a migrated platform hard-fails as missing instead of skipping —
  narrowed runs only, invisible to `check-fast`. Now it always returns the leaf
  root; `a_selected_root_stays_lane_attributable` asserts it.

**Phase C — C1 done; C2 is smaller than this issue first assumed.**

C1: six `build_subdir = "build-cyclonedds"` keys restated the default and are
gone. `coords`, `fixture-groups` and `list` are byte-identical before and after.

C2 is no longer "delete the column". After phase B the column is not an identity
handle for anything — it is the leaf-side artifact ROOT, and what R2 objects to
is how that root is SPELLED (authored, ad hoc) rather than that it exists. So
the work is to DERIVE the spelling from the selector:

    rmw + [rmw-<x>] + ndf   ->  target-<rmw>          37 rows, IDENTICAL today
    [] + features           ->  target-<features>      3 rows, moves
    [] + [] + env           ->  target-<env-token>     1 row,  moves

Measured: 37 of 41 rows keep their exact current path, and **4 move** —
`target-tls` ×2 (would become `target-link-tls`), `target-zero-copy`
(`target-unstable-zenoh-api`), `target-large-buf`. So the acceptance is not
`lane=all`: it is a rebuild of those four leaves plus the native lane, which is
affordable. Nothing else in the tree names those four paths any more — phase B
removed the last resolver literals — so the change is the derivation plus a
`fixtures.toml` edit.

Worth weighing before doing it: the benefit is that a row can no longer author a
suffix nobody derives, which is R2's rule. The cost is that four paths stop being
readable (`target-link-tls` is fine; an env-hashed one is not). A middle option
is to derive only where the derivation is readable and keep a gate that rejects
an AUTHORED `target_dir` that disagrees with what the selector would derive —
which gets R2's invariant without the unreadable spelling.

Verification method used throughout, and worth keeping: each conversion showed
the row's artifact root byte-identical to the literal it replaced (37/37 RMW,
4/4 feature, 5/5 workspace ids, 59/59 workspace callers agreeing), plus a
per-row equivalence test. That is why phases A and B needed no fixture rebuild
at all.

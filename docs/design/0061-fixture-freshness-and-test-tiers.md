# RFC-0061 — Fixture freshness by tool OUTPUT, and a tiered test ladder

**Status:** Draft (2026-07-28)
**Supersedes nothing. Amends:** the fixture-freshness contract established by
issue #182 (tool in the signature) and phase-300 W2.1 (git-index enumeration).
**Motivated by:** a `just ci` run on 2026-07-28 that passed every code stage and
was then blocked by 40 "stale" workspace fixtures, none of which were
semantically stale; plus issue 0337 (remote `pr-checks` red for 60+ runs while
everyone validated locally with `just check`).

## Summary

Two changes, independent but mutually reinforcing:

1. **Signature on toolchain OUTPUT, not tool BINARY.** The workspace-fixture
   signature currently hashes `sha256(packages/cli/target/release/nros)`. Every
   rebuild of the CLI therefore invalidates every workspace fixture, including
   rebuilds whose codegen output is byte-identical. Replace the binary hash with
   a **toolchain fingerprint** — a hash of what the tools emit, covering BOTH
   `nros` and `nros-launch-resolve` — cached per binary so it costs one probe run
   per rebuild, not one per fixture.

2. **A tier ladder with scoped gates.** `just ci` is documented and used as the
   everyday lane but is defined as `check rust-rtos-link-check test-all`, where
   `test-all` requires **every platform's** fixtures to be fresh. A native-intent
   run is blocked by a stale ThreadX fixture. Define tiers 0–3 explicitly, and
   make each tier's freshness gate cover exactly that tier's fixtures.

## Problem

### What is already right (and must not be undone)

The freshness machinery is content-based, not mtime-based — a fact worth stating
because it is easy to assume otherwise and "fix" it backwards:

- `scripts/build/workspace-fixture-signature.sh` hashes the manifest record, the
  CLI, and the workspace sources **enumerated through the git index** (`git
  ls-files --cached --others --exclude-standard`), reading file *contents*.
  Phase-300 W2.1 moved to the git index precisely because a `find`-based walk
  hashed cmake byproducts and produced FALSE staleness.
- `scripts/test/rust-fixture-stale.sh` delegates to cargo's own fingerprint
  (`cargo build --message-format=json`, looking for `"fresh":false`).
- `scripts/test/cmake-fixture-stale.sh` runs the incremental build and decides
  from whether real compile/link work happened.

So three mechanisms, two of which (rust, cmake) delegate to a build system's
fingerprint and **self-heal** as a side effect, and one (workspace) which is a
declarative signature that can only *report*.

### The defect

The workspace signature includes:

```sh
nros_bin="$repo_root/packages/cli/target/release/nros"
printf 'tool:nros\0'; sha256sum "$nros_bin" | awk '{printf "%s", $1}'
```

Issue #182 added this for a real reason: a fixture built by a pre-`fd32a0f75`
emitter verified as fresh, and realtime tier lanes ran museum TUs with
correct-looking sources. Hashing the tool is right. Hashing the **binary** is an
over-approximation of the property that matters, which is *"would this tool emit
different bytes for this input?"*

Rust binaries are not reproducible across rebuilds in the way this assumes:
incremental compilation, build IDs, and path/metadata differences change the
hash without changing behaviour. In practice **any** `just setup-cli` — required
after every rebase that touches `packages/cli`, and after any CLI change at all —
invalidates every workspace fixture in the tree.

Measured on 2026-07-28: a change confined to `packages/cli/rosidl-codegen` that
only *added a rejection path* for a configuration no in-tree fixture uses
invalidated **40 workspace fixtures across 35 build directories**. Rebuilding
them is a multi-hour, ~100 GB operation. The correct answer was zero.

### The second-order cost

Because the sweep is expensive and frequently spurious, it gets skipped, and
skipping is how the real reds hide:

- Issue 0337 — remote `pr-checks` red for 60+ consecutive runs, unnoticed because
  everyone validates locally with `just check`, which is a different lane.
- Issue 0328 — 24 `#[ignore]` tests no lane runs; one had been failing since
  phase-303 W4 added the XCDR2 DHEADER seam, discovered only when this session
  ran it by hand.
- The `NROS_SKIP_FIXTURE_CHECK=1` escape hatch exists and is documented in the
  gate's own failure message, which is an honest admission that the gate cries
  wolf often enough to need one.

## Proposal 1 — fingerprint the output

Replace `tool:nros = sha256(binary)` with `toolchain = fingerprint`, defined as
**the hash of what the toolchain emits over a fixed probe corpus**:

```
codegen_fp(nros)   = sha256( for input in msg_srv_action_corpus: emit(nros, input) )
resolve_fp(nlr)    = sha256( for tree  in launch_corpus:         emit(nlr,  tree)  )

toolchain_fp       = sha256(codegen_fp || resolve_fp)   # resolve_fp only where used
```

### Both tools, because both emit fixture inputs (decided 2026-07-28)

`nros ws sync` shells out to `nros-launch-resolve` by absolute path (RFC-0060),
and the SystemModel that comes back **is** a fixture input — it is committed,
consumed by `nros::main!(model = …)`, and its contents change what gets built.
A fingerprint blind to the resolver would repeat, one layer down, exactly the
`#182` bug it exists to prevent: a fixture built by a museum resolver verifying
as fresh.

Two failures on 2026-07-28 make this concrete:

- the rebuilt `nros` passed `--bringup-root` and the installed resolver rejected
  it (`unexpected argument '--bringup-root'`) — a skew the signature could not
  see, because the resolver is not in it;
- upstream's `--bringup-root` fix changed emitted models from absolute to
  **repo-relative** paths (issue 0320) — an output change, in fixture inputs,
  from a tool the signature does not hash.

**Scope it to records that use it.** The manifest record already carries a
bringup field, so `resolve_fp` enters the signature only for fixtures whose build
actually runs `ws sync`. A resolver rebuild then invalidates workspace fixtures
with a bringup and nothing else — correct, and much narrower than hashing it into
everything.

**CPython caveat.** The resolver embeds CPython, so computing `resolve_fp`
requires a working Python environment. Where it cannot be computed, fall back to
`sha256(resolver binary)` — degrading to today's over-approximation, never to
"assume fresh". Fail-safe beats the optimisation.

Properties that make this the right key:

- **Exact, not approximate.** It changes if and only if emitted bytes change. A
  refactor, a comment, a new rejection path for an unused config, a rustc bump —
  none of these move it. A template edit does.
- **Empirical, not declarative.** A hand-maintained `CODEGEN_VERSION` constant
  would drift the first time someone forgets to bump it — the same class as the
  `is_phase1_supported()` predicate that issue 0343 found had gone false while
  nobody called it. Measuring the output cannot go stale.
- **Cheap, via caching.** Key each fingerprint on its binary's hash:

  ```
  .nros-cache/codegen-fingerprint/<sha256-of-nros-binary>     -> <codegen_fp>
  .nros-cache/resolve-fingerprint/<sha256-of-resolver-binary> -> <resolve_fp>
  ```

  One probe run per new binary; every subsequent signature computation is a file
  read. The corpora are small — one msg with each configurable shape, one srv, one
  action (the corpus this session used ad hoc for golden diffing), and one launch
  tree with a bringup for the resolver.

### The probe corpus doubles as a codegen golden test

The same corpus, with its expected output committed, gives a **tier-1 codegen
regression test costing seconds** — no fixture, no toolchain, no QEMU. Used ad
hoc during issues 0344–0346 it caught two real regressions that would otherwise
have surfaced as a fixture build failure much later:

- a macro-extraction bug that swapped serialize and deserialize bodies;
- a trailing-newline change that would have rewritten the last byte of every
  generated file in the tree.

Committing the corpus makes both permanent, and makes the fingerprint's inputs
reviewable in a diff.

### Migration

1. Add `nros codegen-fingerprint` (or a `scripts/build/codegen-fingerprint.sh`
   wrapper if the CLI should not learn about the corpus).
2. Signature version bump: `nros-workspace-fixture-v2` → `v3`. The bump
   invalidates once, deliberately, and never again for the same reason.
3. Keep the binary hash as a *fallback* when the fingerprint cannot be computed
   (no cargo, cross-only host), so the gate degrades to today's behaviour rather
   than to "assume fresh". Failing safe matters more than the optimisation.

### What this does NOT change

Rust and cmake fixtures keep delegating to cargo/ninja fingerprints. They are
already exact and self-healing; the tool identity reaches them through the
generated sources those builds consume.

## Proposal 2 — tiers, and gates scoped to them

### The scope mismatch

```
ci: check rust-rtos-link-check test-all
test-all …: _require-fixtures _check-fixtures-stale build-zenohd
```

`_check-fixtures-stale` covers every platform. So `just ci` — the recipe
`CLAUDE.md` tells every agent to run after a task — is a **full-matrix** lane
wearing an everyday name. On 2026-07-28 it refused to run native tests because
ThreadX and FreeRTOS workspace fixtures were stale.

### The ladder

| tier | name | scope | gate covers | budget | when |
| --- | --- | --- | --- | --- | --- |
| 0 | `check-fast` | fmt, drift gates, source gates, no build | nothing (no fixtures) | ~30 s | every save |
| 1 | `ci` | tier 0 + workspace/host build + **codegen golden diff** + native tests | native fixtures only | minutes | every commit, pre-push |
| 2 | `ci-matrix` | tier 1 + a **pairwise cover** of the declared matrix | that subset's fixtures | ~20 % of a full sweep | diff touches `packages/core`, codegen, `cmake/` |
| 3 | `ci-full` | the whole matrix + Miri + interop + QEMU lanes | everything | hours | nightly, pre-release, on demand |

Two rules make the ladder honest:

- **A tier's gate covers exactly that tier's fixtures.** Tier 1 must not consult
  ThreadX. This is a scoping change to `_check-fixtures-stale` (it already takes
  manifest records; it needs a platform filter), not new machinery.
- **`just ci` means tier 1.** Rename today's meaning to `ci-full` and update
  `CLAUDE.md`'s "always `just ci` after a task" to point at tier 1, with tier 2
  named for core-touching changes. The current instruction asks for something
  nobody can afford to do per task, and an instruction that cannot be followed is
  followed selectively.

### Tier 2 is the missing rung, and it is pairwise (decided 2026-07-28)

The repo already declares the matrix (`packages/testing/nros-tests/src/matrix.rs`),
so tier 2 must be **computed from it**, never a hand-picked list that rots.

The axes are not independent — an RMW is not available on every platform — so
"pairwise" here means a **set cover over the DECLARED cells**, not a covering
array over the cartesian product:

> choose a minimum subset of declared Runtime cells such that every
> (axis_i = a, axis_j = b) pair occurring in ANY declared cell occurs in the
> chosen subset.

Greedy set cover is adequate (within a `ln n` factor, and the input is tiny).

**Measured on the matrix as it stands today** — 182 Runtime cells:

| pairing axes | pairs to cover | cover size | share of sweep |
| --- | --- | --- | --- |
| platform × lang × rmw | 55 | **31 cells** | 17 % |
| platform × lang × rmw × kind | 96 | **37 cells** | 20 % |
| platform × lang × rmw × workload × kind | 227 | 73 cells | 40 % |

**Tier 2 pairs over platform × lang × rmw × kind — 37 cells, ~20 %.** Full 5-axis
pairwise is 40 % of the sweep, which is not a middle tier; and the evidence says
`workload` is the axis that does not need pairing. Every interaction defect this
session found lives in the first three axes:

- sizes-header mirror (0268) and the `#245` recurrence — platform × language;
- freestanding-header gaps (0332) — platform × language;
- vtable ABI / transport-ops SSoT (0331) — RMW × language;
- storage-mode codegen (0343–0346) — language × entity kind.

None was workload-specific: a `Pubsub` cell and an `Action` cell on the same
(platform, lang, rmw) fail together. Keeping `kind` in the pairing costs 6 cells
over the three-axis version and buys the Example-vs-Workspace distinction, which
is where the entry/carrier wiring bugs live (0097, 0263).

The cover must be **recomputed, not stored**: adding a platform to `matrix::CELLS`
must add it to tier 2 without editing a second list. Cache the computed cover
keyed on a hash of the cell table so the recompute is free when nothing moved.

### Change-impact routing

Two signals are already computable and should select the tier automatically:

- **Executor storage size.** If `NROS_EXECUTOR_STORAGE_SIZE` changes, every C/C++
  fixture must rebuild — that is the entire mechanism of issue 0268 and issue
  0245. If it does not change, none must. The value is emitted by the build; diff
  it against the previous build's header.
- **Codegen fingerprint** (proposal 1). Changed ⇒ regenerate consumers. Identical
  ⇒ skip, no matter how much the CLI source moved.

## Operational corollaries

These do not depend on proposals 1–2 but bound the same cost:

- **Disk.** A full sweep needs ~800 GB and hit **11 MB free** twice on
  2026-07-28. Tier 3 must build → test → **drop that family's artifacts** →
  next, rather than accumulating. The per-family artifacts are reproducible; the
  test result is what needs keeping.
- **QEMU concurrency.** Documented flake under load (287-W7: six NuttX lanes
  failed in-sweep, passed solo). Tier 3 should cap concurrency for QEMU-bearing
  lanes. A tier whose reds are routinely noise trains people to ignore reds.
- **Ignored tests.** Issue 0328's 24 `#[ignore]` tests either get a tier (2 or 3)
  or get deleted. A test with no lane rots invisibly and then blocks the day
  someone enables it — which is why nobody enables it.
- **Local ≡ remote.** Issue 0337's 60 red runs came from `pr-checks` running
  something no developer runs. Remote lanes should invoke the same named tier
  recipes, so "tier 1 is green locally" is a claim about the same thing CI checks.

## Acceptance

- A `just setup-cli` that does not change emitted bytes invalidates **zero**
  workspace fixtures. Demonstrated by: capture signatures, rebuild the CLI
  (touch a comment in `nros-cli-core`), re-capture, diff — empty.
- A template edit that changes emitted bytes invalidates the affected fixtures.
  Demonstrated by the inverse of the above.
- `just ci` (tier 1) runs to completion with a stale ThreadX fixture on disk.
- The codegen golden corpus is committed, and a deliberate template change fails
  tier 1 with a readable diff.
- Tier 2's cell list is computed from `matrix::CELLS` as a pairwise set cover
  over platform × lang × rmw × kind, and adding a platform to the matrix adds it
  to tier 2 without editing a second list. Regression check: the cover must
  contain at least one cell for every declared value of each of those four axes.
- A resolver rebuild that changes emitted SystemModels invalidates the workspace
  fixtures **with a bringup** and no others; a resolver rebuild that does not
  change emitted models invalidates nothing.
- With no Python available, `resolve_fp` falls back to the resolver's binary hash
  and the gate still refuses to call a stale fixture fresh.

## Non-goals

- Making Rust binaries reproducible. The fingerprint sidesteps the question.
- Replacing cargo/ninja fingerprinting for rust/cmake fixtures. They are correct.
- Removing `NROS_SKIP_FIXTURE_CHECK=1`. It stays as an escape hatch; the aim is
  that it stops being routine.

## Decisions

**Decided 2026-07-28 (maintainer):**

1. **The fingerprint covers the resolver as well as the codegen tool.** Scoped to
   records that declare a bringup, with a binary-hash fallback where CPython is
   unavailable. Rationale and the two 2026-07-28 failures it would have caught are
   in Proposal 1.
2. **Tier 2 is pairwise, over platform × lang × rmw × kind** — 37 cells, ~20 % of
   the runtime sweep on today's matrix. Full 5-axis pairwise (adding `workload`)
   was measured at 73 cells / 40 % and rejected as too heavy for a middle tier;
   no interaction defect found this session was workload-specific. Numbers and
   method in Proposal 2.

## Open questions

1. **Where does the probe corpus live?** `packages/cli/rosidl-codegen/tests/fixtures/`
   is the natural home for the codegen half, but the fingerprint is consumed by
   shell scripts in `scripts/build/`, and the resolver half needs a launch tree
   (a natural fit for `packages/cli/testing_workspaces/`). A committed corpus plus
   a thin CLI verb per tool is probably right; worth settling before implementation.
2. **Does tier 2's cover need stability across runs?** Greedy set cover is
   deterministic for a fixed cell order, but adding one cell can reshuffle the
   chosen set, which makes "why did this lane change?" harder to answer. A
   lexicographic tie-break plus committing the computed cover as a reviewable
   artifact (recomputed and diffed, not hand-edited) would fix it — at the cost of
   a file that must not be edited by hand.

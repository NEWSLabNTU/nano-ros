# RFC-0061 — Fixture freshness by tool OUTPUT, and a tiered test ladder

**Status:** Draft (2026-07-28; amended 2026-08-31 — see the breadth/depth amendment)
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

`nros sync` shells out to `nros-launch-resolve` by absolute path (RFC-0060),
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
actually runs `nros sync`. A resolver rebuild then invalidates workspace fixtures
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
   wrapper if the CLI should not learn about the corpus). **Shipped as the verb:**
   the corpus is `include_str!`-compiled into `rosidl-codegen`, so a wrapper would
   only have re-invoked the binary, and the verb lets the golden test and the
   fingerprint read the same `emit_corpus()` map — which a separate script could
   not. The resolver half (`resolve-fingerprint.sh`) genuinely is a script,
   because it runs a DIFFERENT binary over an on-disk launch tree.
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
| 1 | `ci` | tier 0 + host build + **codegen golden diff** + a 16-cell native selection | native fixtures only | minutes | every commit, pre-push |
| 2 | `ci-matrix` | tier 1 + an **11-cell 1-wise cover** (platform, lang, rmw, kind) | that subset's fixtures | **26 %** of a full sweep | diff touches `packages/core`, codegen, `cmake/` |
| 2n | `ci-matrix-nightly` | the **37-cell pairwise cover** (platform×lang×rmw×kind) | that subset's fixtures | **70 %** of a full sweep | nightly |
| 3 | `ci-full` | the whole matrix + Miri + interop + QEMU lanes | everything | hours | pre-release, on demand |

**Tier 2 split in two (amended 2026-07-30, see §Cost is coordinates).** This RFC
originally put the pairwise cover on `ci-matrix` at "~20 % of a full sweep". That
number counted CELLS, and cells share fixtures: in fixture COORDINATES — the unit
that drives build hours — the pairwise cover is 70 %, not 20 %. A middle tier
costing 70 % of the sweep is one nobody runs, which is the failure mode this RFC
exists to fix, so the affordable 1-wise cover became `ci-matrix` and the pairwise
cover moved to a nightly lane rather than being dropped.

Two rules make the ladder honest:

- **A tier's gate covers exactly that tier's fixtures.** Tier 1 must not consult
  ThreadX. This is a scoping change to `_check-fixtures-stale` (it already takes
  manifest records; it needs a platform filter), not new machinery.
- **`just ci` means tier 1.** Rename today's meaning to `ci-full` and update
  `CLAUDE.md`'s "always `just ci` after a task" to point at tier 1, with tier 2
  named for core-touching changes. The current instruction asks for something
  nobody can afford to do per task, and an instruction that cannot be followed is
  followed selectively.

### Selection strategy — strength per AXIS, not per tier

Uniform t-wise is the wrong frame, because the axes select different KINDS of
thing and therefore fail in different shapes:

| axis | selects | defect shape | strength needed |
| --- | --- | --- | --- |
| `workload` | which core CODE PATH runs (action/service/param/lifecycle) | single-config logic bug | **1-wise** — pairing it is waste |
| `lang` × `rmw` | which ABI SEAM PAIR meets (FFI shim × backend vtable) | needs both sides present | **pairwise** |
| `platform` | toolchain + libc + linker + allocator | only bites combined with a language | **pairwise with lang** (+ rmw for link/DCE) |
| `kind` | entry vs carrier WIRING path | wiring-specific | **pairwise with platform** |

The table is derived from defects, not from theory:

| defect class | axes that must meet | instances |
| --- | --- | --- |
| core logic | none — any single cell | 0322 accept_goal, 0323 param truncation, 0324 spin, 0339 compat shim |
| codegen output | no cell at all | 0343–0346 (a golden diff catches these) |
| sizes / `_opaque` ABI | platform × lang | **0268** (freertos × C), **0245** (zephyr × C++) |
| freestanding headers | platform × lang | 0332 |
| vtable / transport ABI | rmw × lang | 0331 |
| force-link / DCE | platform × lang × rmw | archived 0155 / 0163 |
| entry / carrier wiring | platform × kind | 0097, 0263 |
| transport config mismatch | platform × rmw | archived 0135 |
| threshold / timing / emulator | the whole cell | 0269, 0292, QEMU flake |

Nothing in that catalogue requires `workload` × `platform`: an action-path bug
fails on every platform.

### Per-tier selection (measured against the matrix at 182 Runtime cells)

| tier | selection | cells | why |
| --- | --- | --- | --- |
| 0 | none — text invariants only | **0** | Comparisons of committed text: 0336, 0321, 0268's mirror gate, 0320/0334. No build ⇒ no fixture ⇒ no staleness. |
| 1 | Native only: **1-wise(workload, kind) + pairwise(lang × rmw)** | **16** of 77 native | Every core code path runs once (where most P1s live), and every language meets every RMW — on the host, where a failure costs minutes, not a QEMU boot. |
| 2 | **1-wise(platform, lang, rmw, kind)** | **11** | Every declared value of every axis at least once, at 26 % of the sweep. Gives up interaction coverage to stay affordable per change. |
| 2n | **pairwise(platform × lang × rmw × kind)** | **37** | Exactly the interaction classes above. Deliberately excludes `workload`. Nightly, because it costs 70 % of the sweep. |
| 3 | everything: Runtime + BuildOnly | **182** + build-only | Thresholds, timing, emulator behaviour — irreducible. |

Measured alternatives, for the record:

| selection | cells | share |
| --- | --- | --- |
| 1-wise over all axes | 19 | 10 % |
| pairwise platform × lang | 29 | 16 % |
| pairwise platform × lang × rmw | 31 | 17 % |
| **pairwise platform × lang × rmw × kind** | **37** | **20 %** |
| + 1-wise(workload) | 42 | 23 % |
| full 5-axis pairwise | 73 | 40 % |

### Cost is coordinates, not cells (measured 2026-07-30, phase-318 W4.d)

Everything above counts cells. That is the wrong unit, and getting it wrong is
what put "tier 2 ≈ 20 % of a sweep" in this document.

A cell is a test lane. A **coordinate** is a distinct `(platform, lang, rmw)`
fixture. Many cells share one fixture — the four threadx-linux C cyclonedds cells
are one build — so what a tier costs in HOURS is its coordinate count, not its
cell count. The matrix is 182 runtime cells over **47** coordinates.

| lane | selection | cells | coords | cost |
| --- | --- | --- | --- | --- |
| tier 1 | native, 1-wise(w,k) + pairwise(l × r) | 16 | 10 | 21 % |
| tier 2 | 1-wise(p, l, r, k) | 11 | 12 | 26 % |
| tier 2n | pairwise(p × l × r × k) | 37 | 33 | 70 % |
| tier 3 | everything | 182 | 47 | 100 % |

The pairwise cover reduces cells by 80 % and fixtures by only 30 %. The floor is
structural: pairwise(platform × lang) needs one fixture per declared pair and
there are 29 of them, so no tuning reaches a cheap pairwise tier — the only lever
is whether tier 2 pairs platform × lang at all.

Two further consequences:

- **Filtering the TEST run saves nothing.** Every cover here touches all ten
  platforms by construction (the anti-rot gate requires every declared value of
  every covered axis), so a per-platform nextest filter excludes no platform. The
  saving is entirely in which FIXTURES get built, which is why `lane-coords`
  emits coordinates and `fixtures-manifest.py --coords-from` consumes them.
- **The ladder is not monotone in cells** — tier 1 selects 16, tier 2 selects 11 —
  because tier 1's cells are all native and a native fixture is nearly free. Any
  invariant about tier cost has to be stated in coordinates; the shipped one
  (`the_ladder_is_monotone_in_fixture_cost`) is.

### Two calls that are not obvious

**Tier 2 does not re-cover `workload`.** Adding 1-wise(workload) costs 42 cells
instead of 37 and buys nothing, because **tier 1 already ran every workload on
native**. Workload selects platform-INDEPENDENT core logic; what a platform
changes is build/ABI/link/wiring, which is what the pairwise set targets. Tiers
compose — a tier should not repeat what a cheaper tier already proved.

**Tier 1 takes `kind` at 1-wise, not pairwise.** Example-vs-Workspace changes the
entry/carrier wiring and does break on its own (0097, 0263) — but on native it
breaks the same way for every language. The platform × kind pairing is where the
variation lives, and that is tier 2.

### What each tier is ALLOWED to miss

Stated explicitly, because a tier believed to catch everything gets trusted wrongly:

- **Tier 0** misses anything needing compilation. It is a lint tier.
- **Tier 1** misses every platform-specific defect — the whole 0268/0245/0332
  class. Green tier 1 means "the logic and the seams are sound", never "it builds
  on the targets".
- **Tier 2** misses workload × platform interactions (argued empty above),
  threshold effects (0269 needed 37 pubs / 21 services), and anything timing- or
  emulator-shaped.
- **Tier 3** misses nothing in the matrix — but its reds are noise-prone under
  load (287-W7), so without the concurrency cap it decays into a tier nobody reads.

### Computing the cover

Because the axes are not independent (an RMW is not available on every platform),
"pairwise" means a **set cover over the DECLARED cells**, not a covering array
over the cartesian product:

> choose a minimum subset of declared Runtime cells such that every
> (axis_i = a, axis_j = b) pair occurring in ANY declared cell occurs in the
> chosen subset.

Greedy is adequate (within a `ln n` factor; the input is tiny). The cover must be
**recomputed from `matrix::CELLS`**, never stored as a hand-edited list — adding a
platform must extend tier 2 without touching a second file.

## Amendment (2026-08-31, phase-410) — tiers are BREADTH; DEPTH is a second axis

Proposal 2 defines the tiers as a ladder of COVERAGE: which coordinates a run
visits. That is one axis, and the expensive one is the other.

| axis | values | cost |
| --- | --- | --- |
| **breadth** — which coordinates | tier1, tier2, nightly, full | low |
| **depth** — what we do with each | build+link, build+run | **high** |

Measured 2026-08-31 on one host: `just ci l3` (cross build + link + ELF symbol
interrogation, no QEMU, no tests) is **46 s**. The tier-2 run over the same tree
is **9.5 min** on top of an **11 min** warm fixture rebuild. Depth is roughly
**25x** breadth.

This ladder already contained the second axis without naming it: the LANE
vocabulary (`just ci l1`, `just ci l3`) is depth, while `just ci <tier>` is
breadth-with-depth-fixed-at-build+run. Two ladders sharing the `ci` namespace is
why `ci l3` and `ci full` read as siblings and are not.

**The operational rule that follows:**

> Build+link is MANDATORY and WIDE. Build+run is SCHEDULED and NARROW.

"It compiles and links for every target" is the regression that hurts most and
costs least to catch, so it can afford to gate every merge. Running cannot.

**And a constraint the original ladder could not have known**, because it
predates the merge queue: a run-depth tier on `push(main)` with
`cancel-in-progress: true` STARVES once merges arrive faster than the tier
completes. With ten agents landing through a queue that batches four, a 20-minute
tier is cancelled before it finishes, every time — and a lane that always cancels
looks busy while reporting nothing. Run-depth belongs on a clock.

### The vocabulary (phase-410 W4)

"Lane" named three things here, meaning two:

| name | means |
| --- | --- |
| `CiTier` | the ladder rung |
| `CiLane` | the COMPUTED cell selection for a rung |
| `_NROS_LANES` | the fixture coordinate set — breadth |
| `ci l1` / `ci l3` | DEPTH — compile+unit / cross build+link |

Tier-vs-lane is a real distinction and stands: a rung is a name, a lane is the
selection computed for it. What collided was `l1`/`l3` using "l" for DEPTH.

Resolved as:

```
just ci gate                  compile + unit; visits NO coordinates
just ci <tier>                build + run at that breadth        (default)
just ci <tier> build          build + link only at that breadth
```

`l1` was never a rung — it selects no platform and boots no QEMU, so it has no
breadth at all. It is the GATE. `l3` is a depth on a breadth, and survives as
the implementation of `ci matrix build`.

The argument is POSITIONAL, and that is forced rather than chosen: `just` does
not parse `name=value` for a recipe inside a MODULE — `just ci matrix
depth=build` yields the literal string. Measured 2026-08-31.

phase-410 restructures the workflows accordingly. This RFC keeps the ladder; it
gains the axis the ladder was missing.

## Operational corollaries

These do not depend on proposals 1–2 but bound the same cost:

- **The fingerprint makes invalidation CORRECT, not cheap.** Measured 2026-07-31
  while running tier 1 for acceptance: a pull that advanced `packages/cli` moved
  the codegen fingerprint (`fd00dd67` → `92069f05`), correctly invalidating all
  81 workspace fixtures — those fixtures really were built by an emitter that
  emits different bytes. Tier 1 then could not start until 65 of them were
  rebuilt. So on a machine that has just pulled, tier 1's dominant cost is not the
  run, it is the fixture rebuild the pull earns. W1 removes the SPURIOUS
  invalidations (#182); it cannot remove the real ones, and a tier-1 budget quoted
  as "minutes" is a budget for the second run of the day, not the first.

  The same session showed the mechanism working in the other direction: two
  distinct `nros` binaries (`0b93a2f0`, `e413ab08`) hashed to the SAME fingerprint
  `fd00dd67`, so a rebuild between them invalidates nothing. That pair is the
  clearest evidence the fingerprint does what the binary hash could not.

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

  Where a tier CANNOT be one remote job, the selection still has to be shared.
  `ci-matrix-nightly` is the case: the fixture build scripts need per-platform
  toolchain env that only the `just <module>` recipes export, and the cover spans
  eight modules whose SDKs do not coexist on one runner. So `nightly.yml` computes
  the cover with `lane-coords tier2-nightly`, DERIVES its platform matrix from it,
  and adds a `lane-coverage` job asserting every module in the cover has a job
  somewhere in the tier. Distributing a lane is fine; letting the remote copy of
  the selection drift is not — and a coverage claim with nothing checking it is
  how a platform joins the matrix, joins the lane, and is never built.

## Acceptance

- A `just setup-cli` that does not change emitted bytes invalidates **zero**
  workspace fixtures. Demonstrated by: capture signatures, rebuild the CLI
  (touch a comment in `nros-cli-core`), re-capture, diff — empty.
- A template edit that changes emitted bytes invalidates the affected fixtures.
  Demonstrated by the inverse of the above.
- `just ci` (tier 1) runs to completion with a stale ThreadX fixture on disk.
- The codegen golden corpus is committed, and a deliberate template change fails
  tier 1 with a readable diff.
- Tier 1's cell list is computed from `matrix::CELLS` as
  1-wise(workload, kind) + pairwise(lang × rmw) restricted to `Native`, and tier
  2's as pairwise(platform × lang × rmw × kind) over all Runtime cells. Adding a
  platform to the matrix extends tier 2 without editing a second list. Regression
  check: each cover contains at least one cell for every declared value of every
  axis it pairs or singles.
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
   method in Proposal 2. **Amended 2026-07-30 — see decision 3.**

**Decided 2026-07-30 (maintainer):**

3. **Tier 2 splits: 1-wise on `ci-matrix`, pairwise on `ci-matrix-nightly`.**
   Decision 2's "~20 %" was measured in cells; in fixture coordinates the same
   cover is **70 %** (§Cost is coordinates, not cells). The choice was between a
   70 % middle tier, a 26 % one that gives up interaction coverage, and splitting.
   Splitting was taken: the affordable cover gates every change, the pairwise
   cover — which is where the 0268 / 0245 / 0332 class lives, so it cannot be
   dropped — runs nightly. What tier 2 now trades away is not coverage but
   LATENCY on the interaction classes: a day, instead of pre-merge.

   The nightly lane keeps `rmw` and `kind` in the pairing rather than reducing to
   platform × lang: it costs ~4 coordinates more and a lane off the critical path
   should not trade coverage that cheap.

## Open questions

**Both settled during phase-318 implementation.**

1. ~~**Where does the probe corpus live?**~~ **Settled: with the emitter, behind a
   hidden CLI verb.** The codegen corpus is
   `packages/cli/rosidl-codegen/tests/fixtures/fingerprint-corpus/`, reached from
   shell via `nros codegen-fingerprint`; the resolver's launch tree is
   `packages/cli/nros-launch-resolve/tests/fixtures/fingerprint-launch/`, hashed by
   `scripts/build/resolve-fingerprint.sh`.

   The load-bearing detail is not the location: `tests/codegen_golden.rs` reads the
   SAME `emit_corpus()` map the fingerprint hashes. A golden test covering
   different bytes than the fingerprint could pass while the fingerprint moved (or
   the reverse), and then neither signal is trustworthy.

2. ~~**Does tier 2's cover need stability across runs?**~~ **Settled: the
   lexicographic tie-break, and no committed cover file.** `ci_lane::cells` is
   deterministic for a fixed `matrix::CELLS`, which answers "why did this lane
   change?" for everything except adding a cell — and there the honest answer is
   that the cover legitimately reshuffles.

   Committing the computed cover was rejected: it is a file that must not be
   hand-edited, and this repo's recurring defect is exactly the second copy that
   drifts. What ships instead is the coverage GATE
   (`lanes_touch_every_declared_value_of_every_axis_they_cover`), which asserts the
   property the file was meant to protect without being a second source of truth.
   For CI legibility the resolved cover is published per run as an artifact plus a
   job summary (`nightly.yml`'s `lane` job) — reviewable, and impossible to edit
   into disagreement with the computation.

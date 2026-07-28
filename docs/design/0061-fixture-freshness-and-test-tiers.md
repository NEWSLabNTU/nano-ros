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

1. **Signature on tool OUTPUT, not tool BINARY.** The workspace-fixture
   signature currently hashes `sha256(packages/cli/target/release/nros)`. Every
   rebuild of the CLI therefore invalidates every workspace fixture, including
   rebuilds whose codegen output is byte-identical. Replace the binary hash with
   a **codegen fingerprint** — a hash of what the tool emits — cached per binary
   so it costs one probe run per rebuild, not one per fixture.

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

Replace `tool:nros = sha256(binary)` with `tool:nros = codegen_fingerprint`,
defined as **the hash of what the tool emits over a fixed probe corpus**.

```
fingerprint(nros) = sha256(
    for each input in probe_corpus (sorted):
        emit(nros, input)          # generate-rust / generate-c / codegen entry / ws sync
)
```

Properties that make this the right key:

- **Exact, not approximate.** It changes if and only if emitted bytes change. A
  refactor, a comment, a new rejection path for an unused config, a rustc bump —
  none of these move it. A template edit does.
- **Empirical, not declarative.** A hand-maintained `CODEGEN_VERSION` constant
  would drift the first time someone forgets to bump it — the same class as the
  `is_phase1_supported()` predicate that issue 0343 found had gone false while
  nobody called it. Measuring the output cannot go stale.
- **Cheap, via caching.** Key the fingerprint on the binary's hash:

  ```
  .nros-cache/codegen-fingerprint/<sha256-of-nros-binary> -> <fingerprint>
  ```

  One probe run per new binary; every subsequent signature computation is a file
  read. The probe corpus is small (one msg with each configurable shape, one srv,
  one action — the corpus this session used ad hoc for golden diffing).

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
| 2 | `ci-matrix` | tier 1 + one representative cell **per axis value** | that subset's fixtures | tens of minutes | diff touches `packages/core`, codegen, `cmake/` |
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

### Tier 2 is the missing rung, and it should be derived

The repo already declares the matrix (`packages/testing/nros-tests/src/matrix.rs`).
Tier 2 should be a **minimum covering set computed from it** — every platform at
least once, every RMW at least once, every language at least once — not a
hand-picked list that rots. Roughly 6–8 cells covers the axes.

This is the tier that would have caught most of this session's real breakage:
the sizes-header mirror (issue 0268, C/C++ on any RTOS), the freestanding-header
gaps (0332), the vtable ABI issues (0331).

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
- Tier 2's cell list is computed from `matrix::CELLS`, and adding a platform to
  the matrix adds it to tier 2 without editing a second list.

## Non-goals

- Making Rust binaries reproducible. The fingerprint sidesteps the question.
- Replacing cargo/ninja fingerprinting for rust/cmake fixtures. They are correct.
- Removing `NROS_SKIP_FIXTURE_CHECK=1`. It stays as an escape hatch; the aim is
  that it stops being routine.

## Open questions

1. **Where does the probe corpus live?** `packages/cli/rosidl-codegen/tests/fixtures/`
   is the natural home, but the fingerprint is consumed by shell scripts in
   `scripts/build/`. A committed corpus + a thin CLI verb is probably right;
   worth deciding before implementation.
2. **Does the fingerprint need to cover `nros ws sync` end-to-end**, or only the
   emitters? Sync also shells out to `nros-launch-resolve`, whose version skew
   caused a separate failure on 2026-07-28. Including the resolver's fingerprint
   would catch that class too, at the cost of coupling the two tools' identities.
3. **Tier 2's covering set** — minimum covering (fastest, ~6 cells) versus
   pairwise (catches interaction bugs, ~15–20 cells). Pairwise is the textbook
   answer for a 4-axis matrix; the cost difference is roughly one order of
   magnitude in wall clock.

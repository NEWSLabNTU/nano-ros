---
id: 466
title: "Tier 1 has an unstated, ORDERED setup contract — eight consecutive
  blockers, one of them a test"
status: resolved
type: bug
area: build
related: [issue-0422, issue-0430, issue-0431, issue-0451, rfc-0061, phase-318]
---

## Symptom

`just ci` is the instruction every task ends with (CLAUDE.md, RFC-0061). Over one
session it was attempted repeatedly on a correctly-cloned, ROS-provisioned tree
and stopped **eight** times. Each stop was real, each was invisible until the
previous one cleared, and only ONE was a test failure:

| # | where it stopped | what it actually was |
| --- | --- | --- |
| 1 | `check-examples`, ~3x slower | no GNU `parallel`; the lane DEGRADES to serial |
| 2 | `_require-build-sources` | in-tree `nros` stale after a tree refresh |
| 3 | `nros_box_publish` | read `$CARGO_TARGET_DIR`, which is correctly UNSET |
| 4 | `check-leaf-lockfiles` | two NuttX leaf locks their manifests outgrew |
| 5 | `rust-rtos-link-check` | NuttX static link args, fixed upstream meanwhile |
| 6 | `check-build-profile-literals` | a waiver detached by an inserted comment |
| 7 | `check-no-tracked-file-find` | a checker that walked the tree it scanned |
| 8 | `check` | `nros-board-mps2-an385` did not COMPILE on main |

Nothing here is exotic. The tree was clean, the toolchains installed, the SDKs
provisioned. The cost is not any single item — it is that the list is serial and
undiscoverable: fixing one reveals the next, ~40 minutes later.

## Two distinct problems, and they compound

### A. The setup contract is real, ordered, and written nowhere (1, 2, 3, 5)

There is a sequence a tree must satisfy before `just ci` means anything:

```
nros setup --system          # system packages from [system.*]
cargo build --release --manifest-path packages/cli/Cargo.toml --bin nros
nros_box_publish             # or the host equivalent
just setup-launch-resolve
just build-test-fixtures lane=<the lane you will test>
just ci
```

Every step is documented SOMEWHERE. The sequence is documented nowhere, and the
order is load-bearing: the CLI must be rebuilt before fixtures, because fixtures
key on the CLI's stamp; fixtures must be built for the lane, because
`_require-fixtures` checks coverage per lane (#393).

Worse, the steps re-arm each other. **Any** tree refresh — pull, rebase, stash,
rsync — restages the CLI's source stamp AND every fixture mtime (CLAUDE.md's
"fixture mtime treadmill"). So the contract is not once-per-clone, it is
once-per-refresh, and a session that syncs mid-run silently invalidates work it
already did.

The failure mode is what makes this a bug rather than a docs gap: **most of
these degrade instead of failing.** No `parallel` prints one line and walks 99
example leaves serially — on a 4-core box that reads as a hung tier, not as a
missing package. `just doctor` had been reporting `[MISSING] gnu-parallel` with
the exact apt line the entire time; nothing in the box bootstrap said to run it.
Compare #0451 (embedded SDK env), which is the same shape one layer down: a
required set, surfaced one variable at a time, each looking like a code fault.

### B. Tier 1 is where the source gates live, and it is too expensive to reach (4, 6, 7, 8)

Four of the eight were source-level reds ALREADY ON MAIN — including one crate
that simply did not compile (`mod log_bridge;` committed without its file). They
sat there because the gates that catch them run only in `just ci`, and `just ci`
was unreachable behind problem A.

The consequence is measurable, not hypothetical: **two of the four were fixed
CONCURRENTLY by a different session while this one was fixing them** — the px4
profile-literal waiver and the `git grep` scan, same files, same hour, both
patches landing as duplicates on rebase. That is the signature of a red backlog
that many sessions rediscover independently in different orders.

A ~40-minute serial gauntlet is also a strong incentive to skip the tier, which
is precisely the dynamic RFC-0061 created the ladder to avoid: "the old single
`just ci` WAS tier 3 — an instruction nobody could afford per task, so it got
followed selectively, which is worse than a smaller instruction followed
honestly." Tier 1 is now re-acquiring that property, not through its own runtime
but through everything required before it starts.

## Why the existing pieces do not cover it

- `just doctor` KNOWS about (1). It is not on any path a person walks before a
  tier run, and the box bootstrap never mentions it. Partially addressed: the
  bootstrap and the non-Ubuntu guide now run `nros setup --system` then
  `just doctor`, with the reason inline.
- The stale-CLI guard (#0430) fires correctly for (2) — and #0430 is resolved.
  What is missing is that nothing tells you a tree REFRESH re-arms it.
- `_require-fixtures` (#393) covers lane coverage but not the ordering above it.
- #0451 is the same class for embedded env vars, scoped to direct builds.

## What would fix it

Ordered by how much each buys, not by effort:

1. **A single precondition gate at the head of `ci`** that checks the whole
   contract and prints every unmet item at once with its remedy — the way
   `nros setup --system --check` already does for packages. One failure listing
   four things beats four failures listing one thing each.
2. **Make the degrading probes loud when they matter.** A lane that silently
   drops to serial should say so as a WARNING with the remedy (done for
   `check-examples`); better, `ci` should refuse a degraded lane unless asked.
3. ~~A cheap `just ci --gates-only`~~ — **CORRECTION: this lane already exists,
   and was itself blocked.** `check-fast` is documented as exactly that
   ("BUILDLESS, SOURCE-FREE gates only … needs neither the nros CLI nor any
   provisioned source … ~1 min … the per-push CI gate"). Its FIRST dependency
   was `check-cli-fresh`, so on a tree whose only sin was a `git pull`:

   ```
   just check fast   ->  failed in 0.77s, having checked NOTHING
   ```

   The early placement came from #0363 — front-run `check-dep-chain`, which
   surfaced a stale CLI minutes in as nine cells failing with a cargo resolution
   error. Correct intent, wrong lane: `check-dep-chain` is a check-BUILD gate,
   and no fast-tier gate execs the CLI at all.

   **FIXED (ccd63f474):** moved to the head of `check-build`, which still
   front-runs `check-dep-chain` and gives `check-fast` its contract back. Same
   tree, same stale CLI: 0.77s -> 73s, 19+ gates reporting, including all three
   fast-tier gates that caught this issue's own reds — `check-leaf-lockfiles`,
   `check-build-profile-literals`, `check-staleness-probe-exemptions`.

   So (B) was never a missing-lane problem. The lane existed, ran per-push in
   `check.yml`, and had been disabled for anyone whose CLI was out of date —
   which is anyone who pulled.

   Two things this did NOT fix, both left deliberately:
   - `check-fast` still is not purely buildless: `check-test-targets` compiles
     the test targets, added on purpose after two incidents where a struct field
     broke every test initializer while `check-fast` stayed green. Re-scoping
     what the per-push lane covers is a coverage decision, not a cleanup.
   - On an Arch host that compiling gate now fails for an unrelated reason:
     `pyo3-ffi 0.24.2` caps at Python 3.13 and Arch ships 3.14, so anything
     building `packages/cli` dies with "the configured Python interpreter
     version (3.14) is newer than PyO3's maximum supported version (3.13)".
     Ubuntu 22.04 (Python 3.10) is unaffected, which is why every box run got
     past it. Probably its own issue.
4. Fold "any refresh re-arms the CLI stamp and every fixture" into the pitfall
   index next to the mtime treadmill, which currently names fixtures only.

## Evidence

Session of 2026-08-06/07, in the ROS distrobox mirror and reproduced on the host
where noted. Fixes that came out of it, all landed:

```
f9bd64890  the log_bridge module ab40ab25e declared but never staged
b573b3fe2  nros_box_publish looked for the CLI under an unset variable
32d11ded5  regenerate the two NuttX FFI leaf locks
d602d959c  box-sync writes the box-owned marker BEFORE the transfer
8abd20cf1  logging_smoke lane token (#422)
```

Plus two duplicate-fix collisions described in (B), and the provisioning/doc
changes in `ros2-distrobox-setup.sh` + `docs/development/ros2-on-non-ubuntu.md`.

The one genuine test failure across all eight attempts is #0422's triage index,
which is being worked separately — 10 real failures, independently reproduced on
a second tree.

## Outcome — the thesis reproduced while being fixed (2026-08-07)

Per-push CI was RED on **20+ consecutive runs**, back past 2026-08-06. Fixing it
took three commits, and the shape of that sequence is the argument this issue
makes, observed rather than reasoned:

| # | red | why nobody saw it |
| --- | --- | --- |
| 1 | `check-leaf-lockfiles` demanded a synced tree | first failure in the lane; everything after it never ran |
| 2 | `check-test-targets` needs `-sys` sources | only became visible once (1) was fixed |
| 3 | `scaffold-journey` asked `nros new` for a platform it refuses | a different JOB; unread while the check job was red |

Each was invisible until its predecessor cleared. (3) had been failing since
2026-07-28 — ten days — because `fix(#333)` narrowed `nros new` to platforms
with runnable Rust templates and did not move the one CI job pinned to a
now-refused one.

**A permanent red does not fail, it hides its neighbours.** That is the whole of
(B) in one sentence, and it is why "how long has main been broken" is the wrong
question: main was broken in four places, and the lane could only ever report
the first.

Fixed:

```
b931fc4d3  check-leaf-lockfiles: not-synced is the ENVIRONMENT, warn instead of exit 1
398237d4a  check-test-targets moved to check-build — a compile gate cannot live
           in a source-free lane
29153eab3  scaffold-journey: use a platform the CLI still scaffolds (baremetal)
```

Result — first green push run, and green on the three pushes after it, including
other sessions' commits:

```
success  changes
success  check (fast on push; full on PR/nightly)
success  nros new -> sync -> resolve
success  colcon build examples/templates/local-msg-package/src/
```

### What this changes about the proposals above

Proposal (1) — one precondition gate reporting every unmet item at once — is
worth MORE than first argued, and for a reason the fixing exposed. `just` stops
at the first failed dependency, so a lane does not report "these four things are
wrong"; it reports one, and re-reports one after each fix. Twenty-five gates sat
behind `check-test-targets` alone. Batching the verdict is not ergonomics, it is
the difference between four sequential 40-minute discoveries and one listing.

Also worth recording, since it cost time twice here: verify a fast-tier gate
against a PRISTINE checkout, not a provisioned one. `check-fast` carried a
docstring promising buildless and source-free while two of its gates needed
neither property to hold — and it passed locally the whole time, for the wrong
reason. A `git worktree add --detach` into a scratch dir reproduces CI's
condition in seconds and is now named in the `check-fast` comment.

## Reproduced again 2026-08-11 — five blockers, and the batched gate caught ONE

The precondition gate this issue asked for (fix 1) now exists and works:
`just check tier-preconditions` at the head of `ci` reported the stale CLI with
its ordered remedy, in one shot. It is a real improvement and it is not enough.
A tier-1 run on a long-lived provisioned tree still stopped **five** times, and
four of the five were invisible until the previous cleared:

| # | where it stopped | what it actually was | pre-checkable? |
| --- | --- | --- | --- |
| 1 | `check-tier-preconditions` | in-tree CLI stale after a rebase | **yes — caught** |
| 2 | `check-workspace-build-output` | a Jul-14 `target/` inside `examples/workspaces/mixed/src/rust_heartbeat_pkg` | yes, not checked |
| 3 | `check-artifact-identity-budget` | 1.9 GB of Aug-7 rlibs under `mixed/build-workspace-fixtures` | yes, not checked |
| 4 | workspace fixture LINK | corrosion **0.5.1** still provisioned, so #0493's landed 0.6.1 fix was inert here | yes, not checked |
| 5 | `test-all`, 101 failures | native fixtures never built — every failure was `Test fixture binary not prebuilt` | yes, not checked |

Nothing exotic again. Clean tree, SDKs provisioned, ROS sourced. The pattern this
issue named is intact: the contract is ordered, and each item is discoverable up
front but discovered last.

### Three findings worth acting on

**(a) The gate checks the CONTRACT, not the TREE'S HISTORY.** Items 2 and 3 are
residue a long-lived checkout accumulates; item 4 is a provisioned tool whose
version drifted behind its pin. All three are cheap to detect before a run — the
gates for 2 and 3 already exist and could simply run inside the precondition
batch, and 4 wants a `nros setup --tool <t> --check` that compares installed
against pinned. A tree that has been building for a month is the normal case, not
the exception.

**(b) A landed fix is not an applied fix.** #0493 was verified end-to-end on
2026-08-10 by bumping corrosion 0.5.1 → 0.6.1; on this tree the duplicate-symbol
link failure reproduced in full on 2026-08-11 because only 0.5.1-nros1 was ever
provisioned here. Its own remedy section says so ("provisioning alone is not
enough… the stale build dirs carry the old topology"), which is precisely the
knowledge a precondition check should carry instead of a prose paragraph.

**(c) NEW DEFECT, FIXED — the staleness probe reported an unbuildable fixture as
fresh.** In the run that produced the 101 failures,
`check-fixtures-stale: scope=native` **PASSED** while
`build/cargo-fixtures/linux/nros-relwithdebinfo/talker` did not exist; the tests
then failed one-by-one with `Test fixture binary not prebuilt: … Run
just build-test-fixtures first`.

The cause was in `scripts/test/rust-fixture-stale.sh`, which decided freshness
like this:

```sh
if ( cd "$dir"; … cargo build … --message-format=json --quiet 2>/dev/null ) \
        | grep -q '"fresh":false'; then
```

Only grep's status survives: cargo's exit code was discarded and its errors went
to `/dev/null`. A fixture that **could not compile** therefore printed no
`"fresh":false` line and was reported FRESH — the gate cleared having verified
nothing, no artifact was produced, and the consequence arrived ~100 tests later.
A gate that passes because its own probe broke launders the lane green, which is
the same failure the SCOPE handling in `check-fixtures-stale.sh` guards against
one layer up.

Fixed by capturing the status and reporting three outcomes instead of two: fresh
(silence), stale-and-rebuilt (WARNING — cargo self-healed it, the artifact now
exists), and could-not-build (`FAILED` record, escalated to an ERROR that exits 1
carrying cargo's own first error line and the remedy). Verified by mutation: with
a deliberate compile error in `examples/native/rust/talker`, the probe emits
`FAILED … error: could not compile` and the gate exits 1 naming 4 rows, where
before it printed nothing and exited 0; on a clean tree it still exits 0 with the
self-heal warnings unchanged.

Two claims in the first version of this entry were WRONG and are corrected here,
since both would have sent the next reader the wrong way:

- It said the gate was "stamp-based where the tests are existence-based". It is
  not — the probe genuinely builds, so it heals absence as well as staleness. The
  hole was the swallowed build failure, nothing to do with the lane stamp.
- It said the `native/rust/talker` row was outside the probe set. That came from
  querying the manifest with an invented `--scope native` flag; the gate uses
  `--platform linux` (SCOPE names the lane, `--platform` takes the fixture token —
  different vocabularies, as `check-fixtures-stale.sh` documents). With the
  correct call the native rust probe set is 65 rows and includes 5 talker rows.

### Two others, already-known classes rather than new ones

- `check-workspace-fmt` failed on `packages/api/nros/src/time.rs`, which reached
  main un-nightly-formatted in `6f9881aec` — this issue's item 8 ("main is red")
  recurring. Another session pushed the identical fix concurrently.
- Satisfying item 3 by deleting the tree the gate named then broke
  `_check-fixtures-stale`, whose `.inputsig` stamps live in **that same
  directory**. Two gates with opposing demands on one path: clearing one costs a
  rebuild of the other's inputs. Worth a note in whichever gate is cheaper to
  teach about the other.

Endpoint, for calibration: after all five, `just ci` runs to completion. The only
failures are five of ~1359 tests, and the four retested solo all pass — the
documented sweep-under-load flake, with `large_msg::test_xrce_e2e_integrity`
being #0470 exactly as filed.

## The compile-check lane: the gate is NARROWER than the tests (2026-08-11)

The other four failures in that run — `multi_tier_*` / `single_tier_*`, on
`build/compile-check/orch_tiers_multi/target/debug/demo_entry` — are a second
gate/test disagreement, and it points the OPPOSITE way from the one above.

They cleared by rebuilding (`bash scripts/build/compile-check-fixtures.sh`, after
`just setup-cli` — the stale CLI made every row fail first), and 6/6 pass. But the
interesting part is why `check-fixtures-stale` stayed silent while they were stale:

* the **gate** (`scripts/test/compile-check-stale.sh`) compares a CONTENT
  signature — `.inputsig` against a freshly computed
  `compile-check-signature.sh` — which is the better question in principle
  ("built from the sources on disk right now", immune to mtime churn);
* the **tests** (`require_compile_check_bin` -> `require_prebuilt_binary_fresh`)
  compare cargo dep-info MTIMES.

A `git pull` bumps mtimes without changing content, so the gate passes and the
tests fail. That looks like the tests being wrong, and the tempting fix is to
align them onto the signature. **That fix would be wrong**, and the reason is
worth writing down before someone tries it:

`compile-check-signature.sh` hashes the manifest record, the nros CLI's codegen
fingerprint, and the row's own `$dir` — and nothing else. It does NOT hash the
workspace crates the row compiles against. The input the failing test named was
`packages/boards/nros-board-common/src/platform_config.rs`: a repo crate outside
every `sig_paths` entry. So a REAL edit to a dependency crate leaves the
signature unchanged and the gate silent, while the mtime check catches it.

So the mtime check is currently the only thing covering dependency-crate
staleness for this lane, and the gate is blind to it — CLAUDE.md's issue-0196
rule ("build-side stale probes must watch the same inputs as test-side gates")
violated in the direction that produces museum binaries rather than noise.

The fix belongs in the SIGNATURE, not the test: widen it to the row's in-repo
path-dependency closure (cargo metadata per row, restricted to path deps under
the repo) so that the gate sees what the compile actually consumes. Then the
tests could be aligned onto it, and the pull-induced false STALE goes away as a
side effect rather than as the goal.

Not attempted here: widening it needs a per-row dependency closure and a decision
about caching that closure, which is more than a session's tail end deserves. The
false-positive cost meanwhile is one `compile-check-fixtures.sh` after a pull —
and per issue 0445 the verdict now says so itself ("5th consecutive stale verdict
… suspect the probe before trusting the verdict"), which is how this was found.

## The zephyr lane is `skip_probe = true`, and it produced a false red (2026-08-12)

A worked example of the museum-binary hole, because this one cost real
investigation time and looked exactly like a live bug.

`entry_matrix`'s `zephyr/rust/qos` and `zephyr/rust/lifecycle` cells failed in
the tier-1 sweep with messages that read as genuine product faults:

```
zephyr/rust/lifecycle: `ros2 lifecycle nodes` listed no managed node —
    the entry's REP-2002 services are not on the wire (phase-276 W3 / #128)
zephyr/rust/qos: observer never saw 3 `/qos_ok` republishes from the entry
```

Both were artifacts, not code. The images were built 2026-08-07; issue 0460's
queryable-table fix landed and was verified 2026-08-10 with
`entry_matrix: 14 ran, 1 skipped, 0 failed`. The proof is in the built config
rather than in the dates:

| | 08-07 image | after `just zephyr build-fixtures` |
| --- | --- | --- |
| `CONFIG_NROS_MAX_QUERYABLES` | **8** (pre-fix) | **16** (the tree's value) |
| `entry_matrix` | FAIL, 2 cells | **PASS**, 39.8 s |

So the cells were reproducing a RESOLVED bug out of a stale binary, and the
failure text pointed at the product the whole time.

### Why the gate could not help

The zephyr rows are `skip_probe = true` — deliberately, and for a defensible
reason (west/nuttx machinery, each with its own signature, per the comment in
`check-fixtures-stale.sh`). The consequence is that `check-fixtures-stale` has
NOTHING to say about them: it self-heals rust fixtures, hard-errors on workspace
and compile-check ones, and is structurally blind to this lane. A three-day-old
zephyr image is therefore indistinguishable, to every gate in the tree, from a
current one.

That is the same shape as the compile-check finding above (gate narrower than the
tests) taken to its limit: here the gate does not look at all.

### What would fix it, in the spirit of this issue

The lane already knows how to sign itself — the exemption comment says these are
"own-lane artifacts (west / nuttx machinery, each with its own sig)". So the
missing piece is not a signature, it is REPORTING: something at the head of a
tier run should compare each zephyr fixture's own sig against the sources and say
"N zephyr fixtures predate their inputs — run `just zephyr build-fixtures`",
exactly as the workspace lane already does. That is fix 1 of this issue applied
to the one lane it does not cover.

Until then the operational rule is worth stating plainly, because inferring it
from a failing assertion is expensive: **a zephyr cell failing with a plausible
product-level message is a stale-image suspect first.** Check the built
`.config` against the tree before believing the assertion — one `grep` settled
this after the failure text had already sent two earlier sessions (0460, and this
one) looking at the product.

### Measured 2026-08-12: the premise above is half right, and the real gap is TWO SPELLINGS

"The lane already knows how to sign itself" is true, and "the missing piece is
REPORTING" is not — I checked, having first concluded (wrongly, from grepping
only for `inputsig`) that zephyr had no signature at all. What exists is **two
different zephyr freshness checks with different coverage**, and the entry
fixtures use the weaker one:

| check | watches | used by |
| --- | --- | --- |
| `zephyr.rs::is_binary_stale` | leaf `prj.conf`, its `conf_files`, `boards/`, `src/`, `CMakeLists.txt`, `Cargo.{toml,lock}`, `zephyr/`, language-filtered `packages/core/*`, the rmw package — content-aware (`candidates_changed_content`, #147 / phase-286 W2) | the per-example zephyr resolver, and the ONE base workspace entry (`build-ws-rs-entry-zenoh`) |
| `binaries/mod.rs::require_prebuilt_binary_fresh_zephyr` | ONLY `<build_root>/rust/target/*/<profile>/librustapp.d` vs the `zephyr.exe` mtime | every other zephyr entry — the C, C++, mixed, params, lifecycle, qos, safety entries |

Two consequences, both of which this session paid for:

1. **A source Kconfig edit is invisible to the second check.** Measured on
   `build-ws-rs-qos-entry-zenoh`: `librustapp.d` lists 529 deps, and the only
   `.conf` among them is the GENERATED `<build_root>/zephyr/.conf` — a build
   OUTPUT. `examples/workspaces/features/src/zephyr_rust_qos_entry/prj.conf`, the
   file an author edits, appears **zero** times. So raising
   `CONFIG_NROS_MAX_QUERYABLES` there leaves every image "fresh" while each one
   still has the old value compiled in — which is precisely how the qos/lifecycle
   entry cells failed with product-level assertions instead of a STALE verdict.
   The doc comment's claim is accurate about what it covers ("the dominant drift
   for these entries") and the drift it names is real; the conf axis simply is not
   in it.

2. **C and C++ entries have no freshness check at all.** `zephyr_staticlib_dep_
   file` looks for `librustapp.d`, a C-only image has none, and the helper then
   returns `Ok` — documented as "Missing `.d` → existence-only fallback". For the
   mixed entry it works by accident (that image does carry a Rust staticlib).

So the fix is not a new signature and not a report: it is to route
`require_prebuilt_binary_fresh_zephyr` through the SAME candidate machinery
`is_binary_stale` already implements, one spelling, which fixes both rows at
once. The blocker to doing it mechanically is that `is_binary_stale` keys on an
example NAME (`examples/<example_path_for_name(name)>`) while an entry's sources
live under `examples/workspaces/...`; the shape for bridging that already exists
as `ZEPHYR_WORKSPACE_ENTRY_SRC_KEY`, so the change is to take a source DIR and
let the ~10 entry resolvers name their leaf.

This is the issue-0196 rule again ("build-side stale probes must watch the same
inputs as test-side gates"), and it is the fourth gate found this year whose
coverage was narrower than the rule it enforced.


## FIXED 2026-08-15 — the two zephyr spellings are one, and every entry is covered

The last section named the fix precisely and it held up: route
`require_prebuilt_binary_fresh_zephyr` through the same candidate machinery
`is_binary_stale` already implemented, taking a source DIR so the workspace
entries can name their leaf.

What landed:

* `is_binary_stale(binary, example_name)` — the alias-keyed wrapper — is gone.
  Its body is now `zephyr::source_dir_is_stale(binary, example_dir, lang, rmw,
  conf_files)`, keyed on a directory. The alias decode moved to the two callers
  that have an alias.
* `require_prebuilt_binary_fresh_zephyr` takes a `ZephyrLeafSource { dir, lang,
  rmw, conf_files }` and runs BOTH halves: the staticlib `.d` (the real cargo
  dependency closure, which no hand-written list could enumerate) and the leaf
  source candidates (`prj.conf`, boards, CMakeLists, src, shared core + rmw
  crates). Neither subsumes the other, which is why the fix is a union and not a
  replacement.
* All 16 call sites name their leaf — 14 in `fixtures/binaries/mod.rs`, 2 in
  `zephyr.rs`. A `dir` that does not exist is a HARD error, not a silent pass:
  a typo there would watch nothing and reinstate the exact hole, which is the
  failure mode this issue is about.

### Verified by mutation, not by reading

`tests/zephyr_leaf_staleness.rs` appends a marker to a leaf's `prj.conf`,
asserts the RESOLVER now errors, restores the bytes, and asserts it stops. Both
directions are load-bearing: the second is #147's property that a
content-identical mtime bump is NOT stale, without which every pull reports the
whole lane stale and the verdict becomes noise.

It drives `build_zephyr_workspace_{rust,c}_realtime_entry`, not the probe
underneath them. That distinction is the point — `source_dir_is_stale` has
worked since #147; the defect was that the resolvers never called it, so a test
reaching past them would have passed on the broken tree.

Neutering the new half (`if false && …`) turns both tests red; restoring turns
them green. The C case is the one that previously had NO check at all
(`zephyr_staticlib_dep_file` looks for `librustapp.d`, a C-only image has none,
and the helper returned `Ok`).

### What this does NOT fix

The other items this issue accumulated are untouched and still open on their own
terms:

* finding (a), 2026-08-11 — `check-workspace-build-output` and
  `check-artifact-identity-budget` still are not inside the precondition batch,
  so long-lived-tree residue is still discovered last;
* finding (b) — no `nros setup --tool <t> --check`, so a provisioned tool that
  drifted behind its pin (corrosion 0.5.1 vs #0493's 0.6.1) still presents as a
  link failure;
* the compile-check signature still does not hash the row's in-repo path-dep
  closure, so that lane's gate remains narrower than its tests.

This issue has now absorbed four distinct defects across three months. The
zephyr half is done; the rest deserves its own number rather than a fifth
section here.


## Finding (a), fixed 2026-08-15 — one gate joined the batch, one was checked and declined

Finding (a) above named two gates that "already exist and could simply run
inside the precondition batch". One of the two no longer needs to.

**`check-workspace-build-output` — added.** Stop #2 of the five on 2026-08-11 (a
Jul-14 `target/` inside `examples/workspaces/mixed/src/rust_heartbeat_pkg`). It
is buildless and source-free, so it fits the batch's contract exactly. It was
already running in `check-fast`; what it lacked was a seat in the ONE listing,
so it arrived as its own round trip after the batch had reported green.

Verified by mutation rather than by reading: with a stray `target/` planted under
a workspace `src/` and a stale CLI, the batch reports

```
 2 tier precondition(s) unmet — ALL of them, not just the first
  [x] in-tree nros CLI is stale
  [x] build output beside workspace source (long-lived-tree residue)
```

which is the property this issue asked for — two causes, one verdict, one round
trip instead of two.

**`check-artifact-identity-budget` — deliberately NOT added.** The finding cites
it as stop #3 ("1.9 GB of Aug-7 rlibs under `mixed/build-workspace-fixtures`"),
but that failure mode is already gone: the `started_at` filter (issues 0499 /
0513) now answers exactly that tree with

```
[SKIP] artifact-identity budget: all N rlib(s) … predate started_at=… —
       this tree is history, not that build.
```

and this batch runs BEFORE fixtures are built, which is precisely when that
filter reports SKIP. Adding it here would contribute a line that can never fire
— a gate whose coverage is narrower than the rule it advertises, which is the
shape this tree has now paid for four times. It stays in `check-fast`, where the
tree it measures is the one the run produced.

Worth stating plainly because the issue text argued the other way and the text
was three days older than the fix: **check the gate's current behaviour before
acting on a finding about it.**

**Launch-resolve skew — added as a WARNING.** phase-354 W2's acceptance names it.
`setup-cli` warns when it leaves `nros-launch-resolve` older than the CLI and
deliberately does not fail (a CLI-only setup is legitimate, and the resolver has
its own skip conditions). But a warning at the tail of one recipe is not
something the next run re-states, so the skew reaches a fixture build and
surfaces there. The batch now re-states it. WARN, not fail, for the same reason
`setup-cli` warns: a resolver older than the CLI is only WRONG if the argument
list moved, which neither can know.

### Still open in this issue

* the compile-check lane's signature does not hash each row's in-repo
  path-dependency closure, so that gate stays narrower than the tests it gates
  (the issue-0196 shape);
* no `nros setup --tool <t> --check`, so a provisioned tool that drifted behind
  its pin still presents as a link failure (#0493's corrosion 0.5.1 vs 0.6.1).


## Finding (b), fixed 2026-08-15 — `nros setup --tool <name> --check`

The last item. Finding (b) said "a landed fix is not an applied fix": #0493 was
verified end-to-end on 2026-08-10 by bumping corrosion 0.5.1 -> 0.6.1, and the
duplicate-symbol link failure still reproduced in full the next day on a tree
where only 0.5.1 had ever been provisioned.

The reason that gap existed is smaller than it looked: `run_check_all` — the
doctor pass behind a bare `nros setup --check` — walked `[system.*]`,
`[rust.toolchain.*]` and `[rust.cargo_tool.*]`, and **not `[tool.*]`**. The SDK
store tools were the one declared class nothing verified. And `--tool <name>
--check` could not be asked at all: the generic `--check` branch preceded the
`--tool` branch, so it swallowed the name and walked everything.

Both fixed: `[tool.*]` joins the doctor pass, and `--tool <name> --check` scopes
to one tool and exits non-zero when it is not at its pin.

### What implementing it exposed: two store layouts, two version vocabularies

The first cut asserted `<store>/<tool>/<version>/` and reported corrosion
MISSING on a machine where it is installed and working. It is not a versioned
prefix — it is `<store>/corrosion/` with a `.installed-version` stamp reading
`v0.6.1`, while the index pins `version = "0.6.1-nros1"`.

Two provisioning paths, each with its own spelling of "what version is this":

| path | layout | version compared |
| --- | --- | --- |
| `nros setup --tool` | `<store>/<tool>/<version>/` | index `version` (`0.6.1-nros1`) |
| `just workspace install-*` | `<store>/<tool>/` + `.installed-version` | upstream tag (`v0.6.1`) |

No normalisation was needed in the end, because the index already declares BOTH
— `version` and `upstream` (`= source.ref`). The check reads each layout against
its own field. That is worth recording as its own small seam: one tool, two
installers, two truths, and `just workspace doctor` checks only the second while
`nros setup --check` now checks the first. They agree today by coincidence of
`upstream` being maintained.

Scope, deliberate: the check asks about the SHARED store. A `--prefix` install
(`build/zenohd`, `build/qemu`) is outside it by design, so "absent from the
store" is the right answer there rather than a false negative.

## Status

Every item this issue accumulated is now addressed:

* the zephyr `skip_probe` half — `52e6bda8e` (2026-08-14);
* finding (a), the precondition batch — one gate added, one checked and declined
  with evidence, launch-resolve skew reported;
* the compile-check lane's narrow gate — phase-360 W4, which reads the closure
  the build MEASURED instead of guessing it, extended to every builder;
* finding (b), tool-version drift — this section.

Resolved.

## Reopened finding, 2026-08-15 — the gate itself discovered one family per run

Recorded against this issue although it is already resolved: it is the same
defect one layer down, and the next person will look here.

`check-fixtures-stale.sh` checks three fixture families (rust, workspace,
compile-check) and each had its own `exit 1`, so a tree with two stale families
named ONE. Observed cost in a single afternoon: `ci-matrix` reported two stale
workspace fixtures; rebuilt; then ten stale compile-checks; rebuilt; then the
main fixture set (that last for a different reason — `lane=tier2` builds only
the coordinates the gate checks while the run executes everything). Four
rebuild-and-rerun rounds, one discovery each.

Fixed by deferring the exits: each family records its failure, all report, one
exit at the end names how many families are stale. Verified by hiding one stamp
from each of two families — before, the workspace exit masked all 24
compile-checks.

TRIED AND REJECTED: making `check-tier-preconditions` run the same probe, so
freshness is reported with the other preconditions. `NROS_FIXTURE_SCOPE=all
check-fixtures-stale.sh` measures **1468 s (24.5 min)** — a content signature per
row — so it costs more than the failure it prevents and cannot sit at the head of
every tier-1 run. Preconditions still checks fixture PRESENCE only; the lane gate
remains where freshness is established, and now reports completely.

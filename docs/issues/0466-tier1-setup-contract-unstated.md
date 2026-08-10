---
id: 466
title: "Tier 1 has an unstated, ORDERED setup contract — eight consecutive
  blockers, one of them a test"
status: open
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
   just check-fast   ->  failed in 0.77s, having checked NOTHING
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
`just check-tier-preconditions` at the head of `ci` reported the stale CLI with
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
`build/fixtures-cargo/linux/nros-relwithdebinfo/talker` did not exist; the tests
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

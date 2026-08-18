---
id: 652
title: "`required-features` test targets are in NO lane, so one of them has been
  failing unobserved and a stale feature forward sat in the manifest"
status: resolved
type: bug
area: testing
related: [issue-0319, issue-0599, phase-359, phase-329]
---

## Symptom

`nros-tests` declares test targets behind `required-features`. Nothing enables
those features:

```
$ grep -rn 'trigger-test' just/*.just justfile
(no matches)
```

So `just check`, `just ci` and `just test-all` all build the package WITHOUT
them, and the targets are never compiled, never linked, never run. Two things
were found sitting behind that:

**1. `trigger_conditions` fails.** Run it and it panics immediately:

```
thread 'test_guard_condition_with_zenoh' panicked at
packages/testing/nros-tests/tests/trigger_conditions.rs:32:48:
Failed to open session: Transport(InvalidConfig)
```

0.15 s, so nothing about the router timing out — `Executor::open` rejects the
config outright. `require_zenohd()` passed (the test skips otherwise) and
`build/zenohd/zenohd` exists, so this is not a missing-router skip; it is a
real failure. The file was last touched by a phase-122 API rename, so it has
plausibly been failing since long before this issue.

Verified pre-existing: it fails identically with and without the phase-359 W10
manifest change made in the same session, so nothing recent caused it.

**2. A stale feature forward.** `trigger-test` carried `"nros-node/std"`. None
of the three targets uses nros-node's std-only surface, and removing it leaves
`wake_latency` (3 tests) and `component_runtime` (3 tests) green — 6/6. It made
this crate one more grantor of the core's `std` flavour for as long as nobody
looked, which is exactly what phase-359 is removing elsewhere.

## Why this is the 0319 class

Issue 0319: *a backend's own test suite belongs in a `check-*` lane, never as a
named step on the `ci` line* — a red sat on main for two days because
`just check` never ran it. This is the same shape one level down: the gate is
not missing, the TARGET is unreachable. A `required-features` target that no
recipe enables is indistinguishable from a deleted one, except that it still
looks like coverage when you read the tests directory.

Issue 0599 is the sibling observation for a whole lane ("reports OK when it
skipped everything").

## Scope

`trigger-test` and `component-runtime-test` are the two features seen here;
the audit should enumerate every `required-features` target in the workspace
rather than fix these two by name — that is the "fix the CLASS" rule, and the
enumeration is a one-liner over `Cargo.toml` `[[test]]` blocks.

## Options

1. **Put them in a lane.** They are host tests needing only zenohd; a
   `check-trigger-tests` recipe alongside the other per-component lanes is the
   cheap version. Requires fixing `trigger_conditions` first, or the lane lands
   red.
2. **Gate the audit instead.** A check that every `required-features` value
   appears in at least one recipe — cheaper to keep honest, and it fails loudly
   the next time someone adds a target behind a new feature.
3. **Delete what is not wanted.** If `trigger_conditions` is obsolete (the
   guard-condition API has moved since phase 122), removing it is more honest
   than a target nobody runs.

(2) is worth doing regardless of which of (1)/(3) is chosen for these targets.

## Reproduce

```
cargo nextest run -p nros-tests --features trigger-test,component-runtime-test \
  --test trigger_conditions --test wake_latency --test component_runtime
```


## Audit 2026-08-17 — the scope is FIVE features and nine targets

The issue asks for the enumeration rather than a fix by name. Doing it changes
the size:

| feature | targets | in a recipe? |
| --- | --- | --- |
| `trigger-test` | trigger_conditions, wake_latency | no |
| `component-runtime-test` | component_runtime, tier_filter, component_dispatch, component_param | no |
| `loan-e2e` | loan_e2e | no |
| `phase216-substrate` | dispatch_strategy | no |
| `rmw` | custom_transport_loopback | no |

`rmw` is the one worth pausing on: it appears **250 times** across the justfiles
and is enabled as a feature exactly never — every hit is a substring of
`rmw-zenoh`, `check-rmw-cyclonedds` and friends. A reachability check written as
a plain grep would call it covered. The gate therefore matches only
`--features` / `features = ` positions.

### One target could not have run even with a lane

`dispatch_strategy` failed to compile: an unused `Callback` import, fatal under
`-D warnings`. Fixed here. Nine targets now build.

### Runtime state, and what this host cannot tell you

```
21 tests: 7 passed, 14 "failed"
```

All 14 are `[SKIPPED:capability] no rmw_zenoh_cpp/rmw_zenohd under /opt/ros` —
the `nros_tests::skip!` convention, which CLAUDE.md records bare `cargo nextest`
as reporting like a failure. This host has no `ros-humble-rmw-zenoh-cpp`.

So **the `trigger_conditions` failure reported above is not reproducible here**,
and nothing in this audit contradicts it: on a host with the ROS router the test
gets far enough to open a session, and on this one it never tries. Deciding
between "put them in a lane" and "delete what is obsolete" needs that host.

## Gate landed — option (2)

`check-required-features-reachable` asserts every declared `required-features`
value is enabled by at least one recipe. The five above are a BASELINE and it is
labelled a shrinking backlog, not an exemption — gating nine targets on day one
would fail immediately and get bypassed. What it buys now is that a SIXTH cannot
arrive silently, which is the property whose absence let these accumulate.

Mutation-tested both directions: a target behind an unlisted feature fails, and
a baselined feature becoming reachable fails with "remove it from BASELINE".

Still open: (1) versus (3) for the nine targets.


## The `trigger_conditions` failure, diagnosed and fixed 2026-08-18

Reproducible once `ros-humble-rmw-zenoh-cpp` was installed — this host could
previously only reach the capability skip, so the audit above could not see it.

`Transport(InvalidConfig)` reads like a bad locator. It was a MISSING BACKEND:
the file lacked

```rust
use nros_rmw_zenoh as _;
```

so rustc dropped the dependency, its `#[no_mangle]` registration never reached
the test binary, and `Executor::open` had nothing to select. `wake_latency.rs`
carries that line at its top; `trigger_conditions.rs` did not, which is exactly
why it was the one target that failed while its siblings passed with an
identically-shaped config on an adjacent domain.

Third instance of this class in one session, after the `use nros_platform_cffi
as _;` anchors that issues 0619 and 0612 needed. A dependency nothing references
is not linked, and the resulting error never names the dependency — worth a gate
of its own: a test that opens an `Executor` against an RMW must carry the
anchor.

## Where the nine targets actually stand

With the router present, all nine RUN — 21 tests, 18 passing:

| feature | targets | state |
| --- | --- | --- |
| `trigger-test` | trigger_conditions, wake_latency | GREEN (after the anchor above) |
| `phase216-substrate` | dispatch_strategy | GREEN (after the unused-import fix) |
| `component-runtime-test` | component_runtime, tier_filter, component_dispatch, component_param | 3 green; `component_param` asserts `left: 0` |
| `loan-e2e` | loan_e2e | assertion failure |
| `rmw` | custom_transport_loopback | STALE FIXTURE, not a code failure |

So the issue's option (1) — put them in a lane — is now the right answer for at
least six of the nine, rather than a guess. The two real failures want triage
first, and the stale fixture wants a `just build-test-fixtures lane=native`;
none of that is knowable without the router, which is the point the audit made
and could not act on.


## Resolved 2026-08-18 — laned, and the three defects that were hiding

Running the targets is what found them. Each had rotted precisely because
nothing ran it:

| target | symptom | cause |
| --- | --- | --- |
| `trigger_conditions` | `Transport(InvalidConfig)` | missing `use nros_rmw_zenoh as _;` — the backend was never linked, and the error reads like a bad locator |
| `dispatch_strategy` | did not compile | unused import, fatal under `-D warnings` |
| `component_param` | `assert_eq!(…, 1)` got 0 | pre-phase-258 observable: the seam moved to the executor's component-tick registry, and `component_dispatch.rs` had ALREADY recorded the correct one for the identical path |

The last is the sharpest version of this issue's thesis. The right answer was
written down, in a sibling file, for the same code path — and this file kept the
stale one because no lane ever executed it.

### The lane

`check-required-features-tests` runs seven targets, 18 tests, in `check-build`.

Two are deliberately NOT in it:

* **`loan_e2e` is not broken — it is mis-laned.** It opens two in-process
  sessions and needs `ZPICO_MAX_SESSIONS=2`, which is a BUILD input with its own
  target dir. Verified 2/2 passing under `test-zpico-multisession`'s env; it
  belongs there.
* **`custom_transport_loopback`** needs a native fixture (stale on this host),
  so it wants a fixture-gated lane rather than this one. Still unassessed.

### FOLLOW-UP 2026-08-18 — `custom_transport_loopback` assessed; baseline now 1

The fixture was rebuilt (`just build-test-fixtures lane=native`) and the target
is **not broken**: 3 TCP hops, 3 messages delivered, PASS in 0.58 s. It was
never a code failure, only an unassessable one.

It joined `test-all`'s tail rather than `check-required-features-tests`, because
the distinction this issue drew is real: `test-all` depends on
`_require-fixtures`, that lane does not, and this is the one target here that
needs a prebuilt binary.

Reaching it needed one change to `_nextest-platform`, which had no way to name a
feature. The caller passes the **whole flag** (`"--features rmw"`), not a bare
feature name — a `--features {{feature_args}}` in the helper would leave `rmw`
spelled nowhere in the justfile, and this gate reads reachability off the
literal `--features` text. That would have made the gate narrower than its own
rule (issue 0196) while reporting green, which is the failure mode this issue
exists to prevent.

Mutation-checked both ways: with the flag, `Starting 1 test across 1 binary`;
without it, cargo exits 101 and the issue-#29 guard refuses to mask that as a
`[SKIPPED]` precondition.

### Gate baseline shrunk 5 -> 2

`check-required-features-reachable` now baselines only those two, each with its
reason in the source. A shrinking backlog rather than an exemption — which is
what the gate's own comment promised when it went in with five.

### Not reproducible before today

Every one of these needed `ros-humble-rmw-zenoh-cpp`. The audit above could see
that the targets were unreachable but not what was wrong inside them, because
this host could only reach `[SKIPPED:capability]`.

## Follow-on 2026-08-18 — `loan_e2e` was named a home and not given one

The section above concluded `loan_e2e` "belongs to `test-zpico-multisession`,
which already owns that env and its own target dir", and recorded it verified
2/2 under that env. The recipe was never edited, so the target stayed in exactly
the state this issue is about: in no lane, with a note explaining where it ought
to be.

That is the failure mode a baseline invites, and the gate caught it. Adding the
two `cargo nextest` lines to `test-zpico-multisession` made `loan-e2e` reachable
and `check-required-features-reachable` failed with *"baselined feature(s) now
reachable — remove them from BASELINE; it is a shrinking backlog, not an
exemption"*. Baseline 2 → 1.

The test also now asserts `ZPICO_MAX_SESSIONS >= 2` and names the recipe. The
remedy for that failure is a rebuild under a different environment, which no
runtime error can express — left to itself the test reports
`Transport(InvalidConfig)`, which reads as a bad locator. That is the same
misdirection `trigger_conditions` gave, from the other cause.

Remaining: `rmw` / `custom_transport_loopback`. Recorded here as "unassessed —
the fixture is stale"; it has since been run on rebuilt fixtures and **passes**.
So it is not broken either, and what it lacks is a lane that can guarantee the
fixture rather than a fix.

`signal_fd_wake` joined the lane from `nros-node` (issue 0612), taking it to
20 tests. Its first real execution failed outright → issue 0667.

## The lane cannot pass on a host without ROS (noted 2026-08-18, by another session)

`check-required-features-tests` runs a BARE `cargo nextest`, and a bare run
counts `nros_tests::skip!` panics as FAILURES — only `test-all`'s junit rewrite
turns them into skips (CLAUDE.md records this). Eleven of the lane's targets
reach `zenohd_router`, which on a host with no ROS install skips:

```
[SKIPPED:capability] no `rmw_zenoh_cpp/rmw_zenohd` under /opt/ros (ROS_DISTRO=unset)
…
Summary [0.020s] 18 tests run: 7 passed, 11 failed, 0 skipped
error: recipe `check-required-features-tests` failed with exit code 100
```

So `just ci` is red here for a reason that is not a defect in anything the lane
tests. This is consistent with the commit's own note — *"None of this was
reproducible before `ros-humble-rmw-zenoh-cpp` was installed"* — the lane was
written and verified on a host that has ROS, and tier 1 is meant to run
everywhere.

Not touched from here; it is an hour-old gate and its owner should pick the
shape. The two that fit existing precedent:

* route the lane's output through `rewrite-skipped-junit`, as `test-all` does,
  so a capability skip stays a skip; or
* let the lane deselect the router-dependent targets when no router is
  resolvable, the way `lane_scope` narrows a run by coordinate — with the skip
  REPORTED, so the green states what it did not cover (issue 0650's rule).

Evidence it is not a regression from the concurrent `#0659` work: the lane
appears zero times in two `just ci` logs from before that rebase and twice
after, and the sibling gate's own line changed across it
(`5 declared, 5 baselined backlog` → `5 declared, 2 baselined backlog`).

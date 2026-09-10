---
id: 1177
title: "`check::workspace-features` gates no merge, so its reds are found by
  embedded consumers -- two so far, and the tier it should move to is open"
status: open
type: bug
area: testing, build, c-api
severity: high
related: [issue-0196, issue-0952, issue-0993, issue-1175, phase-395, phase-417]
---

## Status 2026-09-11

Re-triaged against `origin/main` because three `fix(#1177)` commits
(`9fd7e3457`, `04e597288`, `8fdb9631a`) read as if they closed it. They closed
the BREAK half only — the coverage half is still open, and nothing since has
decided the tier question:

* No workflow step runs `workspace-features` on a merge-gating event. Its only
  caller is `check build`, and `no-std` is its own step guarded
  `contains(fromJSON('["schedule","workflow_dispatch"]'), …)`
  (`.github/workflows/gate.yml:983-984`).
* Option B was not taken: `compile-smoke`'s `nros-c` row is still
  `C_API_SHIPPED_FEATURES` (`std,…`), with no `panic-platform,rmw-cffi,lending`
  row (`just/check/lanes.just`, `compile-smoke`).
* `workspace-all` MOVED since this was written — #825 put it on
  `pull_request` as well as `merge_group`, so the table row below reading
  "merge_group" is now "pull_request, merge_group". It still does not cover
  this row: `nros-c` declares `host-only = true` (`packages/api/nros-c/
  Cargo.toml:258-260`), so its embedded arm passes `--exclude nros-c`.
  (#875, queued, gives `workspace-all` a real host clippy; it runs `-p nros-c`
  never, because `nros-c` is in `HOST_UNCHECKABLE`, so it does not bear on
  this either.)
* The rows are green today, re-measured on this tree — both `lending`
  clippies with `-D warnings` (`panic-platform,rmw-cffi,lending` and
  `std,rmw-cffi,lending,platform-posix`) and the default-features
  `cargo test --no-run -p nros-c` that was red at `f0f97de42`: rc=0 all three.

So the status is unchanged: a maintainer's choice among A / A-narrow / B / C
below. Green rows are not coverage — the next break in this class still
reaches `main` the same way the first two did.

## Where this stands

Two halves, and only one of them is still open.

**The BREAK is fixed, twice.** The eleven errors this issue was filed for --
`nros-c`'s spin loop calling five `#[cfg(feature = "alloc")]` executor methods
ungated -- were fixed by `9fd7e3457`, which moved the gate off the four methods
that never needed an allocator. Re-measured on `origin/main` at `f0f97de42`, the
command in the original text exits 0:

```
cargo clippy -p nros-c --no-default-features \
    --features "panic-platform,rmw-cffi,lending" -- -D warnings     rc=0
```

The lane was STILL red at `f0f97de42`, on a different row and the same defect
class, and that red is fixed in the change carrying this rewrite:

```
just check workspace-features
  - nros-c: test-compile (default features)
cargo test --no-run -p nros-c --quiet
error[E0432]: unresolved import `nros_node::executor::TimerClockSource`
    --> packages/api/nros-c/src/clock.rs:65:9
note: found an item that was configured out
    --> packages/core/nros-node/src/executor/mod.rs:126:16
125 | #[cfg(any(has_rmw, test))]
126 | pub use arena::TimerClockSource;
```

`nros-c`'s default feature set is `panic-platform` alone, so `nros-node` is
built with no `rmw-cffi`, so its build script emits no `has_rmw`, so
`TimerClockSource` is not there. `clock` is one of `nros-c`'s
backend-INDEPENDENT modules and compiles anyway. The two sides of the class are
swapped from issue 1175's -- there a gated item had an ungated caller, here
gated callers had an ungated callee.

Both reds reached `main` and stayed there. The lane that names them is the same
lane in both cases, and in both cases it took a person running it by hand: the
first surfaced when the safety island could not build against `main`, the
second when this issue was re-measured. No `pull_request` and no `merge_group`
ran it either time.

**The COVERAGE hole is unchanged, and is why.** That is what this issue is now
about, and it is not settled: see "The tier question" below.

## What no merge-gating event does, re-verified on `f0f97de42`

| lane | event | does it compile `nros-c` with neither `std` nor `alloc`? |
| --- | --- | --- |
| `check fast` | every event | no -- buildless by contract |
| `check compile-smoke` | pull_request | no. `--workspace --all-targets` carries `{{HOST_UNCHECKABLE}}`, which holds `--exclude nros-c`; the crate's own row uses `C_API_SHIPPED_FEATURES`, which starts `std,` |
| `check cli-tests` | pull_request, merge_group | no -- a separate workspace |
| `check node-std-tests` | pull_request, merge_group | no -- `nros-node`, `--features std` |
| `test-unit` | merge_group | no -- each crate at its own defaults |
| `check workspace-all` | merge_group | no. Its embedded arm derives excludes from `[package.metadata.nros] host-only`, and `nros-c` declares it ("cdylib/staticlib C ABI surface"), so `scripts/build/host-only-members.sh` emits `--exclude nros-c` |
| `check no-std` | schedule, dispatch | no -- `nros-c` is absent from its crate list |
| `check workspace-features` | schedule, dispatch | **yes** -- the two `lending` rows, and the default-features test-compile row above |

So exactly one lane holds the row, and `gate.yml` runs it behind
`contains(fromJSON('["schedule","workflow_dispatch"]'), github.event_name)`.
A gate that never runs is indistinguishable from a gate that passes (issue
0196), and two reds now say so.

Worth stating beside it: `check no-std` -- which carries the embedded no-`alloc`
coverage for the CORE crates -- is on the same schedule-only step. Whatever is
decided below applies to it by the same argument, at its own price.

## The question the issue said needed a CI run: answered, without one

The original text said the obstacle was that "`nros-c`'s build script needs a
cross C toolchain, which is why `nros-c` is absent from `check::no-std`", and
that whether the PR runner has one "is answerable in one CI run".

It needs no run. That sentence is about `check::no-std`, whose rows carry
`--target thumbv7m-none-eabi` / `--target riscv32imc-unknown-none-elf`.
Every `nros-c` row in `workspace-features` carries NO `--target`: they are host
clippy invocations, and the PR `check` job already runs a host
`cargo check -p nros-c` inside `compile-smoke` and has done since issue 1163.
The cross toolchain is not a precondition of moving these rows.

`workspace-features` does have one precondition, and the PR job already meets
it: two rows (`--workspace` test-compile, and `zpico-sys --features link-ivc`)
need the vendored `-sys` sources, and `gate.yml`'s
"Provision compile-tier sources" step runs `nros setup --source ...` on
`pull_request`. That satisfies `check-lane-contracts`' rule -- a gate on an
affordability tier may only resolve artifacts the job itself builds -- because
the job builds them. The lane resolves no runtime fixture, no SDK and no QEMU.

## The tier question -- OPEN, with prices

### How these were measured

CI numbers are from real runs, not estimates:

* PR `check` job, run `34153035344` (green, `pull_request`): whole job 1004 s,
  of which `just check fast` 177 s, `compile-smoke` 109 s, `cli-tests` 174 s,
  `node-std-tests` 23 s. Four cores, in the `nano-ros-ci:humble` container.
* Nightly `check` job, run `34075012860` (`schedule`): `just check build` alone
  1329 s, after 1189 s of `generate-bindings` and 71 s of compile-check
  fixtures that only that event builds.

Local numbers are on a 20-core host with the global `sccache` wrapper live, in
a scoped `CARGO_TARGET_DIR` seeded by running `just check compile-smoke` first
-- i.e. the state the PR job is in when it reaches the new step, not a truly
cold tree. The host was also running a self-hosted runner, so they are upper
bounds for a quiet machine and they move with load: `compile-smoke` itself
measured 37 s in one probe and 48 s in the next.

`compile-smoke` is also the scale anchor, because CI runs it: 109 s on the PR
runner against 37 s / 48 s here, a factor of 2.3 to 2.9. Each option below is
scaled by the anchor from its OWN probe, and the CI column is an ESTIMATE with
a stated basis -- only a CI run settles it.

| | local, after compile-smoke | CI estimate | on the PR job's 1004 s |
| --- | --- | --- | --- |
| A: whole `check workspace-features` | 222 s (anchor 37 s) | ~10.5 min | +64 % |
| A-narrow: the four `nros-c` rows only | 95 s (anchor 48 s) | ~3.5 min | +22 % |
| B: one no-`alloc` `nros-c` row | 15 s (anchor 37 s) | ~45 s | +4 % |
| C: stay nightly, add a reporter | 0 s | 0 s | 0 % |

The nightly bounds A from above: the whole 21-gate `check build` tier, fanned
out at -P4 in the same container, is 1329 s, and `workspace-features` is one
gate in it.

`workspace-features` re-run warm on an already-built tree is 32 s in the same
scoped dir (136 s in the shared one). That is what a contributor pays locally
before a push; it is not what a fresh CI checkout pays, and quoting it here
would understate A by a factor of seven.

### Option A -- move `workspace-features` to `pull_request` (+ `merge_group`)

A new step in `gate.yml`'s `check` job, guarded the way `cli-tests` and
`node-std-tests` are. Those two are the precedent and they are exact: both
lived only in `check-build`, both were moved out on a measured cost after a red
reached `main`, and both stayed in `build-serial:`.

Buys: every row in the lane -- eleven feature combinations, `-D warnings` on
the `nros-c` ones, the `zpico-sys link-ivc` row that reaches the Orin island
and is on no other lane, and the no-`alloc` rows this issue is named for.

Costs: about 11 minutes on a job that takes 16.7. It also widens the ONE
required `CI` context: a red anywhere in eleven feature combinations blocks
every pull request, which is the frozen-repo failure phase-396 recorded and
which `workspace-features` was in as recently as `f0f97de42` -- so this option
is the one whose downside has a live example in this very issue.

`.config/ungated-gates.txt` needs no edit under A. `check-gate-visibility`
derives the ungated set from `build-serial:` MEMBERSHIP, and a per-gate step
does not change that -- which is why `cli-tests` and `node-std-tests` are still
listed there while running on every PR. That is a real wart (the ratchet file
now overstates the invisible set by two, three with A), but fixing it is a
change to the gate, not to this lane.

### Option A-narrow -- the four `nros-c` rows, per PR; the rest stays nightly

The cost in A is not spread evenly. The `--workspace` test-compile row builds
every crate's test targets under `--no-default-features`; the four rows that
name `nros-c` are what this issue is about, and they are what caught BOTH reds:

```
cargo clippy -p nros-c --no-default-features --features "std,rmw-cffi,platform-posix,ros-humble"
cargo test   --no-run  -p nros-c                                        # the f0f97de42 red
cargo clippy -p nros-c --no-default-features --features "panic-platform,rmw-cffi,lending" -- -D warnings
cargo clippy -p nros-c --no-default-features --features "std,rmw-cffi,lending,platform-posix" -- -D warnings
```

Buys: both reds, `-D warnings` on the two `lending` rows, and the C-API surface
in the shape an embedded consumer links. Leaves behind: `link-ivc`, the
`--workspace` test-compile, and the four `nros` / `nros-rmw` rows.

Costs: 95 s locally, ~3.5 min estimated on the runner, +22 % of the PR job --
under half of A for the two reds that actually happened, because the
`--workspace` test-compile row is most of A's 222 s.

It also needs a decision the other options do not: whether the
lane SPLITS (a second recipe, so `just check build` does not run the rows
twice) or whether the step just repeats the four commands, which is a second
spelling of a feature set and the drift `C_API_SHIPPED_FEATURES` exists to
prevent.

### Option B -- one no-`alloc` `nros-c` row inside `compile-smoke`

```
cargo check -p nros-c --no-default-features \
    --features "panic-platform,rmw-cffi,lending"
```

Buys: exactly the class this issue is named for -- the configuration an
embedded consumer links -- on a lane that already runs per PR and already
compiles this crate.

Costs: ~45 s. Does NOT buy the rest of the lane: no `-D warnings` (compile-smoke
is a `cargo check`, "no lints, see check-build"), no `link-ivc`, no
default-features test-compile -- which is the row that was red at `f0f97de42`,
so B would not have caught the second red.

### Option C -- leave it nightly and make the red loud

Costs nothing on the PR path. But `nightly-report.yml` triages the `nightly`
workflow, and `just check build` runs in `gate.yml`'s `schedule`, which nothing
reports on -- so C is not free either; it is a reporter that does not exist yet.
And the latency it accepts is what this issue measured twice: the nightly WAS
red on both of these, on consecutive days, and both were still found by hand.
The `schedule` runs on 2026-09-01 through 2026-09-07 all report `failure`, so a
new red in this lane is indistinguishable from yesterday's -- a lane already
red has no signal capacity, which is the second half of what C has to fix.

### What the decision turns on, stated rather than decided

This is a maintainer's call and is deliberately left open here. It is not a
question of fact any longer -- the toolchain obstacle is answered, the lane
contract is satisfied, and the four prices are above. It is a trade between two
things this repository has already paid for in both directions:

* **Latency and blast radius.** Every second here lands on every push an agent
  makes, and every row here becomes a way for the one required `CI` context to
  go red for a reason that is not the contributor's. Six merge groups and three
  authors were blocked for a day the last time a lane joined that set before it
  was green (phase-396).
* **Latency of DISCOVERY.** The alternative is what this issue measured twice:
  the red is found by a person building an embedded image, and the nightly that
  would have said so is on a workflow nothing reports on.

A: buys the most, costs the most, and its downside has a live example above.
A-narrow: buys both known reds, and asks for a lane split so the rows keep one
spelling. B: cheapest, and would have caught only one of the two reds.
C: free on the PR path, and is not actually free -- it is a reporter that does
not exist.

### What is NOT the answer

Putting `check-build` back on the merge group. It resolves generated bindings
and prebuilt `.compile-ok` artifacts no CI job builds, it was there once and
could never pass, and the rule that came out of it is right. The whole tier at
1329 s is also the wrong shape for a per-PR lane; issue 0993 measured that and
took it back off.

## Do not

Do not fix the symptom by giving `nros-c` `std` in the lane. The whole point of
the combination is that an embedded image has neither `std` nor `alloc`, and a
lane that builds the easy configuration is the lane we already have.

---
id: 1395
title: "The provisioned-root ownership guard cannot answer before the build it
  is about — every fixture-build front door reaches it only afterwards, or not
  at all"
status: resolved
type: tech-debt
area: ci, zephyr, build
related: [issue-1253, issue-1360, issue-1226, issue-1040, phase-449, phase-450]
---

## Problem

`scripts/check-zephyr-workspace-checkout.sh` asks whether a provisioned root
(Zephyr west workspace, esp-idf, `external/`) belongs to THIS checkout. It is
correct, it is cheap — it resolves four paths and reads one `.west/config` — and
its west-manifest arm covers exactly the runner shape that produced issue 1253
and issue 1360.

**It cannot fire before the step it is about.** In every CI lane that builds
Zephyr fixtures, the build is one step and the guard is in the next one.

### The call graph, traced 2026-09-20 on `origin/main` (5321beab4)

`rg -n 'check-zephyr-workspace-checkout'` finds exactly three executable call
sites. Nothing else in the tree runs the script.

| # | caller | reached by |
| --- | --- | --- |
| 1 | `scripts/check-tier-preconditions.sh:86` (a `probe`) | `just ci tier1` (`just/ci.just:127`), and `just check tier-preconditions` by hand |
| 2 | `just/ci.just:283` — `ci::_matrix-run` | `just ci matrix` (run depth) |
| 3 | `just/ci.just:346` — `ci::matrix-nightly` | `just ci matrix-nightly` |

Sites 2 and 3 are phase-449 W2, landed for issue 1253, and they are placed
immediately after `_lane-gate` and before the RUN. That is correct as far as it
goes. It does not go as far as the build, because **CI splits the two into
separate steps and the build is the first of them**:

```
.github/workflows/run-matrix.yml
  - name: just build tier2          # <-- the Zephyr fixtures are compiled HERE
  - name: just ci matrix            # <-- the guard runs HERE

.github/workflows/nightly.yml
  - name: just build tier2-nightly  # <-- HERE
  - name: just ci matrix-nightly    # <-- guard HERE
```

Issue 1253's own table records where those runs actually stopped: *"the last
seven stopped in `just build tier2`, in the Zephyr family"*. The guard sits one
step past every one of them.

### The four front doors, and what each reaches

`just build <scope>` (`justfile:240`) dispatches through `_build-scope`, and
neither arm touches the guard:

* **lane arm** → `just build-test-fixtures lane=<lane>` (`justfile:1788`). Its
  dependency list is `check::fast _require-build-sources _clear-fixture-stamp
  _codegen build-zenoh-posix-fixture (build-test-fixtures-leaves lane)`. None of
  those reaches the script. This is the path `just build tier2` takes.
* **platform arm** → `just <plat> build-fixtures`, i.e. `just/zephyr-ci.just:22`
  for Zephyr. No dependencies at all. This is the path `live-peer.yml` takes
  (`just build zephyr`).
* `build-all` (`justfile:446`) calls `build-test-fixtures-leaves` directly,
  bypassing even `build-test-fixtures`.
* `ci::_matrix-build` → `l3` — the lane `queue.yml` and `build-wide.yml` run on
  every merge group — reaches the guard on no path whatsoever.

So the script's reach is: the tier-1 preflight, and two tier-2 recipes that run
after the build has already happened. Not one of the four ways this tree starts
a fixture build asks the question first.

### Consequence

The guard's own header states the complaint it was written to remove — the
refusal *"arrives ~15 minutes into a tier-2 fixture build, inside a cmake
configure, as an error about a binary — naming neither the workspace, nor the
second checkout, nor the fix"*. That is still exactly what happens, because the
only thing that would have said it earlier runs later.

This is issue 1226's shape (a gate that works and does not run where its rule
applies) crossed with issue 1040's (a placement that cannot report in time), and
it is the class [phase-450](../roadmap/phase-450-gate-reach-narrower-than-its-rule.md)
collects: **a gate that is green while the defect it exists for is present**, because
it was not asked.

### Provenance

Recorded in issue 1360's *"What is deliberately NOT changed"* and never filed:

> `check-zephyr-workspace-checkout.sh` (issue 1253) already asks whether a
> provisioned workspace belongs to this checkout, and its west-manifest arm
> covers exactly this runner shape — but it is reached only from
> `check-tier-preconditions`, i.e. from `just ci`, one step AFTER `just build`.
> That is issue 1226's shape and worth its own change.

That note predates phase-449 W2, so its "only from `check-tier-preconditions`"
is now stale in the detail and **unchanged in the conclusion**: the two sites
W2 added are in the run lane, not the build lane.

1360's fix made the Zephyr workspace repair itself (the emitted codegen version
became a freshness input), so the guard is no longer load-bearing for *that*
failure. It is still load-bearing for 1253's, which is open, and for the
esp-idf and `external/` roots the guard also sweeps (phase-440 W5) — none of
which has a self-repair.

## Non-goals

* Making the guard a `check::fast` gate. That lane's contract is *buildless and
  source-free, green in 23 s on a pristine detached worktree*, and the guard
  honours it on a pristine tree — but it reads the ENVIRONMENT, and a worktree
  that has not run `source ./activate.sh` inherits the parent checkout's SDK
  paths (issue 1280). Measured in this worktree: red without
  `activate.sh`, green with it. `check fast` is the lane CLAUDE.md tells
  everyone to run before every push and it carries no `activate.sh`
  precondition; a fixture build does (the sweep contract). The guard belongs
  where the precondition already holds.
* A second copy of the check anywhere. Issue 0196's rule: widen the reach of the
  one that exists.

## Resolution (2026-09-20)

**One shared recipe, four call sites, and a gate that re-derives the reach.**
The check itself is untouched — this is issue 0196's widen-the-reach, not a
second copy.

### The recipe

`justfile`'s `_require-owned-provisioned-roots` runs
`scripts/check-zephyr-workspace-checkout.sh` and nothing else. Every front door
reaches the guard through it, which is what makes the reach auditable at all.

### Where it is wired, and why each

| site | why it is not covered by the others |
| --- | --- |
| `build-test-fixtures` (dependency, FIRST — ahead of `check::fast`) | the lane build. `just build tier2` and CLAUDE.md's `just build-test-fixtures lane=…` |
| `build-test-fixtures-leaves` (dependency) | `build-all` calls the fan-out directly, bypassing `build-test-fixtures` |
| `just/zephyr-ci.just::build-fixtures` (first body line) | `just build zephyr` — `_build-scope`'s PLATFORM arm, which `live-peer.yml` runs — reaches this lane through neither dependency, as do the filtered `just/zephyr-dev.just` recipes |
| `just/esp32.just::build-fixtures`, `just/esp_idf.just::build-c-port` | the same gap for the other provisioned root the guard sweeps |

`just` runs a dependency at most once per invocation, so the two dependency
edges cost one run, not two.

**Placement inside the Zephyr lane was MEASURED, not chosen.** It first went
after `nros_lane_platform zephyr`; against a foreign workspace the lane then
died three lines earlier, at `nros_ensure_central_patch`, which invokes `nros`
and therefore hit the phase-431 ownership guard — printing *"this `nros` does
not belong to the checkout"* and naming a BINARY. That is issue 1253's original
complaint verbatim, so a guard behind it replaces nothing. It is now the first
statement in the body. Before `nros_lane_platform` costs nothing: an
unprovisioned host resolves no root, exits 0, and still reaches its own SKIPPED
verdict (issue 0599).

`just/ci.just`'s two phase-449 W2 sites were converted from `bash scripts/…` to
the shared recipe. They stay — they are correct for the RUN lane — but as one
spelling.

### The gate

`scripts/check-provisioned-root-guard-reach.py`, `just check
provisioned-root-guard-reach`, fast lane. Four rules (R1 one spelling, R2 lane
front doors, R3 one lane per provisioned root, R4 no bypass); the ROOT NAMES in
R3 are READ from the guard script's own `PROVISIONED_ROOTS` block, so a root
added there fails here until it is classified rather than silently acquiring no
coverage.

It found a real defect on its first run: **both `just/ci.just` sites were a
second call site for the script**, which is what had made the reach
un-auditable.

Its `--selftest` plants each of the four defects and requires the matching rule
to fire, plus a negative control on the unmodified tree; per
`check-gate-selftests` it runs on the NORMAL path too, since this gate is green
exactly when the reach is intact — which is also what a gate that stopped
looking prints.

### Acceptance

```
$ python3 scripts/check-default-gates-run-somewhere.py --survey | grep provisioned-root
  fast    provisioned-root-guard-reach    merge_group,pull_request,push,schedule,workflow_dispatch

$ python3 scripts/check-lane-contracts.py
check-lane-contracts OK — 18 test target(s) across 3 affordability tier(s) and 19 CI
lane invocation(s) (3 merge-gating, 16 report-only); none resolves an artifact its job
does not build.
```

`check-lane-contracts` is unaffected by construction: `build-test-fixtures` is a
PRODUCER, not an affordability tier, and the guard resolves no build artifact —
it reads four paths and one `.west/config`.

**Mutation (staged under `tmp/`, nothing near `~/.nros`).** A scratch workspace
outside any foreign checkout whose `.west/config` names a manifest repo carrying
`zephyr/module.yml` + `packages/core/nros-core/Cargo.toml` — issue 1253/1360's
exact shape, where the ownership arm is silent and the manifest arm is not:

```
BEFORE (origin/main) — what build-test-fixtures asked first:
  build-test-fixtures lane="all": check::fast _require-build-sources \
      _clear-fixture-stamp _codegen build-zenoh-posix-fixture (build-test-fixtures-leaves lane)
  (nothing on that list reaches the guard)

AFTER — just build-test-fixtures lane=tier2        exit=1
AFTER — just zephyr build-fixtures                 exit=1
  zephyr workspace: <repo>/tmp/mutation/zephyr-workspace
      its west manifest (the nano-ros Zephyr module) is <repo>/tmp/mutation/other-checkout
      this tree:                                         <repo>

CONTROL — a workspace this tree owns                guard exit=0
```

Both front doors refuse, before any west build, naming the workspace, the
foreign checkout and this tree — which is the message the guard was written to
deliver and could not.

### What this does NOT close

Issue 1253 stays open. This makes its refusal arrive in time; it does not
provision the runner's workspace correctly, which is that issue's subject. The
`_matrix-build` lane (`queue.yml`, `build-wide.yml`) still reaches the guard on
no path — deliberately: it is `l3`, a cross build+link that touches no
provisioned root.

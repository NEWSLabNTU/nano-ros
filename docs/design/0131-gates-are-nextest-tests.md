---
rfc: 0131
title: "Every gate is a nextest test, and a lane is a capability claim"
status: Draft
since: 2026-11
last-reviewed: 2026-11
implements-tracked-by: [phase-543]
supersedes: []
superseded-by: null
---

# RFC-0131 — Every gate is a nextest test, and a lane is a capability claim

**Status:** Draft (2026-11-15). This RFC records the design that
[phase-543](../roadmap/archived/phase-543-nextest-gates.md) built (W0-W6, merged). It
adds nothing new. The decisions marked "maintainer" were made on 2026-11-06
and 2026-11-07, in the phase doc. It goes to Stable after the maintainer's OK.

## Summary

A gate (a `just check <name>` recipe, or a `[group('gate')]` recipe outside
the `check` module) runs as one test of `packages/testing/nros-gates`. That is
a custom nextest harness with no dependencies, and it runs each gate's own
recipe. Filters, timings, junit, partitioning, process-group kills and
resource limits are therefore one interface, shared by gates and tests.

Which gates run is decided by a lane's SCOPE, never by the host. A lane names
the tiers it runs and the capabilities it claims. A gate declares the
capabilities it needs. A gate the lane does not claim is listed as not run, by
name. A gate the lane does claim must pass: under the harness, a runtime skip
is a red `[UNMET PRECONDITION]`. This is the test rule of issue 1758 applied
to gates.

## Motivation / problem

Before phase-543, gates ran through `scripts/build/run-gates-parallel.sh`, a
bespoke fan-out. Its lanes were authored lists: the `build-serial:` recipe and
`.config/gate-lane-exempt.txt`. About 60 call sites skipped at runtime and
exited 0 (`nros_check_skip`). The result was a lane that could read green over
gates that never ran. A unit-bearing gate (`template-copy-out`) passed as a
whole when one of its units built, even though others had skipped. Meanwhile
every test lane already ran under nextest, so gates and tests had two
interfaces for the same jobs: filters, timeouts, junit, sharding and kills.

## Design

### D1. One runner: a custom nextest harness (maintainer)

- `packages/testing/nros-gates` is a `harness = false` binary with zero crate
  dependencies. It is its own cargo root, with a tracked lock in
  `[rust] locked_roots`, its own `.config/nextest.toml` and its own target dir.
  It never resolves the root workspace, never runs the `nros` CLI and never
  reads a submodule. So it runs on a pristine checkout (issues 0650, 1373),
  and builds cold in under a second.
- Test names are `<tier>::<gate>`, and `<tier>::<gate>#<unit>` for a unit.
  Running a test runs `just check <gate>` (or the recipe's own path), with
  `NEXTEST_*` cleared, because build gates run their own `cargo nextest`.
- nextest is the index-pinned `cargo-nextest`, taken from the store path built
  from `nros-sdk-index.toml` (`just check nextest-gates`). It is never taken
  from `PATH`. A host without the pin gets a refusal that names the remedy.
- The listing comes from one derivation, `scripts/check/check-gate-lists.py`,
  which reads the recipes through `just --dump`. Adding a gate is writing its
  recipe, the one-insertion property that issue 1072 bought. No authored lane
  list exists. A gate that does not exist fails its test, and the harness
  refuses an empty listing.

### D2. A gate's metadata is an attribute on its recipe

| Attribute | Meaning |
| --- | --- |
| `[group('gate')]` | a recipe outside `check` is a gate; inside `check`, every non-private recipe is |
| `[metadata("tier", "build")]` | the build tier (needs a build); the default is `fast` |
| `[metadata("tier", "exempt", "<why>")]` | in no tier, with its reason stated on the recipe |
| `[group('needs:<cap>')]` | a capability the gate requires (D3) |
| `[group('uses:<res>')]` | a shared resource the gate holds (D4) |
| `[group('cpu:<n>')]` | `threads-required = n` |
| `[group('units')]` | the gate answers `NROS_GATE_LIST_UNITS=1` with one unit per line, and runs one under `NROS_GATE_UNIT=<unit>` |

- A recipe that takes parameters cannot be a gate, because gates run with no
  arguments.
- A unit may add its own `needs:` (`<unit>\tneeds:<cap>` from its lister).
- A gate whose lister fails is listed as ONE test, and that test fails. A
  broken lister is a red, never a dropped gate.

### D3. A lane is a capability claim (maintainer)

`.config/gate-lanes.toml` holds `[capabilities]`, each with a description, and
`[lane.<name>]` tables of `{tiers, claims}`. A lane is named for where it
runs: `fast`, `build`, `queue`, `dev`, `runner`, `merge`, `pristine`.

- `NROS_GATE_LANE` is read at LIST time. A test whose tier or `needs:` falls
  outside the lane is listed as IGNORED, with its reason, so nextest reports it
  as not run, by name. It is never run and never counted as a pass. A gate out
  of scope as a whole is not asked for its units, because a lister may need
  what the lane lacks.
- Every test that runs gets `NROS_CHECK_SKIP_STRICT=1` and its own skip ledger
  (`NROS_GATE_SKIP_LEDGER`). A non-empty ledger fails the test with
  `[UNMET PRECONDITION]`, even when the gate exited 0.
- Before any gate runs, a claims preflight
  (`scripts/check/gate-lane-preflight.py`) probes every claimed capability and
  names all the missing ones at once. Its probes are coarse; the precise answer
  stays with the gate, whose skip is red.
- `check-gate-needs` holds four things:
  - a `needs:` names a listed capability;
  - a capability no lane claims is printed on every run, so a gate that runs
    nowhere is visible;
  - a recipe that can skip declares a `needs:`;
  - recipes whose skip reason is computed at run time are a ratchet baseline
    that may only shrink.
- A capability no runner provides (FSP) is listed as running nowhere,
  visibly, rather than claimed.

### D4. Parallelism through nextest, declared by resource

- `check-gate-lists.py --list nextest-config` generates
  `build/gates/nextest-gates.toml` from the attributes on every run, and passes
  it with `--tool-config-file`. It cannot drift from the recipes.
- `uses:<res>` puts every test of the gate, units included, in the tool test
  group `@tool:nros-gates:<res>`. That group's `max-threads` comes from
  `[resources]`. A test is in one group, so a gate uses one resource, and the
  generator refuses two. Resources today:
  - `root-target`, the root workspace's `target/` and cargo's build lock;
  - `scratch-tree`, sized from the host: the smaller of TMPDIR's free space
    divided by the measured unit peak, and the run's CPUs divided by 8.
- Priority comes from the previous run's junit times, slowest first. A missing
  junit gives no priority, and is never an error.
- Interrupt uses nextest's per-test process group. The lane runs under
  `nros_guard_lane`, so stopping a lane stops every gate.
- `writes:` and `disk:` declarations were dropped. A write that collides is a
  resource. Disk is bounded per unit by free space, which a static number
  would only restate badly.

### D5. The lane wrapper owns what nextest cannot

`just check lane <lane>` runs `scripts/build/gate-lane.sh` around
`just check nextest-gates`, because nextest's setup and wrapper scripts are
experimental and its overrides cannot set environment.

- **Before:**
  - refuse a TMPDIR inside a checkout, and bound git discovery (issue 2240);
  - run the skip-ledger self-test;
  - clear the `GITHUB_*` step files;
  - complete the pinned Rust toolchains, as `[INFRA]` (issue 1868);
  - run the claims preflight;
  - set one CPU budget (`NROS_GATE_CPU_SHARE`).
- **Disk:** `just check nextest-gates` itself refuses to start with less than
  `NROS_GATE_MIN_FREE_GIB` (default 30) free, naming the numbers and the trim
  command, rather than dying of ENOSPC partway through.
- **After, on every lane:**
  - no tracked file changed (issue 2186);
  - no untracked, unignored file appeared;
  - write a timing profile from junit.
- `just check fast` and `just check build` are that wrapper, each restricted
  to its own tier. The lane comes from `NROS_GATE_SCOPE` (default `dev`).
  `pre-push` runs `check fast`, and `ci gate` runs both tiers.
- CI shards the build tier with `gate-shard.py` over
  `.config/build-gate-weights.txt`, by measured weight and run by name. The
  shard is not split with `--partition slice`, because a slice deals tests by
  name order (issue 2141).

### D6. Order independence is a check, not a lane

`just check gate-order [fast|build]` runs one tier twice. The generated
priority is spread by name, ascending in one run and descending in the other.
The check reports:

- every test whose verdict differs between the two runs;
- every test that is red in both;
- every test that only one run listed.

It costs two runs, so it is on no lane.

## Alternatives considered

- **Keep `run-gates-parallel.sh` and add scope to it.** Rejected: that would
  be a second implementation of filtering, junit, sharding, process-group
  kills and resource limits. nextest already does all of these for the tests.
- **A runtime skip that exits 0 (the old `nros_check_skip`).** Rejected: a lane
  that skips its way to green answers "nothing ran" with "pass", which is the
  defect issue 1758 removed from tests.
- **Authored lane lists.** Rejected: an authored list is only as complete as
  whoever wrote it. Tier and needs live on the recipe, and both lists derive
  from it.
- **Lane names by event (`push` / `pr` / `gate`).** Superseded by what
  landed. The tier (`fast` / `build`) and the lane (where it runs, and what
  that place provides) are separate axes.
- **Moving platform-module gates into `check` (maintainer: no).** They join
  the listing by `[group('gate')]` and stay where they are.

## Open questions

1. Lane claims as the ONE provisioning list. CI's gate jobs provision from
   the claims (`scripts/ci/provision-gate-lane.sh`, through the
   `provision-gate-lane` action), and the preflight refuses a lane whose job
   provides less. `just setup-worktree` still reads its own list,
   `.config/worktree-provisioning.txt`, so the `dev` lane's claims and a
   worktree's provisioning can disagree.

## Changelog

- 2026-11-15: Draft, recording phase-543 W0-W6 as landed.

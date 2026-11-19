# Phase 584 — `just ci gate` is one nextest run

**Status (2026-11-20). OPEN: design approved by the user 2026-11-20; no code
item started. Owner: nros 142.** Implements
[RFC-0131](../design/0131-gates-are-nextest-tests.md) D2/D3 as amended
2026-11-20 (the `gate` tier and the `ci` lane), and the preconditions of D5.

Each item is one PR, claimed with `just claim phase-584-wN` before it starts.

## Why

`just ci gate` is the lane every contributor runs before a push and the one
the merge queue reproduces. Today it is a bash list, `steps=(…)` in
`just/ci.just`, run by `scripts/ci/run-lane-steps.sh`:

- `check::fast` and `check::build`, each a SEPARATE nextest run over the gate
  harness;
- `test-unit`, a third nextest run, over the root workspace;
- eight steps outside any harness: `check::cli-fresh`,
  `check::cli-noop-build`, `check::launch-resolve-fresh`,
  `check::pristine-checkout`, `check::api-parity`,
  `check::probe-workspace-caps`, `test-lane-contracts` and
  `check::out-of-lane-deselected`.

So the lane has three junits and a bash list, and the order of the list is
the only thing that serialises steps that share the root `target/`. The
steps outside the harness carry tier `exempt`, each with an ordering reason.
None of RFC-0131's machinery reaches them: the lane's claims preflight,
its skip ledger, its junit-driven priority, its `uses:` resources.

## Design

Approved by the user on 2026-11-20 as sent; recorded in RFC-0131 D2 and D3.

- **A tier `gate`** (`[metadata("tier", "gate")]`): a gate that `just ci gate`
  runs and neither `check fast` nor `check build` does. The four `exempt`
  gates whose reason is "a step of `ci gate`" move to it, and `test-unit` and
  `test-lane-contracts` join it through `[group('gate')]`.
- **A lane `ci`**: tiers `fast`, `build` and `gate`, the claims of `dev`, run
  locally by `just ci gate` as ONE nextest invocation with one junit. The
  docs-only path (`scripts/ci/gate-tier.py`) stays `check fast` alone.
- **The freshness checks are preconditions.** `cli-fresh`, `cli-noop-build`
  and `launch-resolve-fresh` must hold before anything else runs, because
  every later gate reads the CLI or the resolver. The lane wrapper
  (`scripts/build/gate-lane.sh`, D5's "before") runs them serially,
  fail-fast, and writes one verdict file each. Each is also listed as a test,
  `precondition::<name>`, whose body only reads its verdict. So each appears
  by name in nextest's list and in the junit, runs once, and cannot race the
  gates that need it. A failed precondition stops the lane before nextest
  starts, as the bash list does today. nextest's setup scripts were not used:
  they are experimental, and D5 already keeps such work in the wrapper.
- **Serialisation by resource.** The gates that share the root `target/` and
  cargo's build lock hold `uses:root-target`, so nextest's test group
  serialises them, where today the order of a bash list does.
- **`pristine-checkout` is a gate, not a lane step.** It runs `check fast` in
  a submodule-free clone. As a `fast` gate it would list itself; in tier
  `gate` the inner `fast` run cannot see it, so it does not recurse.

## Work items

### W1 — The `gate` tier

- **Owner:** nros 142.
- **Status:** OPEN
- `scripts/check/check-gate-lists.py` accepts tier `gate`. The
  `nros-gates` harness lists `gate::<name>` tests. `check fast` and `check
  build` do not select them.
- `api-parity`, `probe-workspace-caps`, `out-of-lane-deselected` and
  `pristine-checkout` change from `exempt` to `gate`. `test-unit` and
  `test-lane-contracts` gain `[group('gate')]` and tier `gate`.
- **Acceptance:** `NROS_GATE_LANE=dev` lists none of the six; a lane whose
  tiers include `gate` lists all six by name.

### W2 — The `ci` lane

- **Owner:** nros 142.
- **Status:** OPEN
- `[lane.ci]` in `.config/gate-lanes.toml`: tiers `fast`, `build`, `gate`; the
  claims of `dev`; `local = "just ci gate"`.
- `just ci gate` runs `just check lane ci` for a full diff, and `check fast`
  for a docs-only one.
- **Acceptance:** one `cargo nextest` invocation and one junit per full run;
  `gate-lane-jobs` accepts the lane.

### W3 — Preconditions

- **Owner:** nros 142.
- **Status:** OPEN
- `gate-lane.sh` runs a lane's preconditions before the claims preflight
  returns, serially and fail-fast, and writes
  `build/gates/preconditions/<name>.verdict`. Which lane runs which
  preconditions is in `.config/gate-lanes.toml` (`preconditions = [...]`).
- The harness lists `precondition::<name>` for each. Its body reads the
  verdict, passes or fails with the recorded output, and never runs the
  recipe.
- **Acceptance:** a stale CLI stops the lane before nextest starts, naming
  `cli-fresh`. On a green run the three are in the junit as passes. Mutation:
  a missing verdict file fails its test instead of passing.

### W4 — Resources

- **Owner:** nros 142.
- **Status:** OPEN
- `uses:root-target` on `test-unit`, `out-of-lane-deselected`, `api-parity`
  and `probe-workspace-caps`, and on whichever build gates already contend
  for the root `target/`, after measuring which do. `pristine-checkout` gets a
  `cpu:` weight from a measured run.
- **Acceptance:** `just check gate-order` over the `ci` lane is clean: no
  verdict differs between the two orders.

### W5 — `test-unit`: one invocation or a merged sibling

- **Owner:** nros 142.
- **Status:** OPEN
- Measure both on one host, with the tree and the load stated:
  - (a) `test-unit` as one gate test, its inner nextest's junit attached as
    the testcase's output;
  - (b) a sibling nextest run whose junit is merged into the lane's.
- Pick by wall clock. Record both numbers here, and the choice.
- **Acceptance:** one junit either way, holding every `test-unit` test by
  name under (b), or the one `gate::test-unit` case under (a).

### W6 — Keep the reach

- **Owner:** nros 142.
- **Status:** OPEN
- `check-default-gates-run-somewhere` and `check-lane-step-duplicates` read
  `steps=(…)` from `just/ci.just` today. They move to reading `[lane.ci]`
  and its preconditions, so a gate that leaves the lane (its tier changed or
  its recipe deleted) is still caught.
- **Acceptance:** negative controls: move a `gate` recipe back to `exempt`,
  and drop a precondition from the lane; each makes its check red.

## Phase acceptance

- A full `just ci gate` on a clean tree is green.
- It writes ONE junit.
- Its wall clock is equal to or better than the bash list's on the same host.
  Baseline: the full gates run on host 142 on 2026-11-19 and 2026-11-20,
  between 37 and 44 minutes, `check::build` taking 24 to 36 of them.
- Every former step appears by name in `nextest list` for the lane, checked
  by a test, not by reading.

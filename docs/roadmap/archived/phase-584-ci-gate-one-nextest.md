# Phase 584 — `just ci gate` is one nextest run

**Status (2026-11-27). COMPLETE, archived: every work item DONE.
Owner: nros 142.** Design approved by the user 2026-11-20. The phase
acceptance, measured on host 142:
- a full `just ci gate` on a clean tree is green: 763/763 (W5's a2 and a3);
- it writes one junit: the lane is one nextest run (W2), `test-unit` in it (W5);
- its wall clock beats the bash list's: 1355–1393 s warm, against the 37–44
  minute baseline below; 2874 s on a cold tree;
- every former step appears by name in the lane's listing, checked by a test:
  `check-lane-step-duplicates`' reach rule (W6). Implements
[RFC-0131](../../design/0131-gates-are-nextest-tests.md) D2/D3 as amended
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
- **Status:** DONE (2026-11-19, #4406). Acceptance re-checked 2026-11-26 on
  main `ee6dfe8ff4`: `check-gate-lists.py --lane-steps dev` names none of the
  six, and `--lane-steps ci` names all six (`check::api-parity`,
  `check::out-of-lane-deselected`, `check::pristine-checkout`,
  `check::probe-workspace-caps`, `test-lane-contracts`, `test-unit`). A full
  `just ci gate` on host 142 (2026-11-25, #4854's head, 762/762) ran each as a
  `gate::` test and passed it.
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
- **Status:** DONE (2026-11-21, the W2 PR). `_gate-inner` runs `just check lane ci` for a full diff;
  the `steps=(…)` array is gone. `check-gate-lists.py --lane-steps <lane>`
  lists the `just` addresses a lane runs, from the harness's own listing, and
  the two checkers that walked the array read it instead (see W6).
- `[lane.ci]` in `.config/gate-lanes.toml`: tiers `fast`, `build`, `gate`; the
  claims of `dev`; `local = "just ci gate"`.
- `just ci gate` runs `just check lane ci` for a full diff, and `check fast`
  for a docs-only one.
- **Acceptance:** one `cargo nextest` invocation and one junit per full run;
  `gate-lane-jobs` accepts the lane.

### W3 — Preconditions

- **Owner:** nros 142.
- **Status:** DONE (2026-11-20, #4437). Acceptance re-checked 2026-11-26:
  - a stale CLI stops the lane before nextest starts, naming `cli-fresh`:
    with an uncommitted edit to `packages/cli/nros-cli/src/main.rs`, `just ci
    gate` printed `precondition \`check::cli-fresh\` FAILED (rc=1); no gate
    ran.` after `source-stamp: STALE`, and exited 1;
  - on a green run the three are in the junit as passes: host 142's full
    `just ci gate` at #4854's head (2026-11-25) ran them in the wrapper in
    order and reported `precondition::check::{cli-fresh, cli-noop-build,
    launch-resolve-fresh}` as PASS, in the same nextest run as the 759 others;
  - mutation: the harness's own `self::precondition-verdicts` test holds that
    a missing directory or verdict file is red, not a pass.
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
- **Status:** DONE (2026-11-22, #4647). Its acceptance was measured in that
  PR on host 142: a full `ci gate` in gate-order mode passed both name orders,
  728/728, wall 3489 s.
- `uses:root-target` on `test-unit`, `out-of-lane-deselected`, `api-parity`
  and `probe-workspace-caps`, and on whichever build gates already contend
  for the root `target/`, after measuring which do. `pristine-checkout` gets a
  `cpu:` weight from a measured run.
- **Acceptance:** `just check gate-order` over the `ci` lane is clean: no
  verdict differs between the two orders.
- **Measured (2026-11-22, host 142, 80 CPUs, a warm worktree).** Each
  candidate ran alone under `strace -f -P` on the root `target/`'s
  `.cargo-lock` files and `.rustc_info.json`; `check::node-std-tests`, already
  tagged, was the positive control (25 opens of `target/debug/.cargo-lock`).
  - `test-unit`, `test-lane-contracts` and `out-of-lane-deselected` each open
    `target/nros-relwithdebinfo/.cargo-lock`: they take `uses:root-target`.
    `test-lane-contracts` was not on the list above, and is.
  - `api-parity` and `probe-workspace-caps` open neither. `api-parity` builds
    in its own temp `CARGO_TARGET_DIR` and `probe-workspace-caps` in its
    shared cargo root, so they stay untagged; tagging them would only take
    two of `root-target`'s slots from gates that need them.
  - The build gates that contend are the seven already tagged since
    phase-543; none was added.
  - `pristine-checkout`: ~2,400 CPU-s per run whatever the parallelism, and
    wall time against `NROS_GATE_JOBS` of 200 s at 80, 228 s at 32, 282 s at
    16, 396 s at 8 and 625 s at 4. The knee is 16: `[group('cpu:16')]`, its
    basis stated beside it.
- `just check gate-order` takes `gate` and `all` as well as `fast` and
  `build`; `NROS_GATE_SCOPE=ci just check gate-order all` runs the `ci` lane
  twice, in `just ci gate`'s own shape.

### W5 — `test-unit`: one invocation or a merged sibling

- **Owner:** nros 142.
- **Status:** DONE (2026-11-26): (a), unchanged. Measured on host 142
  (80 CPUs, Linux aarch64) on main `ee6dfe8ff4` plus this phase's docs
  commits, each run under the host gate lock in one worktree, order
  a, b, a, then b, a, load1 (1/5/15 min) recorded at each end:

  | run | shape | wall | `test-unit` | load at end |
  | --- | --- | --- | --- | --- |
  | a1 | (a), cold tree | 2874 s | 144 s in the lane | 16 / 23 / 35 |
  | b1 | (b) | 1325 s | 15 s beside it | 27 / 36 / 43 |
  | a2 | (a) | 1393 s | 83 s in the lane | 21 / 32 / 42 |
  | b2 | (b) | 1406 s | 15 s beside it | 28 / 36 / 46 |
  | a3 | (a) | 1355 s | in the lane | 29 / 37 / 47 |

  (b) is `gate-lane.sh ci -E 'not test(=gate::test-unit)'` with `just
  test-unit` started beside it, timed until both finish. a1 is the first run
  in the tree and compiles from cold, so it is not compared. Warm, (a) is
  1393 and 1355 s (mean 1374 s) and (b) 1325 and 1406 s (mean 1366 s): 8 s
  apart, against 81 s between the two (b) runs. Wall clock does not separate
  them. (a) is chosen because it is what the lane already runs: `test-unit`
  stays inside the lane's claims preflight, skip ledger, one junit and
  `uses:root-target`, and (b) would add a junit merge for no measured gain.
- Measure both on one host, with the tree and the load stated:
  - (a) `test-unit` as one gate test, its inner nextest's junit attached as
    the testcase's output;
  - (b) a sibling nextest run whose junit is merged into the lane's.
- Pick by wall clock. Record both numbers here, and the choice.
- **Acceptance:** one junit either way, holding every `test-unit` test by
  name under (b), or the one `gate::test-unit` case under (a).

### W6 — Keep the reach

- **Owner:** nros 142.
- **Status:** DONE (2026-11-26). `check-lane-step-duplicates` reads the `ci`
  lane's own listing (`check-gate-lists.py`'s `test_names`) and holds two
  rules over it: no `just` target is run by two of the lane's tests, and every
  step the bash list ran outside the two whole tiers (`FORMER_CI_GATE_STEPS`,
  the nine named in "Why") is still run by the lane, with `[lane.ci]` keeping
  tiers `fast`, `build` and `gate`.
- **Negative controls, measured on main `ee6dfe8ff4`:** before this item both
  were GREEN everywhere — `api-parity` moved back to `exempt`, and
  `launch-resolve-fresh` dropped from `[lane.ci]`'s preconditions, each left
  `check-default-gates-run-somewhere`, `check-lane-step-duplicates` and
  `check-gate-lists.py` at rc 0. With this item each is red, naming the step
  that left. Both are also in the script's self-test, with a duplicate and a
  dropped tier.
- **Reach, as a diffed list.** The bash list ran the `dev` scope's in-scope
  `fast::` and `build::` tests plus its nine named steps; the `ci` lane runs
  `check-gate-lists.py --lane-steps ci`. Computed at the commit before W2
  (`8c0e789787`) and at W2 (`56fdc8ce15`), sorted with `LC_ALL=C`: 671
  targets each, `diff` empty, identical sha256 (`44ded4fc97013e6f…`). The 11
  gates the raw tier lists hold beyond that (`artifact-identity-budget`,
  `hosted-cxx-opt-in`, `tiarmclang-image`, …) need a claim neither scope has
  (`build-tree`, `arm-gcc`, `zephyr`, `fsp`, `tiarmclang`), and were
  deselected before as they are now.
- `check-default-gates-run-somewhere` and `check-lane-step-duplicates` read
  `steps=(…)` from `just/ci.just` today. They move to reading `[lane.ci]`
  and its preconditions, so a gate that leaves the lane (its tier changed or
  its recipe deleted) is still caught.
- **Acceptance:** negative controls: move a `gate` recipe back to `exempt`,
  and drop a precondition from the lane; each makes its check red.
- **Done in W2, because removing the array broke them:**
  `check-default-gates-run-somewhere` (R4) and `check-lane-contracts`' walk of
  `ci::gate` read `--lane-steps ci`. Measured: with that listing emptied, R4's
  self-test is red ("the `just ci gate` step list did not parse") and
  `check-lane-contracts` is red ("examined 0 test target(s)"). Left for W6:
  `check-lane-step-duplicates` on `[lane.ci]`, and the two negative controls.

## Phase acceptance

- A full `just ci gate` on a clean tree is green.
- It writes ONE junit.
- Its wall clock is equal to or better than the bash list's on the same host.
  Baseline: the full gates run on host 142 on 2026-11-19 and 2026-11-20,
  between 37 and 44 minutes, `check::build` taking 24 to 36 of them.
- Every former step appears by name in `nextest list` for the lane, checked
  by a test, not by reading.

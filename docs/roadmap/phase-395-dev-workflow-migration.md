# Phase 395 — migrating to the multi-agent dev workflow

**Status (2026-08-28). Plan, nothing landed.** The design and its measurements
live in [multi-agent-ci-workflow.md](../development/multi-agent-ci-workflow.md);
this is the ordered work to get there. Each wave is independently landable and
each one pays for the next. Waves 0–3 need no runner, no queue and no new
service — they are worth doing even if the rest is never adopted.

## Why, in one paragraph

Ten-plus agents each run a 40–90 minute local verification before pushing, they
duplicate each other's work, and the GitHub CI meant to catch the rest takes
hours because a 9.2 GB Zephyr SDK cannot fit in a 10 GB `actions/cache` on a
14 GB runner. Meanwhile `just ci` runs `NROS_TEST_SCOPE=native` and never
exercises the ~50 runtime cells that are host-executable and free.

## W0 — Measure before changing anything

Half a day, no code. Two numbers decide the rest of the sequence.

1. **What fraction of the tests in `just ci` actually need a fixture?**
   **Measured 2026-08-28: 89 of 163 test files (55%) call a fixture resolver;
   74 (45%) do not.**

   Method matters here — a first pass grepping for `build_*` returned 27%, and
   was wrong: `qemu_baremetal_main_e2e_binary` is a resolver too. The figure
   above is against the authoritative list of all 377 `pub fn` resolvers in
   `fixtures/binaries/`. Same mistake class as guessing a pattern instead of
   reading the source of truth; recorded so the next person does not repeat it.

   Caveat: file-level, not test-level. `rstest` cases are not statically
   countable, so a file needing one fixture counts the same as one needing ten.

   *Consequence for this plan:* 45% of test files are gated behind a
   precondition they do not need, so W2 is a real and immediate win — but it is
   not "most", so **W10 cannot be deferred as far as first written**. Roughly
   half the suite genuinely needs built artifacts, which keeps the fixture cache
   load-bearing rather than an optimisation.
2. **Wall-clock split of one PR run.** **Measured 2026-08-28, and it inverts
   the premise of this plan.** Nothing on GitHub takes hours:

   | workflow | median | state |
   | --- | ---: | --- |
   | `pr-checks` | **2.5 min** | **14 of 14 runs failing** |
   | `host-tests` | 3 min | failing |
   | `nightly` | 30–34 min | failing |
   | `images` | 8–13 min | succeeding |

   Every signal-bearing workflow is RED, and has been for days. Causes so far:

   - `pr-checks` — `check-subtree-guard` counting a recycled PGID rather than
     its own processes. Fixed; it failed only on the runner because 4 vCPUs
     running `check-fast` 32-way parallel churn PIDs hard enough for reuse.
   - `host-tests` — `ERROR: cannot locate rmw_zenoh_cpp/rmw_zenohd`. The CI
     container has no ROS zenoh router, so RFC-0075's resolution finds nothing.
     A provisioning gap in the image, not a code defect.
   - `nightly` — qemu and freertos jobs fail at ~30 min. Root cause **not
     pinned**; the log tail is teardown noise.

   **So the hours are not on GitHub — they are LOCAL.** Agents run the 40–90
   minute treadmill precisely because CI is red and tells them nothing, and a
   permanently-red required check is indistinguishable from no check at all.
   That is a cycle, and it is cheap to break.

**This re-orders the plan.** The bottleneck is not runner capacity or queue
latency; it is that CI carries no signal. Fixing the reds and keeping them fixed
comes before runners (W6), before the queue (W7), and before the cache (W10) —
because none of those help while every result is ignored. W5 (flake quarantine)
is effectively already underway: the first flake was found and fixed during W0.

Everything below assumed the hours were on GitHub. Re-read it with that
corrected.

## W0.5 — Make CI carry signal again

Inserted after W0.2 measured that every signal-bearing workflow is red. Nothing
downstream matters while results are ignored, and this is the cheapest wave in
the plan.

All six red jobs are now root-caused and fixed. **Not one was a code
regression** — every single one was wiring, provisioning, or a gate that could
not see its own rule. That is the finding, more than the individual fixes.

| job | defect |
| --- | --- |
| `pr-checks` | `check-subtree-guard` counted a RECYCLED PGID, not its own PIDs. Passed locally and in a quiet container; failed only where 4 vCPUs run gates 32-way parallel. |
| `host-tests` | `just zenohd setup` built the VENDORED router, deleted by RFC-0075 / phase-362. `zenohd` takes a LOCATOR, so `setup` was one — the step could never work. |
| nightly `qemu` | ran `test-wcet`, which deliberately refuses under emulation (no DWT counter). Red by construction. Also silently narrowed: `build-all`/`build-examples` did not exist for this module alone, so the `\|\|` chain fell through to the lightest build. |
| nightly `nuttx`, `threadx_linux` | a nextest `-E` filter cannot survive just's UNQUOTED variadic interpolation — `args=(-E test(Nuttx))` is a bash syntax error. The only two callers with parens; exactly the two failures. |
| nightly `freertos` | the platform job never installed cross targets; `armv8r-none-eabihf` (s32z270, Cortex-R52) was absent, so cmake configure died. |

Three of these were invisible to gates that exist for the class:
`check-just-recipe-refs` read `just/*.just` and never `.github/workflows/`
(widened — but it still cannot see `just ${{ matrix.plat }} build-all`, because
it skips any line containing `{{`); and the recycled-identity lesson was already
written down as `group_ledger::start_time()` but never applied to PGIDs.

**Then keep it green**: a red required check that persists for days is the
   condition this wave exists to prevent, and issue 0840's `pre-push`
   `check-fast` hook is the complementary half — it stops reds being *created*,
   this stops them being *tolerated*.

Only after CI is trustworthy does it make sense to spend on runners, queues or
caches — those reduce the cost of a signal nobody currently reads.

## W1 — Generate the issue index

**Blocker for batching, not a cleanup.** `docs/issues/README.md` is a
hand-maintained registry of files that already carry frontmatter. A merge queue
needs its batch to merge cleanly, so three agents filing issues concurrently
produce a textual conflict that stops the batch from *forming*. With ten
doc-heavy agents that is the common case.

- `scripts/gen-issue-index.py` emitting the open list from each file's
  frontmatter, in the established generated-page style (pool inventory, RMW
  matrix, support status).
- `check-issue-index` becomes a drift check against the generated output rather
  than a hand-comparison. It currently compares distinct ids to files, which is
  why `main` has carried **two open `#0824` rows** while reporting OK.
- The "Recently resolved" prose stays hand-written; only the open list is
  generated.

## W2 — Lane recipes, and redefine the local verb

The largest coverage gain available, and it needs no infrastructure.

- Define `just ci-l1` (affected crates), `just ci-l2` (the four
  host-executable platforms), `just ci-l3` (cross build + link + symbol),
  `just ci-l4-tier1`.
- **Redefine `just ci` as L0+L1+L2** and keep `just ci-full` for the
  pre-release sweep.
- **Update CLAUDE.md and AGENTS.md in the same commit.** CLAUDE.md currently
  instructs every agent to "run the TIER your change earns" and names
  `ci-matrix` / `ci-full`. Agents follow that faithfully — one session ran the
  full treadmill four times. If the verbs change without the instructions,
  agents keep paying the old cost and the queue re-tests what they already ran.
- Ship a minimal `CONTRIBUTING.md` here: run `just ci`, expect L0–L2, do not
  attempt the cross lanes.

`ci-l2` widens coverage from `native` to `Linux` + `ZephyrNativeSim` +
`ThreadxLinux` + `FreertosPosix`. `ZephyrNativeSim` builds with
`ZEPHYR_TOOLCHAIN_VARIANT=host`, so this needs no SDK.

## W3 — Make L3 a real lane

Today `rust-rtos-link-check` is the only cross-target gate. Two more witnesses
landed in phase 392 and are wired into nothing:

- `scripts/nros-mem-report.py --check` over cross ELFs — turns the static-RAM
  campaign into a gate instead of a manual measurement.
- `scripts/check-no-alloc-image.py` — the link-time allocation gate.

Together these catch 32-bit layout, linker/section, static-RAM and
allocation-freedom defects **without QEMU**. That is most of this project's
recurring embedded classes, at build cost.

Also here: the paths→coordinates map that makes lane selection a function of the
change rather than a fixed set. `row_coord()`, `NROS_TEST_COORDS` and
`CiLane::run_scope` already exist; the map does not.

## W4 — Hosted workflow files

`pr.yml` running L0/L1/L2 on `ubuntu-22.04`, replacing the PR half of
`pr-checks.yml`. Thin `just` callers per
[ci-workflow-reorg.md](../development/ci-workflow-reorg.md); fresh-clone
assumptions per [ci-conventions.md](../development/ci-conventions.md).

Path filters so an express change (docs, an index row) runs L0 and nothing else.
`pr-checks.yml` already has a `changes` job to model this on.

## W5 — Flake quarantine

**Must precede the merge queue.** A batch red is ambiguous between a defect and
a flake, and bisection multiplies the cost — one flake ejects and re-tests four
innocent PRs.

- A registry of quarantined tests: they still run and still record, they do not
  block.
- Automatic solo-retry before a batch red is believed.
- First entry: `action_raw_goal_ships_one_cdr_header` — 60 s timeout in-sweep,
  3.6 s solo, 5/5.

`_nextest-tolerant` and the skip budget are most of the mechanism already; what
is missing is the registry and the does-not-block half.

## W6 — Runner scripts, and one runner

- `scripts/ci/runner-register.sh` — registration token via `gh api`,
  `config.sh --ephemeral --labels`, service install.
- `scripts/ci/runner-provision.sh` — make the labels true, reusing `nros setup`
  so a runner and a contributor provision identically.
- `scripts/ci/runner-doctor.sh` — refuse to register a runner that lies about
  its labels. A runner labelled `nros-sdk-zephyr` without the SDK produces a red
  that looks like a code failure.
- `scripts/ci/runner-sweep.sh` — reap orphaned process groups between jobs and
  own disk GC. One session found **71 orphaned `add_two_ints_server`**, oldest
  10 days; on a shared runner one leaked peer is every later job's flake.

Register one machine with `nros-qemu,nros-sdk-zephyr,nros-big`. Outbound 443
only, so NAT is fine. Ephemeral and unprivileged, because this is a public repo.

## W7 — The queue

- `queue.yml` on `merge_group`, self-hosted, running `ci-l3`, uploading logs
  unconditionally (a NAT'd runner cannot be reached).
- `post-submit.yml` on `push` to `main` running `ci-l4-tier1`.
- Branch protection with `strict: false` and required checks naming the
  **hosted** lanes only.
- Merge queue on: rebase, batch 4, min 1, 5 min wait, timeout above L3's p99.
- Partition by path so docs never queue behind a Cyclone build.

## W8 — Claims

`just claim` / `claim-renew` / `claim-release`, modelled on
`scripts/reserve-issue-id.sh` — arbitration at origin, unique object per
attempt, push-rejection as the CAS.

Beyond id reservation: a TTL in the object, renewal driven by **liveness**
rather than by the agent remembering, and stealing an expired claim with
`--force-with-lease=<ref>:<oid>`. An open PR supersedes the claim, so the TTL
governs only the window before first push — hours, not days.

Includes the predecessor check (a wave cannot be claimed while its predecessor
is claimed-and-unlanded) and the rule that agents check **both** claim refs and
GitHub issue assignees, because outside contributors cannot write refs.

## W9 — The contributor path

- Full `CONTRIBUTING.md` with the six-stage flow.
- `.github/PULL_REQUEST_TEMPLATE.md` asking which lanes ran, what could not be
  verified, and whether the work was AI-assisted — the last for review
  emphasis, not as a filter.
- `just doctor` gains a one-line answer to "what can I run?" for someone with a
  fresh clone.
- Require approval for outside collaborators; document that **enqueueing a fork
  PR is the trust decision**, because that is when its code reaches
  self-hosted hardware.

## W10 — Content-addressed fixture cache

Key on (input hash, coordinate) using `row_coord()` and the `hashFiles` pattern
`nightly.yml` already uses for `packages/cli/target`. Shared on the runner's
persistent disk.

This is the term multiplied by N agents, and it also dissolves the treadmill
class: a rebase that does not change a fixture's inputs stops invalidating it,
and museum binaries become impossible because the key *is* the input hash.

## Not in scope

- Replacing GitHub's merge queue with a third-party tool. The native one has
  batching and ejection; partitioning is `paths:` filters. Revisit only if
  measurement shows the queue is the bottleneck.
- Remote *execution* (as opposed to caching). Much larger, and the cache is
  where the measured win is.
- Auto-revert automation before W7 has run long enough to trust culprit
  attribution.

## Exit criteria

Stated so this is falsifiable rather than a one-way door:

- If median time-to-land does not fall below today's local-verification time,
  the queue is not paying for itself — go back to direct pushes.
- If batch reds are more often flakes than defects, W5 failed; stop batching.
- If the self-hosted runner becomes a single point of failure for landing,
  move L3 back to hosted and accept a narrower L3.

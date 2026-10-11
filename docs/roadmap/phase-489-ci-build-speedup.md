# Phase 489 — CI build speed-up: the required gate first, then the build-bound lanes

**Status (2026-10-12). W1 + W3 landed, acceptance pending.** W1 and W3
shipped together; their measured acceptance waits for post-merge runs (see
each item). W2, W4–W7 not started. The numbers below were
measured from the GitHub Actions API over 2026-10-09 to 2026-10-10. They are
the baseline every work item is judged against. Implements no new RFC
decision. It continues the cost work of
[phase-353](archived/phase-353-build-and-fixture-cost.md),
[phase-367](archived/phase-367-build-and-sync-process-cost.md) and
[phase-410](archived/phase-410-ci-breadth-depth-restructure.md), and it sits
beside issue 1353 (hosted runners running out of disk), which several items
here relieve as a side effect.

## Why

Merge throughput is set by ONE job. `CI` from `gate.yml` is the only required
context, and every merge-queue batch waits on its `check` job. That job runs
on a GitHub-hosted `ubuntu-22.04` runner, and it runs its gates one after
another.

| Workflow (event) | Runs | Median wall | Max |
| --- | --- | --- | --- |
| `gate` (merge_group) | 44 | **49.3 min** | 54.0 |
| `gate` (pull_request) | 67 | **42.9 min** | 81.8 |
| `queue` (merge_group, self-hosted L3) | 17 | 24.8 min | 139.4 |
| `gate` (push) | 10 | 22.8 min | 39.5 |
| `post-submit` (push) | 25 | 9.7 min | 18.2 |
| `live-peer regression` (schedule) | 1 | 106.4 min | |
| `gate` (schedule, compile tier) | 1 | 91.3 min | |
| `run-matrix` (schedule) | 1 | 88.5 min | |
| `host-tests` (schedule) | 1 | 88.1 min | |
| `nightly` (schedule) | 2 | 53.9 min | 67.7 |

Two measurements settle where the time goes:

- **Queue wait is zero.** For merge_group and pull_request runs, the gate's
  `check` job starts within 0.1 min of being created. It then runs for
  33–52 min. All of the latency is the job's own run time, so more hosted
  capacity buys nothing.
- **Self-hosted capacity is now idle.** Since three runners came up
  (`newslab-118-server`, `newslab-133`, `newslab-server-241`) the self-hosted
  jobs wait 0 min. Before that, L3 waited 6–21 min. Most of the time one or
  none of the three is busy.

## Where the time goes

### The required path: `gate` / `check` (merge_group, 4 recent runs)

| Step | Median | Max |
| --- | --- | --- |
| `just check fast` | 10.0 min | 15.0 |
| `just test-lane-contracts` | 5.6 | 8.1 |
| `just check workspace-all` (host + embedded clippy) | 5.3 | 7.7 |
| `just check cli-tests` | 4.8 | 5.7 |
| Cache CLI build (restore-only) | **3.8** | 3.9 |
| `just test-unit` | 3.4 | 4.6 |
| `just check compile-smoke` (pull_request only) | 4.3 | 4.8 |
| everything under 2 min (setup, container init, the CLI build) | about 7 | |

Inside `just check fast`, the slowest gates on one batch were
`knob-single-reader` **270 s**, `probe-workspace-caps` **257 s** and
`api-parity` **167 s**. On another run `api-parity` took 190 s. The fast
line runs its gates in parallel, so these three set its floor.

### The build-bound lanes

| Lane | Dominant step | Time |
| --- | --- | --- |
| live-peer regression, board-IS-runner job | Build the fixtures those rows resolve | **84.5 min** (the cells run in 10.4) |
| live-peer regression, board-NOT-runner job | Build the fixtures those rows resolve | 33.7 |
| gate (schedule) | `just check build` | **44.4** (also where issue 1353's disk runs out) |
| gate (schedule) | generate message bindings | **21.7** |
| host-tests (schedule) | `just ci tier1 gates` | **56.9** |
| host-tests (schedule) | generate message bindings | **18.7** |
| run-matrix (schedule) | `just build tier1` / `just build tier2` | 31.3 / 27.8 |
| nightly | Build (nuttx) / Build (freertos) / `just build tier2-nightly` / Build (threadx_linux) | 46.5 / 36.2 / 28.9 / 22.7 |

### A slow runner

`newslab-server-241` ran L3 in 16, 17, 31 and **58** min, against 10–12 min
on `newslab-118-server` and `newslab-133`. The 139-min `queue` maximum above
is that runner.

## Work items

Ordered by expected payoff on the merge rate.

### W1 — split the required `check` job into parallel jobs

The seven steps in the table above are independent: fast, lane-contracts,
workspace-all, cli-tests, test-unit and compile-smoke. Run them as sibling
jobs that the `CI` aggregator waits on, so the critical path is the longest
one (`check fast`, about 10–15 min) plus setup, not their sum (about 33 min).

The aggregator stays the ONE required context (CLAUDE.md: never add a job
name to the required set). Each sibling restores the same CLI cache, which
W3 makes cheap.

**Acceptance:** median `gate` (merge_group) wall time under 25 min over 20
batches, with the same gates run. `check-lane-contracts` and
`check-default-gates-run-somewhere` must still prove every gate runs.

**Landed (2026-10-12).** `check` is now seven siblings — `check-fast`,
`check-compile-smoke` (PR), `check-cli-tests`, `check-workspace-all`,
`check-test-unit` and `check-lane-contracts` (merge group + nightly), and
`check-build` (the nightly-only tier, which needed a home too). Each runs the
gate steps it ran before under the same step `if:`, after the same setup, on
the same hosted runner. `CI` `needs:` all of them and excuses a skip only on an
event the sibling's own `if:` excludes. `check-default-gates-run-somewhere`
gained R3 (every gate job is in `CI`'s `needs:`, inspected, with an excuse list
equal to its `if:`), and `check-lane-contracts` now examines 32 merge-gating
lane invocations where it examined 3 — the single job's nightly
`generate-bindings` had exempted all of it. The 25-min median waits for 20
post-merge batches.

### W2 — run the gate's heavy siblings on the self-hosted runners

Three self-hosted runners are idle most of the time, and they keep warm
`target/` directories that a hosted runner never has. Route the cargo-heavy
siblings from W1 (`workspace-all`, `cli-tests`, `test-unit`) to
`[self-hosted, linux, nros-big]`. Keep a hosted job for anything that must
prove a clean machine.

Watch the 0196 trap. A gate a self-hosted runner can satisfy only from
leftover workspace state (issue 1476's shape) is a false pass. Each moved
job must start from a clean checkout of its sources, keeping only the cargo
and sccache caches.

**Acceptance:** the moved siblings' median at least 40 % below their hosted
median, and no new "passes only on a warm runner" red when one runner is
re-provisioned.

### W3 — make the CLI cache cost seconds, not 3.8 minutes

"Cache CLI build" restores `packages/cli/target` and
`nros-launch-resolve/target` on every event, in a constant 3.8 min. That is
a large tarball for one `nros` binary. Measure what the job actually needs.
If it is the binary plus the fingerprint for `setup-cli`'s freshness assert
(phase-429 W5), cache that alone, under a key that covers `source_stamp.rs`'s
full watch set. That closes the "key narrower than the stamp" gap the step's
own comment names.

**Acceptance:** restore under 30 s, and the phase-429 W5 freshness assert
still refuses a stale binary.

**Landed (2026-10-12).** The `check-*` siblings restore four files —
`nros`, `nros-launch-resolve`, its content stamp and its Python half (41 MB,
12.6 MB zstd) — under `nros-cli-bin-<os>-<sha256>` from
`scripts/ci/cli-bin-cache-key.sh`: the git index entries of `packages/cli`
plus every `cli-source-dirs.txt` dir (gitlinks, so the `play_launch` pin, are
in it) plus `rust-toolchain.toml`, a superset of the stamp. Main's old entry
was 2967 MB. The cache only pre-populates: every sibling still runs
`just setup-cli` (and `just setup-launch-resolve` where its tests need the
resolver), whose own freshness check skips or builds, then asserts
`source-stamp`. Measured locally: a
restored binary skips the build and `setup-launch-resolve` skips; a `.rs` or
`.jinja` edit makes `source-stamp` exit 1 and moves the key. `check-cli-tests`
alone still restores the full target tree for its test targets. The 30-s
restore figure waits for post-merge runs.

### W4 — stop regenerating message bindings from cold (19–22 min)

`just generate-bindings` costs 21.7 min in the scheduled `gate` and 18.7 min
in `host-tests`, every run. Before changing anything, find out which part is
codegen and which part is the cargo build of the generated crates. Then cache
the output keyed on its real inputs: the interface packages, the CLI's
codegen version (`codegen_version.rs`) and the ament set. Never key it on
time.

**Acceptance:** under 3 min on a cache hit. On a deliberate codegen change,
the cache misses and the fixtures rebuild (issue 1360's codegen-version gate
must still fire).

### W5 — shorten the slowest fast-line gates

`knob-single-reader` (270 s), `probe-workspace-caps` (257 s) and
`api-parity` (167–190 s) set the fast line's floor, and the fast line also
runs in the `pre-push` hook, so every local push pays it too. Profile each:
a cold cargo build inside a gate, or a re-scan that could read a cached
index, is the usual cause. Fix only what the profile shows.

**Acceptance:** `just check fast` under 6 min wall on CI, and the `pre-push`
hook proportionally faster.

### W6 — a persistent build cache for the fixture lanes

Live-peer (84.5 + 33.7 min of fixture build), run-matrix (about 30 min a
tier) and the nightly ports (23–47 min each) are build-bound, and they run on
self-hosted runners that could keep warm build trees. Measure what the
fixture build re-does on a second run with no source change; by the
"rm -rf is an antipattern" rule, anything it rebuilds there is a missing
edge or a cache miss. Then decide whether a per-runner persistent
`target`/`build` directory is safe for each lane, given issue 0616 (one
target dir per workspace root) and the treadmill rules for fixture mtimes.

**Acceptance:** a no-change re-run of each lane's fixture build is under
10 % of its cold time, with `_require-fixtures` freshness unchanged.

### W7 — find out why `newslab-server-241` is 2–5× slower

Compare `nproc`, the jobserver budget, disk type and free space, and other
tenants (CLAUDE.md: `/home` is shared by 14 users) across the three runners.
Until it is fixed, consider a label that keeps the critical-path L3 job off
it.

**Acceptance:** `newslab-server-241`'s L3 median within 25 % of the other
two runners'.

## Measuring

Rerun the measurement that produced this document after each work item: the
median per workflow and event over 20 completed, non-cancelled runs, plus the
per-step medians of the slowest jobs. It reads only the Actions API. Report
every number against the baseline tables above, and do not count a cancelled
run as a fast one.

## Not in scope

- Lane membership. What each tier runs is phase-410's decision and RFC-0061's.
  This phase makes the same work cheaper, not smaller.
- Disk exhaustion as a correctness problem (issue 1353). W1, W2 and W6 should
  relieve it, but closing it is that issue's job.

# Phase 489 — CI build speed-up: the required gate first, then the build-bound lanes

**Status (2026-10-13). W1 and W3 met their acceptance; W4 met it on a hit and
cut the miss path by half; W5 landed, CI acceptance NOT met; W2 landed and is
switched OFF again after a three-hour trial starved the merge queue; W6
landed in part, acceptance not met; W7 is handed to the runner operators.**
Each item's own section has the post-merge numbers. The numbers below were measured from the GitHub Actions API over 2026-10-09 to 2026-10-10. They are
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
`generate-bindings` had exempted all of it.

**Acceptance met (measured 2026-10-12).** The 40 successful `gate`
merge-group runs from 05:02Z to 21:44Z, all after #2006 merged (00:12Z), have
a median of **15.2 min** (11.1–21.7) against the 25-min target and the
49.3-min baseline. Runs from the W2 trial below are excluded: their wall time
is runner queueing, not the gate.

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

**Landed (2026-10-12), switched OFF.** `check-workspace-all` and
`check-test-unit` carry an event-routed `runs-on`: `[self-hosted, linux,
nros-sdk-zephyr, nros-big]` (queue L3's label set — the embedded clippy half
needs `arm-none-eabi-gcc`) on `merge_group` / `workflow_dispatch` when
`vars.NROS_GATE_SELF_HOSTED == 'true'`, and the old hosted job in the CI image
otherwise. The container image is routed by the same condition: an EMPTY image
means no container, which the unprivileged runner container needs (measured on
a probe run, 38218504918: empty image ON-HOST, set image IN-CONTAINER, both
with the credentials block present). The variable is unset, so nothing moves
until an operator sets it after a green `workflow_dispatch` run on the pool.

Hosted baseline, first day after W1 (successful jobs, 2026-10-12, Actions API):

| Sibling | merge_group median (n) | pull_request median (n) |
| --- | --- | --- |
| `check-workspace-all` | 13.9 min (21) | 12.9 (20) |
| `check-test-unit` | 9.4 (21) | — |
| `check-cli-tests` | 14.6 (21) | 13.4 (20) |
| `check-fast` (stays hosted) | 17.9 (21) | 20.2 (18) |

What was decided, and why:

- **Merge group only, not pull requests.** The merge-group run sets merge
  throughput, so it is where a faster sibling pays. PR load was about twice the
  merge-group load that day (41 vs 21 runs), and three runners also carry queue
  L3 and the nightly lanes; routing both events would saturate the pool.
- **No pull request reaches the pool, so no fork PR does.** The repo is
  public and asks approval only of FIRST-TIME contributors.
  `check-workflow-runner-isolation` now parses the route
  (`scripts/lib/runner_route.py`) and refuses one whose condition admits
  `pull_request` / `pull_request_target`, is widened by `||`, or has a
  self-hosted fallback; any other expression stays fail closed.
  `check-workflow-just-provisioning` reads the hosted arm's image and refuses a
  container condition that differs from the `runs-on` one.
- **Its own interlock, default off.** While this was written, two of the three
  runners (`newslab-118-server`, `newslab-133`) showed online and idle and took
  no job for hours — every L3 that day ran on `newslab-server-241`, and nightly
  and run-matrix sat queued. A required-path job routed to that pool waits
  rather than fails, so `NROS_SELF_HOSTED_READY` (on, for L3, which nothing
  requires) was not reused.
- **`check-cli-tests` was held back, then routed (PR #2177).** With
  `/opt/ros/$ROS_DISTRO/share` present its `cli-tests` step runs the rosidl
  parity tests `--ignored`, and they FAIL on an absent interface package
  (`[NO-PKG]`, issue 1552). The runner image carried one ROS package (the zenoh
  router). PR #2177 adds `[prereq.ros-interface-packages]` to
  `nros-sdk-index.toml` — `std_msgs`, `geometry_msgs`, `sensor_msgs`,
  `example_interfaces`, measured from the `ros_input` / `ros_input_dir` call
  sites in `rosidl-codegen/tests/{parity,comparison}_test.rs`; the apt names
  derive from `ROS_DISTRO` (humble) like the router's — and
  `scripts/ci/runner-container.sh` puts them in its ROS layer
  (`AMENT_PREFIX_PATH` still unset; `ROS_DISTRO` in the image ENV is what the
  recipe's `/opt/ros/$ROS_DISTRO/share` probe reads). `check-cli-tests` now
  carries the same event-routed `runs-on` / `container` / clean-tree step as the
  other two, behind the same `vars.NROS_GATE_SELF_HOSTED`; with it unset the job
  is the hosted one it was. Checked only statically
  (`check-workflow-runner-isolation`, `check-workflow-just-provisioning`,
  `check-ci-image-apt-packages`, the generator's `--check`): no image was built
  and no run happened on the pool.

  **To re-enable, operators must:** rebuild the runner image
  (`scripts/ci/runner-container.sh <labels> --build`) and redeploy the runners;
  get the idle runners taking jobs again and settle capacity and labels (the
  trial's starvation, below, is not fixed by this); run a green
  `workflow_dispatch` of `gate.yml` on the pool; only then set the variable.
- **W7 is not routed around.** GitHub labels only select, never exclude, so
  keeping jobs off `newslab-server-241` needs a new label on the other two
  runners — a registration change (`NROS_RUNNER_LABELS`, re-applied on every
  ephemeral re-registration), out of scope here.

**Clean tree.** A self-hosted checkout lives in the `nros-runner-work` volume
and `actions/checkout` cleans only the superproject, so an earlier job's
submodule worktrees (its pin, its build output) survive. The first step after
checkout on the pool is `scripts/ci/self-hosted-clean-tree.sh`: it reports what
it found, deinits every live submodule, `git clean -ffdx`, refuses
`CARGO_TARGET_DIR` or a `build.target-dir` in the runner's cargo config, and
ASSERTS no untracked or ignored file and no submodule worktree remain before
any step runs. It keeps the cargo registry, sccache (left on its persistent
volume at the justfile's size — the hosted 1 GB would evict queue L3's warm
cache), rustup and the pinned SDK store. Checked by planting state in a scratch
clone: a submodule at an older commit with a stale `build/` inside, `target/`,
`target-embedded/`, `build/.fixtures-built`, a museum `nros`, an untracked
`generated/` — `--check` fails naming each, the clean leaves none, and the
job's own `git submodule update --init` then lands at this commit's pin.

**Trial (2026-10-13), switched off again.** A `workflow_dispatch` run on main
(38312520026) with the variable set put both siblings on `newslab-133`, green:
`workspace-all` 3–4 min, `test-unit` 1.5 min (1966 passed, 6 skipped). The
clean step ran in 1 s; the CLI restore + build and the resolver took about
80 s of the workspace-all job. The variable was then left on from 05:14Z to
08:20Z for merge groups:

| | `workspace-all` | `test-unit` |
| --- | --- | --- |
| run time on the pool (16 jobs, all green, all three runners) | 1.9–6.8 min | 1.3–6.2 min |
| hosted merge-group median, for comparison | 13.9 min | 9.4 min |
| **wait for a runner** | **5–122 min** | **10–164 min** |

The jobs are 60–85 % faster and the 40 % bar is cleared on run time, but a
gate run that took about 15 min end to end now waited an hour or more, so the
variable was unset. Two causes, both on the runner side: three runners now
carried queue L3 plus two siblings for each of up to five queue entries, and
`newslab-133` sat online and idle while about 14 routed jobs were queued —
the same symptom recorded above when this item was written. Jobs already
queued with the variable on kept their self-hosted `runs-on` and drained
over the following hours; none was cancelled, because cancelling a queue
entry's required check ejects the PR.

**Still to do, before the variable goes on again:** a runner that reports
idle must take queued work (an operator diagnosis, beside W7), and either
more runners or a label that keeps L3 and the siblings from competing for
the same three. `check-cli-tests` still needs the ROS interface packages in
the runner image. The acceptance median then needs the variable left on.

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
alone still restores the full target tree for its test targets.

**Acceptance met (measured 2026-10-13).** Over the last 20 successful
merge-group runs, the restore step in `check-fast` took 0–1 s against the
30-s target and the 3.8-min baseline.

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

**Landed (2026-10-13, PR #2072).** Measured first, and the split was not the one the
title assumed: almost none of it is codegen.

| part of `just generate-bindings` (host-tests run 38200041441, 1153 s) | time |
| --- | --- |
| discovery + 222 standalone `nros generate-rust` | ~18 s |
| `nros sync` of the 9 Rust / bridge / empty workspaces | 0–39 s each, ~70 s |
| `nros sync` of the 15 C/C++ workspaces — the **metadata probe** | 58–122 s each, ~1070 s |

The probe is `metadata_probe_cmake`: one CMake project per workspace that
builds the component packages and links a recording executable against a real
`libnros_cpp.a`. Each workspace compiled its OWN copy of that runtime — the
C/C++ sibling of issue 0522, which had shared the Rust harness's target dir and
left this one alone. And the verb already had a freshness edge: every sidecar
carries `probe_inputs_key` (package sources + CLI source stamp + every crate
the probe compiles), so a tree with fresh sidecars runs the whole verb in
**14 s** locally — **9 s** with every probe build dir moved away, none of which
it recreates. The CI cost was missing persistence on a hit and an unshared
runtime on a miss. Both are fixed, in that order of importance:

- **The cache** (`gate` check-build and `host-tests`). `scripts/ci/bindings-cache.sh
  key` prints the key, the fallback prefix and the cached path list; the
  workflows restore before the verb and save right after it, and the job still
  runs `just generate-bindings`, whose own freshness regenerates anything
  stale. Cached: every gitignored `generated/` tree (with its `.sig` stamp) and
  every workspace member's `metadata/*.json` + `*.unprobeable` — 16 070 files,
  0.7 MB zstd — at fixed depths, because by save time the probe build dirs exist
  and a `**` both walks them and matches their own `metadata/` (60 files on a
  built tree). `check` refuses a cached path git tracks. Key: prefix
  `nros-bindings-v1-<os>-<arch>-<checkout path>-<ament>-<codegen>-`, where
  `<ament>` is `nros_generate_rust_ament_signature` (split out of the verb's own
  signature, so it is the same function) and `<codegen>` the three
  `codegen_version.rs`; then the CLI binary key (W3, a superset of the stamp
  every sidecar carries) plus the index entries of `examples/`, the test-bin
  roots, every `probe-source-dirs.txt` crate, `cmake/`, `zephyr/cmake` and the
  verb's scripts. A codegen bump moves the PREFIX, so nothing is restored and
  every generated tree and fixture rebuilds from cold; any other input moves
  only the suffix, so `restore-keys` restores the newest entry and the verb
  re-validates it package by package.
- **The miss path** — `configure_project` passes `-DNROS_SHARED_CARGO_ROOT` (issue
  0805's keyed mechanism; `nros_shared_cargo_dir` hashes the full cargo input
  set, so only identical configurations share) rooted where the Rust harness
  already shares (`$NROS_BUILD_ROOT` or `<checkout>/build`). 15 workspaces
  resolve to three keys (plain, `lifecycle,param_services`, `safety`). A
  pre-existing probe build dir keeps its real `cargo/` and does not share — the
  module's documented degrade, so nothing is wiped.

Results (4-core box, same core count as the hosted runner):

| case | before | after |
| --- | --- | --- |
| cache hit: restore into a tree with no outputs and no probe build dirs, run the verb | 1153 s (CI, no cache) | **10.2 s** |
| one C/C++ probe build, cold (`workspaces/cpp`) | 76 s | 23 s after `workspaces/c` filled the shared dir (`realtime-c`: 10 s) |
| whole verb after a CLI change (every sidecar stale) | 1441 s (warm unshared dirs) | **633 s** (fresh dirs, shared) |
| probe build dir per C/C++ workspace | ~1.6 GB | ~230 MB + one shared 1.5 GB per key |

The 177 sidecars the shared probes wrote are identical to the unshared ones
field for field, apart from `provenance` (which moved with the CLI stamp, as it
must). `check-gate-cache-keys-agree` gained a rule: every job running
`just generate-bindings`, in any workflow, takes its key, fallback and paths
from the key script's outputs, restores before the verb and saves after it
(self-test: six mutations).

**What a hit is worth, honestly.** The CLI source stamp is in every sidecar's
key, and `packages/cli` or a crate it compiles changed on most days of the week
this was written (0–64 commits a day), so night to night the scheduled lanes
will mostly take the miss path — now ~10 min instead of ~19–22 — and the
under-3-min hit is what the second lane of the same night (`gate` 02:1x then
`host-tests` 03:2x, same `main` commit, one cache scope) and every re-run get.

Measured on CI before merge, two `workflow_dispatch` runs of `host-tests` on
the PR branch (same commit, hosted `ubuntu-22.04`, CI image):

| run | restore | `just generate-bindings` | save |
| --- | --- | --- | --- |
| 38306090379 — cache empty, shared probe runtime | 1 s (miss) | **11.4 min** (was 18.7) | 3 s |
| 38308233601 — exact hit | 2 s | **14 s** | skipped |

The key step costs 1 s with the ament walk included.

**Scheduled runs on main (2026-10-13).** Both lanes took the miss path, as
predicted: `host-tests` 38341537002 spent 696 s in `generate message
bindings` (1154 s the night before), and the `gate` check-build sibling,
38336075498, spent 673 s (1420 s the night before). Restore and save cost
1–2 s each. Still to do: a night where the second lane lands on the first
lane's commit, to see the scheduled hit, and how often a night's first lane
gets a prefix hit rather than a cold restore.

### W5 — shorten the slowest fast-line gates

`knob-single-reader` (270 s), `probe-workspace-caps` (257 s) and
`api-parity` (167–190 s) set the fast line's floor, and the fast line also
runs in the `pre-push` hook, so every local push pays it too. Profile each:
a cold cargo build inside a gate, or a re-scan that could read a cached
index, is the usual cause. Fix only what the profile shows.

**Acceptance:** `just check fast` under 6 min wall on CI, and the `pre-push`
hook proportionally faster.

**Landed (2026-10-12).** Profiled each gate solo first. Every fix removes a
repetition; none narrows a check, adds a cross-run cache or skips work.

| gate | where the time went | fix | solo before → after |
|---|---|---|---|
| `knob-single-reader` | comment-stripping inside `readers_in`, i.e. once per (file, knob, spelling): 45 158 strips of 684 files | strip once per file; memoise the CMake-half strip on its own text | 180 s → 7 s |
| `probe-workspace-caps` | each case's `pkg_a_lib` compile waited on a cold cargo build of the runtime (69 of 72 s) | both cases name one `NROS_SHARED_CARGO_ROOT` (issue 0805's input-keyed dir) inside the run's tmpdir; headers and TU compiles stay per build tree | 135 s → 72 s |
| `rmw-ret-sign` | a per-slot regex rebuilt for every (line, slot): 53 M `re.search` calls | compile once, prefilter lines with the union, skip the empty `DUAL_RETURN` scan | 105 s → 8 s |
| `rmw-api-comparison`, `rmw-api-parity`, `rmw-slot-producers` | `consumers_in`: one search per slot per file | one look-ahead `finditer` per file | 21 → 8, 11 → 4, 10 → 4 s |
| `api-parity` | 92 of 93 CPU-s is the cold `cargo doc` (host builds 43 s, check 22 s, rustdoc ~25 s) | none; see below | 36 s |

Outputs of `knob-single-reader`, `rmw-ret-sign` and the three slot-producer
consumers are byte-identical before and after, and `consumers_in` was compared
over 1814 files and 50 000 fuzz inputs. Each gate still fails on its defect: a
second reader of `NROS_EXECUTOR_MAX_CBS`, a retired `NROS_DECLARED_MAX_QOS_DEPTH=`
under `cmake/`, the workspace-caps walk disabled (case A goes 3/5 red), a new
`rc < 0` on `create_session`, and a consumer planted on an inert slot.

`just check fast` at -P4 on the 4-core box: **424 s → 274 s wall**, busy CPU
1638 s → 1084 s. The lane runs at 4.0× effective parallelism, so it is
CPU-bound: its wall time is now total CPU divided by four, not the slowest
gate. New top five: `probe-workspace-caps` 100 s, `api-parity` 56 s,
`hook-repo-side-effects` 38 s, `rmw-doc-slot-names` 38 s, `rmw-ret-sign` 20 s
(contended times).

Deferred: `api-parity`'s rustdoc could skip JSON for third-party crates
(about 15 CPU-s), but a re-export of a third-party item would then drop off the
surface silently, so it is not done. A faster shared comment stripper
measured within noise outside cProfile.

**CI acceptance NOT met (measured 2026-10-13).** Over the last 20 successful
merge-group runs, the `just check fast` step took 363–735 s, median about
**11 min**, against the 6-min target and the 10.0-min baseline (4 runs). So
the local 424 s → 274 s did not reach CI at all, and why is not yet known:
the baseline is small, and since W1 the step shares no job with the others,
so that is not it. Still to do: profile the step on the hosted runner itself
(per-gate times from one run's log) before shaving more gates, and decide
whether the fast line belongs on a larger runner.

**Follow-up (2026-10-14, PR #2144): sccache hit rate, and the hardware.**
Two measured suspicions from the paragraph above, both now evidence.

*Hardware.* Every compiling gate job prints the CPU
(`scripts/ci/report-runner-hardware.sh`). The same `just check fast` landed
on `AMD EPYC 7763` (4 vCPU = 2 cores x 2 threads) and on `AMD EPYC 9V74`: busy
CPU 2385-2897 s on the first against 2118-2253 s on the second, and wall 607-736
s against 541-573 s, across runs of one change with the same cache state. That is the
bimodal spread the profile found; it is the runner pool, not the gates.

*Why sccache hit 3.7 %.* Reproduced on a fresh checkout with sccache 0.17.0:
a cold `just check fast` makes 1239 compile requests with 28 hits (CI: 1241
and 30). A second run on the same cache, same path, targets wiped: 74.5 %.
The same cache from a moved checkout: 40.8 %. Two causes, one fixed here:

1. **A tmpdir in the key.** Of the 208 misses that survive a same-path rerun,
   148 were `api-parity`'s `rustdoc-ours` build under a Python tempdir and 57
   `probe-workspace-caps` under `mktemp -d`: sccache's Rust key contains the
   cargo target dir's path. `SCCACHE_BASEDIRS` is supported by 0.17.0 and does
   NOT normalise a Rust key (a two-crate cargo project built under two sibling
   target dirs: 2 misses of 2, with and without it), so it is not used. Both
   gates now build under `<checkout>/tmp/gate-scratch/<name>`, emptied first
   and removed after (`scripts/lib/gate-scratch.sh`, `gate_scratch.py`,
   `init_stable_test_tmpdir`): only the NAME is stable, never the contents.
   Under `build/` it made `check-sizes-header-mirrors` verify mid-run and
   `check-fast-skip-allowlist` red on the PR's first run. The two gates alone,
   second run: 109 hits / 206 misses -> 310 / 5. Left alone: `rustprobe` and
   `build-root` tempdirs, 2 misses.
2. **The warm job warmed the wrong thing.** The cache a pull request
   restores holds what `warm-cache` compiled, which was the CLI build
   (`cargo test --no-run` on `packages/cli`) and none of `check-fast`'s ~1240
   requests. `warm-cache` now runs `just check fast` after the same
   provisioning, and saves with `!cancelled()`; it also runs on
   `workflow_dispatch` so a branch can prove it, and `ci-ok` hears it
   (`check-default-gates-run-somewhere`).

Cache sizing: the warm job's dir is 1.4 GiB on disk and 1415-1532 MiB as an
entry (the old 1 GiB cap was full with the CLI alone), so
`SCCACHE_CACHE_SIZE` is 2G in every job; `check-gate-cache-keys-agree` holds
one size and the warm ordering (`just check fast` after configure, before
save). Pool: 7.8 GB active after the runs below. An entry is per sha, so two
retained sccache entries are ~3 GB of the 10 GB beside W3's 12 MB and W4's
1 MB; the 4.2 GB `nros-cli-*` entry is the real pressure and is not touched.

Measured on CI (`check / fast`, PR #2144):

| run | cache restored | CPU | hits | `just check fast` | busy |
|---|---|---|---|---|---|
| pull_request, first push | main (CLI only) | EPYC 7763 | 6.4 % | 736 s | 2897 s |
| pull_request, fixed | main (CLI only) | EPYC 9V74 | 3.7 % | 573 s | 2253 s |
| dispatch 1 (old gate paths in the cache) | branch, 1st warm | EPYC 7763 | 57.9 % | 607 s | 2385 s |
| dispatch 2 | branch, warm on the final sha | EPYC 9V74 | **89.4 %** (730 / 817) | 541 s | 2118 s |

A pull request cannot restore a branch's cache (it reads `refs/pull/N/merge`
and `main` only), so its row is the baseline; the hit rate shows on the
dispatch, and on pull requests once `main`'s warm job has run this change.
The time saved is small next to the hit rate (573 -> 541 s on the same CPU
model, `declared-qos-registration` 97 -> 34 s, `api-parity` 92 -> 68 s,
`codegen-size-bound-golden` 87 -> 42 s, `probe-workspace-caps` 158 -> 124
s): most of a fast run's CPU is not compile. Acceptance (< 6 min) is still
NOT met; the runner CPU is the larger lever.

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

**Landed in part (2026-10-13), acceptance NOT met.** What each lane starts
from, measured from the job logs rather than assumed:

| lane | runner | what persisted between runs |
| --- | --- | --- |
| run-matrix `tier 1`, `tier 2` | self-hosted | sccache (rustc only) — `actions/checkout`'s `git clean -ffdx` deleted `build/`, every `generated/`, leaf build and target dir; tier 1's clean alone took 5 min |
| nightly `tier 2 nightly` | self-hosted | the same |
| live-peer (all three build jobs) | **hosted** `ubuntu-22.04`, CI image | nothing |
| nightly ports (`nuttx`, `freertos`, `threadx_linux`, …) | **hosted** `ubuntu-22.04`, CI image | nothing |

So the item's premise holds for three lanes, not all: live-peer and the
nightly ports run on hosted VMs with no tomorrow. They are left as they are —
a persistent tree is not available there, and `actions/cache` (10 GB per repo,
shared with W3's and W4's entries) cannot hold multi-GB fixture trees without
evicting those. Moving them onto the pool is a routing decision of W2's kind
(labels, image contents, capacity next to W7), not a cache.

**The self-hosted lanes: a persistent tree.** The `_work/<repo>/<repo>`
checkout is shared with queue L3 and the W2 gate siblings, which MUST start
clean (issues 0196/1476), so the tree cannot be that checkout, nor live inside
it (`-ff` removes a nested repo). `scripts/ci/fixture-tree.sh sync` keeps one
BESIDE it, `_work/<repo>/nros-fixture-tree` (the `nros-runner-work` volume
outlives the runner container), and every later step runs there
(`defaults.run.working-directory`). The sync is a user's `git pull`: this
commit checked out `--force` (tracked edits undone), initialised submodules
moved to their pins, build output KEPT and not trusted — `just build <lane>`
decides freshness exactly as on a developer's tree, clears the fixture stamp
before building and rewrites it after, and the test half's `_require-fixtures`
+ `_check-fixtures-stale` re-probe every fixture. The tree is shaped like the
job checkout (same `origin`, same `refs/remotes/origin/*`, `--depth=1`), so
`origin/main` baselines and ancestry questions answer as they do there. A tree
older than 7 days is reset to tracked content: a periodic cold run. A tree
whose git metadata is unreadable re-creates its git dir and keeps its build
output.

Two things the first CI runs of the script found, both fixed before this
landed: `check-hook-repo-side-effects` ran it bare under a victim repo's
`GIT_DIR`, the default command was `sync`, and `checkout --force` wrote the
victim's commit into the REAL tree mid-build (run 38334069002) — the script now
clears the inherited git environment for every command, refuses a bare
invocation, and the probe runs its `--self-test`; and a fetch from the job's
shallow checkout without `--depth` died on a missing parent (run
38336863204).

**mtime, measured.** Ninja and cargo freshness is mtime-keyed, and git
rewrites only the paths whose content differs, so an unchanged source keeps
its mtime across the sync — the same property a user's pull has, and the
reason content-keyed freshness is not needed for this tree. One trap was
real: `checkout --force` rewrote stat-dirty but content-identical files (a
touch, a restore) with a new mtime; the script refreshes the index first
(self-test case).

**What a no-change re-run re-did** (`just build native`, 4-core box, warm
tree; cold is 9479 s, the build right after a rebase onto main):

| part | before | after | cause / fix |
| --- | --- | --- | --- |
| compile-check fixtures (68 rows) | 2297 s | 162 s | every builder `rm -rf`-ed and rebuilt; now reuses a row whose `.inputsig` (the signature the test-side probe recomputes) matches, as the west lane already did (issue 0509) |
| the native module (154 rows) | 1883 s | 1747 s | 26 rows recompile the core crates: rows sharing a target dir point `NROS_SIZING_DESCRIPTOR` at different descriptors — [issue 1869](../issues/archived/1869-shared-fixture-target-dir-thrashes-on-sizing-descriptor.md), fixed after this table (see below); every C/C++ workspace row re-runs its cmake configure through `nros build` (~10 s each) |
| `_codegen` + `check::fast` (a dependency of `build-test-fixtures`) | ~390 s | 297 s | `check fast` is 275–333 s at -P4 on this box |
| compile-check + the post-build checks | ~2450 s | 262 s | |
| **whole re-run** | **4720 s (50 % of cold)** | **2306 s (24 %)** | |

**Issue 1869, fixed afterwards** (PR #2137, RFC-0100 D13): a caller-named target dir
now gets one cargo `build.build-dir` per configuration, keyed on the sizing
descriptor's content, with the descriptor named as a class copy. The 24 cargo
workspace rows of the native lane (`workspace-fixtures-build.sh linux rust`),
warm, no change: 19 rows / 273 s of row time before, 0 rows / 30 s after
(the same measurement also found and removed `cyclonedds-sys`'s self-dirtying
`OUT_DIR` watch). The native module's 1747 s above therefore drops by roughly
240 s; the C/C++ rows' per-row cmake reconfigure is fixed below (PR #2170).

Freshness, checked on the same tree after the change: a one-line edit to a
compile-check row's source made `_require-fixtures-ready` (lane native) fail
naming that row's `.inputsig`; `just build-compile-check-fixtures` then rebuilt
that row alone (63 reused) and the gate passed; reverting the edit rebuilt it
once more. An edit to a cargo leaf (`examples/linux/rust/talker`) is never
judged from a stamp at all — the staleness probe for cargo rows IS a
`cargo build`, so the gate rebuilt the binary (mtime moved) rather than pass
the old one. Neither path can hand a stale artifact to a test.

Found on the way, each a defect a warm tree hits and a cold one may not, and
each fixed on main by its own issue while this was measured: a pre-W7b build
dir carries the tree's OWN old `NANO_ROS_*` cache entries (phase-484 W8 now
refuses them with the `cmake -U` line that drops the residue — every build
dir in a CI tree postdates W7b, and the 7-day reset bounds it), and `nros
sync`'s synthesised ament prefix registered nothing in the resource index the
2026-10-10 parser reads ([issue 1861](../issues/archived/1861-sync-synth-ament-prefix-has-no-resource-index.md)).

**CI.** The third dispatch on the branch (run 38343006472, `tier 2` on
newslab-server-241, `tier 1` on newslab-133) synced the tree in 3 min and 1 s
— the first sync of a tree means a submodule update — and then stopped in
`check::fast` at `sizes-header-mirrors`, on the same two probe dirs main's own
tier-2 nightly stopped on that morning in the ordinary cleaned checkout (runs
38346448295, 38350414982): [issue 1858](../issues/archived/1858-metadata-probe-cpp-config-mirror-goes-stale.md),
fixed on main the same day. So no CI run of this item reached its fixtures,
and the second-run CI number per lane is still to be measured.

Gates: `check-fixture-tree-persistence` (self-hosted only; never a
merge-gating or clean-tree job; run default equals the script's path; sync
directly after checkout; no step escapes the tree; uploads rooted in it; every
self-hosted `just build` job uses it — 10 mutations) runs the script's
`--self-test` (14 cases: build output kept, tracked edits undone, unchanged
mtime kept, tree inside the checkout refused, over-age reset, shallow and
`origin/*` mirrored, an inherited `GIT_DIR` does not redirect it, a bare call
refuses, a broken git store recovers). `lane-stage.py` maps the new step to
provisioning.

**`nros build`'s per-row cmake configure, fixed afterwards** (PR #2170).
Cause, measured: `cmake -S -B` is not make-shaped, it runs the whole configure
and generate on every call, and `nros build` calls it once per image. A
workspace's images share one build dir per board/RMW, so a no-change re-run of
the linux C workspace (`workspace-fixtures-build.sh linux c`, 20 rows) printed
`Configuring done` 20 times — nothing forced it but the call itself (the shared
cache also held the last row's `-DNROS_RESOLVED_DIR`, which differs per image
and would have defeated a plain "args unchanged" test). `run_configure` now
keeps a stamp beside the cache (the `-D` arguments except that seed-only
variable, plus the `NROS_*`/`CMAKE_*`/toolchain environment) and skips the
explicit configure when the dir holds a generated build system and the stamp
matches; `cmake --build` re-runs the configure itself on any registered input,
as it already did on the bash road (`nros_cmake_configure_if_needed`). All three
planner configure sites (workspace, single-leaf, cmake member) share the one
helper, `builder/cmake_configure.rs`.

| measurement (4-core box, warm tree, no change) | before | after |
| --- | --- | --- |
| C workspace rows, row time | 200 s, 20 configures | 32 s, 0 configures |
| `just build native` | 2306 s (24 % of 9479 s cold) | 1772 s (18.7 %); a second run 2249 s (23.7 %) |

Negative controls: unit tests (a changed `-D`, a changed or removed `NROS_*`
variable, a removed build system, an invalidated stamp each force the
configure); end to end, `touch src/demo_bringup/system.toml` made `cmake
--build` reconfigure once while `nros` skipped its own, and a changed `NROS_*`
variable made `nros` configure.

**Still to do.** Acceptance (under 10 % of cold) is not met: 18.7 % locally.
What the 1772 s is made of: `check fast` and `_codegen` ~300 s, the native
module 1186 s of which about 100 single-package cargo leaf rows cost ~14 s each
(the single-package leaf road; PR #2158 extends the shared target dir to it), and the post-build checks ~260 s. The CI second-run time per
lane (two runs of each lane on the same self-hosted runner) is blocked on free
runners and still unmeasured.

### W7 — find out why `newslab-server-241` is 2–5× slower

Compare `nproc`, the jobserver budget, disk type and free space, and other
tenants (CLAUDE.md: `/home` is shared by 14 users) across the three runners.
Until it is fixed, consider a label that keeps the critical-path L3 job off
it.

**Handed to the runner operators (2026-10-13).** It needs the hosts, and so
does the W2 symptom of a runner that reports idle and takes no job. Both are
for whoever administers the runner machines; this repo cannot see them
beyond job logs.

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

## Open issues that name this phase

Open issues whose `related:` names this phase (listed 2026-10-12, so the
link runs both ways). Listing one here records the link, not a claim that
this phase will fix it.

- [issue 1869](../issues/archived/1869-shared-fixture-target-dir-thrashes-on-sizing-descriptor.md) — workspace fixture rows sharing a target dir rebuilt the core crates every run (W6; resolved by RFC-0100 D13)
- [issue 1177](../issues/1177-no-alloc-nros-c-lane-runs-only-on-schedule.md) — `check::workspace-features` gates no merge, so its reds are found by embedded consumers -- two so far, and the tier i …

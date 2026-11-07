# Phase 543 — Every gate is a nextest test

**Status (2026-11-06). IN PROGRESS — W0 merged (#3207); W1 and W2 in #3209; W3 first pass done.** No RFC yet: the design below
is this phase's. If W1-W3 settle it, it moves into an RFC and this doc keeps
the work items.

Decided (maintainer, 2026-11-06):

- **One runner for every gate.** Gates run as nextest tests, so filters,
  timings, junit, partitioning, process-group kills and resource limits are the
  same interface for gates and for tests. `scripts/build/run-gates-parallel.sh`
  retires (W6).
- **No gate skips its way to green.** A gate that runs must pass. What does not
  apply to a lane is out of its SCOPE, declared precisely, and is never run or
  counted. This is the test rule of issue 1758 applied to gates: today about 60
  call sites skip at runtime and exit 0 (`nros_check_skip`), and that pattern
  goes.
- **Gates outside the `check` module stay where they are.** Platform-module
  checks (`linux::check`, `zephyr::check-copy-out`, …) join the umbrella by
  attribute. They are not moved.
- **cargo-nextest at the latest stable**, 0.9.151 (W0).
- **Unused `check-*` scripts are deleted** (W0).

## Inventory (2026-11-06)

| Where | Count | Runs today through |
| --- | --- | --- |
| `check` module, fast lane | 511 | `run-gates-parallel.sh` (`NROS_GATE_LANE=fast-serial`) |
| `check` module, build lane | 28 | the same runner; CI shards it 3 ways with `gate-shard.py` |
| `check` module, in no lane (`.config/gate-lane-exempt.txt`) | 28 | single calls from `ci gate` steps, workflows and `pre-push` |
| `check` module, takes parameters | 6 | by hand |
| CI-only gates outside `check` | ~12 | workflow steps (`colcon-parity`, `scaffold-journey`, `generate-bindings` + diff, `sdk/verify-index.py`, `check-qemu-source-features.py`, `ci/check-fast-skip-allowlist.sh`, `pr-verdict-check.sh`, `queue-timeout-agreement.py`, `roadmap-audit.py`, `lane-census-diff.py`; `next-stable-delta.py` is advisory) |
| Platform-module gates | ~10 | by hand / nightly (`linux::check`, `qemu::check`, `nuttx::check-libc-struct-sizes`, `zephyr::check-copy-out`, `ros_editions::image-check`, `rust-rtos-link-check`, `doc-c-check`, `verify-kani`, `verify-verus`, `verify-size-probe`) |
| `pre-push` hook | 4 | the hook (`check fast`, `issue-ids-check`, `submodule-commits-reachable`, `submodule-pins-check`) |
| Test lanes | — | already nextest (`nros-tests`): `test-unit`, `test-lane-contracts`, tier 1, tier 2, matrix |

The gate set is derived in one place, `scripts/check/check-gate-lists.py`
(issue 1072 removed the authored list). Skip call sites fall into about twelve
capabilities: `cmake`, `cc`, `cli`, `ros2`, `colcon`, `zephyr`, `nuttx`, `fsp`,
`bindgen`, `build-tree`, `git-history`, `submodule-store`.

## Design

### Organization

```
packages/testing/nros-gates/      a `harness = false` binary, no dependencies
  list:   <tier>::<gate>[::<unit>]      fast::cli-old-spellings
                                        build::scaffold-builds::rust_project_talker
  run:    just <module>::<gate> [<unit>]
```

A gate's metadata lives ON ITS RECIPE, as `just` attributes, which
`just --dump --dump-format json` exposes (measured, just 1.50). That keeps the
one-insertion property issue 1072 bought: adding a gate is writing its recipe.

| Attribute | Meaning |
| --- | --- |
| `[group('gate')]` | a recipe outside `check` is a gate (inside `check` every non-helper recipe is) |
| `[group('tier:build')]`, `tier:image` | needs a build, or a built image; `fast` is the default |
| `[group('needs:<cap>')]` | a capability it requires (`cmake`, `zephyr`, …) |
| `[group('writes:<resource>')]` | a shared resource it writes (`target`, `example-build`, a generated tree) |
| `[group('cpu:<n>')]` | its CPU weight |
| `[group('disk:<gb>')]` | its peak scratch disk |
| `[group('units')]` | it answers `--list-units`; each unit is its own test |

### Scope: lanes are capability claims

```toml
# .config/gate-lanes.toml
[lane.push]   # pre-push
tiers  = ["fast"]
claims = ["cmake", "cli"]
[lane.pr]     # the PR's required check
tiers  = ["fast", "build"]
claims = ["cmake", "cc", "cli", "bindgen", "rust-targets"]
[lane.gate]   # `just ci gate`
tiers  = ["fast", "build"]
claims = ["cmake", "cc", "cli", "bindgen", "rust-targets"]
```

- The harness reads `NROS_GATE_LANE` at LIST time. A gate whose tier or
  `needs:` lies outside the lane goes in the `--ignored` listing, so nextest
  shows it as not run, by name. It is never run and never a pass.
- Every gate that runs runs with `NROS_CHECK_SKIP_STRICT=1`, so a runtime skip
  is a red `[UNMET PRECONDITION]`. Provision the host, or narrow the lane.
- A per-unit `needs:` covers today's part-level skips (`template-copy-out`'s
  Zephyr projects, `scaffold-builds` variants).
- Lane names are provisional (open question 1).

### Parallelism without conflicts

| Resource | Mechanism |
| --- | --- |
| CPU | `threads-required` from `cpu:<n>`; a heavy gate takes more slots |
| The root `target/` (cargo's build lock) | test group `gate-root-target`, low `max-threads`: a slot held by a gate blocked on the lock is a slot lost (the build lane measured 3.8x on 48 CPUs) |
| A shared in-tree output (generated trees, example `build/`) | `writes:<resource>` -> a test group with `max-threads = 1` per resource |
| Disk | see below |
| The git index | `GIT_OPTIONAL_LOCKS=0`, set by the wrapper, as today |
| Scratch trees | `scripts/build/gate-scratch.sh`, unchanged: each unit owns one |
| Tracked files | the wrapper's before/after snapshot, as today: a gate that writes one fails the lane |
| Ordering | `priority` from the previous run's junit times, slowest first |
| Interrupt | nextest's per-test process group: SIGTERM, then SIGKILL after the grace period |

The test-group DEFINITIONS are fixed and live in `.config/nextest.toml`. The
overrides that put gates in them are GENERATED from the attributes into
`build/gates/nextest-gates.toml` and passed with `--tool-config-file`; nextest
documents overrides coming from a tool config (0.9.145 diagnostics), not group
definitions. An undeclared write is caught by a self-test that runs the fast
tier in two random orders and compares verdicts.

### Disk

Measured on this host after one `just ci gate` in a fresh worktree: `target/`
54 G, `build/` 128 G (`corrosion-cargo` 51 G, `sizes-probe` 38 G,
`example-lint` 32 G), `packages/cli/target` 25 G — about 207 G per worktree. Six
agent worktrees filled a 1.8 T disk on 2026-11-06, and the failure surfaced as a
file cut off mid-write, not as a disk error. CI hosted runners have about 75 G
after reclaim (issue 2100).

What exists: `gate-scratch.sh` (scratch trees: pass removes, fail keeps until
the next same-scope run; issue 2239), `gate-prune.sh` (a gate releases its own
output, opt-in `NROS_CI_PRUNE_GATE_OUTPUTS=1`; issue 2100),
`reclaim-disk.sh` / `disk-report.sh` (CI; issue 1353),
`release-fixture-intermediates.sh` (issue 2130), cargo-sweep in CI (issue 2191).

What this phase adds:

- `disk:<gb>` on the gates that write a lot (the three above, the two parallel
  build gates). A test group `gate-disk-heavy` bounds how many run at once, so
  peak scratch is bounded rather than `jobs x worst unit`.
- A wrapper PREFLIGHT: the lane's declared peak against `df` of the build root
  and of `TMPDIR`. Short of it, the lane refuses before any gate runs, naming
  both numbers. It never starts and dies of ENOSPC mid-write.
- `disk-report` (per-area `du`) printed by the wrapper on a failed lane, so a
  red lane says where the space went.
- Not in scope: deleting a developer's incremental build output. That stays
  opt-in (`gate-prune.sh`), as RFC-level policy says.

### The runner's own dependencies

The gate runner runs BEFORE any build, and the fast tier's premise is a
checkout with no submodules, no CLI and no provisioned sources (issues 0650,
1373; `check pristine-checkout` enforces it). So the runner may depend only on
what that checkout has:

| Needs | Why it is safe | Must not |
| --- | --- | --- |
| `cargo` + the pinned rustc | the root `rust-toolchain.toml`; `install-rust-toolchains.sh` completes it once, before the run (issue 1868) | — |
| `cargo-nextest` 0.9.151 | the index pin, at the store path built from `nros-sdk-index.toml` | come from PATH: on this host `~/.cargo/bin` holds 0.9.99, ahead of the pin |
| `just`, `python3`, `git`, `bash` | every gate already needs them | — |
| the harness crate | its OWN cargo root (`[workspace]` in its manifest), zero crate dependencies, its own target dir | resolve the root workspace: `cargo metadata` there needs every `-sys` submodule |

Consequences:

- `nros-gates` is a standalone root like `packages/cli`, excluded from the root
  workspace, with a tracked zero-dependency `Cargo.lock` in
  `[rust] locked_roots`. Building it cold is one small crate.
- It never runs the `nros` CLI and never reads a submodule. A gate that needs
  either says so with `needs:cli` / `needs:sources`, and the pristine lane does
  not claim them.
- Provisioning nextest must not need the CLI either. Today it does
  (`nros setup --tool cargo-nextest`), and the CI image ignores the pin
  (`ci/docker/ci-base/Dockerfile` runs an unpinned `cargo install
  cargo-nextest --locked`). W0 makes the image bake the index's prebuilt, by
  URL and sha256. A host without the pinned nextest gets a refusal naming the
  remedy (`just setup linux`), never a fallback to PATH and never a skip.

### Workflow

```
<lane wrapper> [<filterset>]   (W5: one recipe per lane) the wrapper:
   pre:  TMPDIR refusal, git ceiling, GIT_OPTIONAL_LOCKS=0, unset GITHUB_*,
         skip-ledger self-test, pinned toolchains, disk preflight
   run:  cargo nextest run -p nros-gates --profile gates  (NROS_GATE_LANE=<lane>)
   post: tracked-file check, timing profile from junit, disk report on failure
just check <gate>                     one recipe, directly, as today
cargo nextest list -p nros-gates      every gate and unit
serial bisect:  --test-threads 1 --fail-fast
CI shards:      --partition slice:<k>/<n>   (replaces gate-shard.py + weights)
```

The harness clears `NEXTEST_*` before running `just`, because build gates run
their own `cargo nextest`.

nextest facts this rests on (docs, 0.9.151): the custom-harness protocol is
`--list --format terse`, `--list --format terse --ignored` and
`<name> --nocapture --exact`; test groups, `threads-required` and `priority`
are stable; setup and wrapper scripts are still EXPERIMENTAL, and overrides
cannot set environment variables, which is why pre/post and the lane stay in
the `just` wrapper; `slice:` partitioning is 0.9.127, `group()` filtering
0.9.133.

## Work items

### W0 — Housekeeping — DONE (#3207)

- cargo-nextest 0.9.143 -> 0.9.151 in `nros-sdk-index.toml` (both dists, sha256
  measured, floor glibc 2.27 measured on both arches unchanged). The repo's
  `.config/nextest.toml` parses under 0.9.151.
- `ci/docker/ci-base/Dockerfile` installs the index's pinned prebuilt
  cargo-nextest (URL + sha256 read from `nros-sdk-index.toml`), not an unpinned
  `cargo install`.
- Delete `scripts/check/check-zephyr-sdk-lines.py`: nothing in the tree names
  it. Of the six `check-*` scripts a basename search called unused, three have
  callers (`check-cargo-dir-knob-key-sites.py`, `check-rust-targets-installed.sh`,
  `cpp_capability_subjects.py`). Two are unreached but owned by live designs and
  are held for a ruling: `check-book-page-map.py` (phase-532 W8 folds it into
  `book-audience`) and `check-profile-against-census.py` (RFC-0078's consumer
  tool, run with arguments).

### W1 — The harness lists every gate — DONE

Landed as specified, measured on this tree:

- `packages/testing/nros-gates`: a standalone root, zero dependencies, its own
  `.config/nextest.toml` (the repository's names `binary()` filters this root
  lacks, a parse error by issue 0743). It builds in 0.3 s. Its lock is tracked,
  in `[rust] locked_roots`.
- `check-gate-lists.py --list tests` is the listing: 515 `fast::`, 28
  `build::`, 35 `exempt::` (578). A plain run's default filter leaves out
  `exempt::`.
- `just check nextest-gates [nextest args]` runs it with the pinned
  cargo-nextest from the store path (`index_tool.py path`), refusing with the
  remedy when it is absent. It never takes nextest from PATH.
- The fast tier through nextest against today's runner, same tree: 514 of 514
  pass in 43 s, against 44 s for `just check fast`. Both runs pass the same six
  gates that SKIP at runtime (`artifact-identity-budget`,
  `cxx-compat-shim-facilities`, `ivc-fsp`, `leaf-lockfiles`,
  `python-entry-provenance`, `zephyr-workspace-foreign-checkout`), which is
  W3's work list for this lane.
- A gate that does not exist fails its test (rc 1); the harness refuses an
  empty listing.


`packages/testing/nros-gates`: lists every gate (the `check` module plus
`[group('gate')]` recipes elsewhere) as `<tier>::<gate>`, runs one with
`--exact`. No scope, no groups yet: every gate listed, tier from attributes,
defaulting to today's derivation (`check-gate-lists.py`), so the listing equals
today's lanes. Acceptance: `cargo nextest list -p nros-gates` names all 511 + 28
+ 28 + the outside gates, and a run of the fast tier matches today's runner.

### W2 — Units — DONE

- A gate with `[group('units')]` on its recipe answers `NROS_GATE_LIST_UNITS=1`
  with one unit per line and runs one with `NROS_GATE_UNIT=<unit>`. Each unit is
  a test `<tier>::<target>#<unit>`. The listing reads attributes through
  `just --dump`; a gate that cannot list its units is listed as ONE test,
  which fails when run, so a broken lister is a red, never a dropped gate.
- `scaffold-builds`: 22 units (`@self-test` + 21 variants).
  `template-copy-out`: 19 units (`@self-test`, 15 projects, 3
  `scaffold:<board>`), selected through the ownership filter its shards
  already use. An unknown unit is an error.
- The listing is 617 tests and takes 1.7 s.
- All 41 units at `--test-threads 8`, on this host: 498 s, 36 pass, 5 fail. The
  five are the Zephyr projects of `template-copy-out` (no west workspace here).
  The old runner passed the gate over them as NOT VERIFIED, because another
  unit in the same gate built. As their own tests they are red, by the phase's
  ruling. W3 gives them `needs:zephyr`, which takes them out of scope on a lane
  that does not claim Zephyr.
- `scripts/build/gate-units.sh` stays until W5 moves the lanes: the old runner
  still runs these gates whole.

### W3 — Scope — DONE (first pass)

Decided (maintainer, 2026-11-06): the fast lane claims what the ci-base image
and CI's fast job provide; Zephyr and built-artifact gates get a self-hosted
`queue` lane that claims them; a capability no runner provides (FSP) is listed
as running nowhere, visibly.

- `.config/gate-lanes.toml`: `[capabilities]` (14, each described) and four
  lanes, `fast`, `build`, `queue`, `pristine`, each `{tiers, claims}`.
- A gate declares `[group('needs:<cap>')]`; a unit adds its own
  (`<unit>\tneeds:<cap>` from its lister). `template-copy-out` tags its five
  Zephyr-road projects `needs:zephyr`, using `unit_part`, the road split its
  CI parts already use.
- `check-gate-lists.py --list tests --lane <lane>` marks each out-of-scope test
  with its reason. The harness lists those as IGNORED under `NROS_GATE_LANE`,
  so nextest shows them as not run. A gate out of scope as a whole is not
  asked for its units, because a lister may need what the lane lacks.
- Under the harness a gate never passes by skipping. It gets its own skip
  ledger (`NROS_GATE_SKIP_LEDGER`, honoured by the one ledger writer in
  `check-skip.sh`, which the Python helper also goes through) and
  `NROS_CHECK_SKIP_STRICT=1`. A non-empty ledger fails the test with
  `[UNMET PRECONDITION]`, even when the gate exited 0.
- 27 gates declare `needs:`: the six that skipped on this host, the build-tree
  pair, the two unit gates (`cli`), and 18 mapped from their skip reasons.
- `check-gate-needs` (fast) enforces three things. A `needs:` names a listed
  capability. A capability no lane claims is printed on every run (`fsp`:
  `ivc-fsp` runs nowhere; `nuttx`: no gate needs it yet). A recipe that calls
  `nros_check_skip` / `nros_check_unverified` declares a `needs:`. The 18
  recipes whose skip reason is computed at run time (`$_missing`, `$out`) are a
  ratchet baseline, `.config/gate-needs-baseline.txt`; an entry may leave it,
  never join it.
- Measured, `NROS_GATE_LANE=fast`, strict: 512 run, 512 pass, 67 not run, and
  no runtime skip anywhere. Getting there needed two things this host's
  `setup-worktree` does not provision but CI's fast job does: the `rosidl`
  source and Zephyr's sparse `minimal/include`. The two provisioning lists
  (`.config/worktree-provisioning.txt` and the hardcoded step in `gate.yml`)
  have diverged, which is why a local fast run and CI's could disagree.
  `third-party/nuttx/libc` was added to the file now (for `leaf-lockfiles`).
  W5 makes the lane's claims the one provisioning list.

### W4 — Resources — IN PROGRESS

The generated tool config (`writes:`, `cpu:`, `disk:`, `priority`), the fixed
test groups, the disk preflight, the order-independence self-test.

**Cache trim — DONE (in its pull request).** Decided (maintainer, 2026-11-07):
keep caches that runs use, remove the ones they do not.

- The leak's source: `scaffold-builds` scaffolds each variant at a fresh
  `mktemp` path, and in the checkout's shared `corrosion-cargo` / `sizes-probe`
  its image builds got a new key every run. Measured: 2.4-3.1 GB left behind
  per variant per run, about 50 GB a gate run, with no speed-up from sharing
  (50 s, then 47 s). #3239 files the issue that measures the duplication
  underneath. Each unit now keeps its caches in its
  own scratch tree, which goes with it: 57-62 s cold, 0 MB left in the checkout,
  across two runs. `NROS_KEEP_CACHES=1` restores the shared cache.
- `scripts/build/trim-caches.py` evicts whole cargo target dirs in
  `corrosion-cargo` and `sizes-probe`, least recently used first, to
  `.config/cache-budgets.toml` (60 G and 40 G). It never evicts a unit used in
  the last 12 h. "Used" is the newest atime/mtime of a FILE in the unit's
  `.fingerprint`, which cargo reads on every build. A directory's atime is not
  evidence, because listing it refreshes it: the first version's own scan made
  every unit look used now, and the self-test catches that by mutation. Unit
  boundaries come from `CACHEDIR.TAG`, with triples from `rustc --print
  target-list` and not a pattern.
- A blanket `gc-shared-cargo --prune` was measured and NOT used: template-copy-out
  deletes its copies after each run, so the stores its copies reuse read as
  unreachable between runs, and pruning them makes every run start cold.
- It runs at the end of every `just ci gate`, pass or fail, and cannot change
  the verdict. Fast gate `cache-trim` runs its report and self-test.
- On this host its first run freed 42.5 G (22 + 30 target dirs from earlier
  days). Both areas were then at budget, apart from what the last 12 h used.
- The self-hosted runner is NOT touched: `runner-sweep.sh` `_evict` budgets the
  same directories there, and #3239 caps its host target dirs. Agreed with
  nros 133 p466: two evictors over the same dirs would fight.

### W5 — Wrapper and lanes

- The post-step also refuses UNTRACKED files a gate left inside the source tree
  (outside `build/`, `target/`, `tmp/` and other ignored output roots). The old
  runner checks tracked files only, and only on the fast lane. Found while
  building W4: a scaffold unit with an empty `NROS_SIZES_PROBE_TARGET_DIR`
  wrote 1.1 G of probe cache into `packages/api/nros-c/`, and no gate saw it.

- The lane's `claims` become the one provisioning list: `setup-worktree`, the
  wrapper's preflight and CI's provisioning steps read them, so a claimed
  capability is provisioned by construction, and a missing one is named before
  any gate runs (issue 2306's fix belongs here too).

One wrapper recipe per lane (names: open question 1), the pre/post steps moved from
`run-gates-parallel.sh`; `pre-push` and `ci gate` call them.

### W6 — CI, then retire the old runner

`gate.yml` fast and build jobs on the wrapper, `--partition slice:`, junit
uploaded. Retire `run-gates-parallel.sh`, `gate-shard.py`,
`.config/build-gate-weights.txt`, `.config/gate-lane-exempt.txt`, the
`build-serial` list; CLAUDE.md and AGENTS.md lines that name them.

## Found on the way

- [Issue 2306](../issues/2306-interrupted-submodule-checkout-reads-provisioned.md):
  an interrupted submodule checkout reads as provisioned. The runner's
  preflight (W5) is where its fix belongs.
- [Issue 2308](../issues/2308-sparse-zephyr-tree-reads-as-west-workspace.md):
  CI's sparse Zephyr header tree (`zephyr-tree`) passes the "west workspace
  provisioned" test (`zephyr`), so outside a scoped lane `template-copy-out`
  tries a real Zephyr build. Two capabilities share one path.
- `build/corrosion-cargo` and `build/sizes-probe` grew 51 -> 160 G and
  38 -> 93 G over five gate runs in one worktree, and filled a 1.8 T disk
  twice in a day. Nothing prunes them. W4's disk declarations and preflight
  need their growth bounded first. Why one run is that large: issue
  [2318](../issues/2318-cmake-leaf-recompiles-host-tooling.md) (every CMake
  leaf recompiles the same host tooling; ~80% of `corrosion-cargo` is host-side).

## Open questions

1. Lane names: `push` / `pr` / `gate` (by where a lane runs), or `fast` /
   `build` (today's)?
2. W0's two held scripts: delete, or wire into a lane?

## Acceptance

- `cargo nextest list -p nros-gates` names every gate in the inventory.
- The `gate` lane wrapper runs them all, with no gate passing by skipping, and the
  same set of verdicts as today's runner on the same tree.
- No lane dies of ENOSPC mid-run: it refuses first, naming the numbers.
- `run-gates-parallel.sh` and the authored lane lists are gone.

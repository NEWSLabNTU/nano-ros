# Multi-agent CI workflow

**Status: draft, 2026-08-28.** Written to be argued with. Nothing here is
implemented yet; the measurements are real, the design is a proposal.

Ten-plus agents work this repo concurrently. Each one runs a long local
verification before pushing, they collide on the same issues and phases, and the
GitHub CI that was supposed to catch the rest takes hours. This doc proposes a
shape that keeps the guarantees and removes most of the waiting.

## What is actually expensive

Measured on this tree, 2026-08-27/28:

| thing | size / cost |
| --- | --- |
| Zephyr SDK under `scripts/zephyr/` | **9.2 GB** (`sdk/` 7.8 + `downloads/` 1.4) |
| `actions/cache` limit | **10 GB per repository**, LRU |
| GitHub-hosted runner disk | ~14 GB |
| `just ci` (tier 1) end to end | 40–90 min, dominated by fixture builds |
| one C++ Cyclone fixture leaf | ~2m42s on a 32-thread machine |
| fixture manifest | 314 `[[fixture]]` rows |

The SDK does not fit in the cache and barely fits on the runner, so every heavy
hosted job re-provisions it. That is where the hours go — not in compiling our
own code.

A second cost is structural: `check-tier-preconditions` demands fixture
freshness unconditionally, so a **docs-only commit pays the same treadmill as an
RMW change**. Two commits in this session did exactly that.

## Runner topology

Split by what can physically run where.

| lane | trigger | runner | why there |
| --- | --- | --- | --- |
| **L0 source** | `pull_request` | hosted | buildless, seconds |
| **L1 unit** | `pull_request` | hosted | `actions/cache` on `target/` keyed by `hashFiles` — the pattern `nightly.yml` already uses for `packages/cli/target` |
| **L2 host-exec** | `pull_request` | hosted | host gcc only. **`ZephyrNativeSim` needs no SDK**, so three RTOS APIs fit on a hosted runner |
| **L3 cross-build** | `merge_group` | self-hosted | needs cross toolchains; the Zephyr SDK is 9.2 GB and cannot be cached |
| **L4 cross-run** | `push` to main, `schedule` | self-hosted | QEMU; auto-revert rather than block |
| **L5 interop** | `schedule` | self-hosted `nros-ros2` | needs a ROS install; isolated so its flakes poison nothing |
| **L6 hardware** | manual | self-hosted `nros-hw` | real boards |

The split is not arbitrary: everything a **hosted** runner can do is everything
that fits in 14 GB of disk without the SDK. That line falls exactly between L2
and L3.

### Runners behind NAT are fine

A GitHub Actions self-hosted runner **polls outbound over HTTPS**; nothing
connects to it. No inbound ports, no port forwarding, no static address. A
runner behind NAT needs only outbound 443 to `github.com`,
`api.github.com`, `*.actions.githubusercontent.com` and the artifact/cache
hosts. This is the normal deployment, not a workaround.

Consequences worth planning for: a NAT'd runner cannot be reached for debugging,
so it must ship its logs as artifacts; and it may be behind a slow uplink, so it
should be labelled for jobs whose cost is CPU rather than download.

### Labels, not hostnames

Register each runner with labels describing what it *has*, and target jobs by
label:

| label | means |
| --- | --- |
| `nros-sdk-zephyr` | the 9.2 GB SDK is provisioned and warm |
| `nros-qemu` | QEMU + the RTOS toolchains |
| `nros-ros2` | a real ROS 2 install for interop lanes |
| `nros-big` | ≥16 cores, for fixture fan-out |

Then `runs-on: [self-hosted, nros-sdk-zephyr]` rather than naming a machine.
Adding a second runner is registration plus labels; nothing in the workflows
changes.

### Capacity: one runner serialises the queue

L3/L4 on a single self-hosted runner makes that runner the queue's critical
path — every batch waits on it, and a 40-minute batch means a 40-minute
minimum latency no matter how small the change. Two runners with the same
labels halve that; the queue schedules across them with no workflow change,
which is the point of labelling by capability rather than hostname.

The hosted side has its own ceiling: a public repo on the Free plan gets 20
concurrent jobs. Ten agents each opening PRs that fan out L0/L1/L2 can reach it,
and jobs then queue invisibly. Fan out per *changed area*, not per platform, and
keep L0 a single job rather than one per gate.

### Security — this is a PUBLIC repo

GitHub advises against self-hosted runners on public repositories: a fork's pull
request can run arbitrary code on the machine. This is not theoretical and the
machines here also carry unrelated research work.

Rules, all of them:

1. Self-hosted jobs trigger **only** on `merge_group` and `push` to `main` —
   never on `pull_request` from a fork. Fork PRs get hosted runners only.
2. Require approval for first-time contributors.
3. Run the runner as a dedicated unprivileged user, ideally in a container or
   VM, never as the account holding other work.
4. Ephemeral runners (`--ephemeral`) so one job cannot leave state for the next.

## Scripts to own the procedure

Registration should be one command, not a wiki page.

| script | does |
| --- | --- |
| `scripts/ci/runner-register.sh <labels…>` | download the runner, `--ephemeral`, configure with a short-lived registration token, install the service, apply labels |
| `scripts/ci/runner-provision.sh <labels…>` | make the labels true — install the Zephyr SDK, QEMU, ROS 2, toolchains — reusing `nros setup` so a runner and a contributor provision the same way |
| `scripts/ci/runner-doctor.sh` | assert every label's claim actually holds; refuse to register a runner that lies about what it has |
| `scripts/ci/runner-sweep.sh` | reap orphaned process groups and stale build dirs between jobs |

`runner-doctor.sh` matters more than it looks: a runner labelled
`nros-sdk-zephyr` without the SDK produces a red that looks like a code failure.
It is the same class as the vacuous gates catalogued below.

`runner-sweep.sh` also owns **disk GC**, and that is not a detail: the
persistence that makes a self-hosted runner fast is the same property that lets
it rot. A 9.2 GB SDK, per-coordinate fixture trees, sccache and `build/` all
grow without bound, and a runner that fills its disk fails in ways that look
like code failures. Budget per label, evict LRU, and report the high-water mark.

`runner-sweep.sh` is not optional either. This session found **71 orphaned
`add_two_ints_server` processes**, oldest 10 days, each holding a DDS
participant; issue 0659 recorded 59 of the same in August. On a shared runner,
one leaked peer becomes every later job's flake.

## Lanes, re-arranged by execution cost

Today's tiers are cuts of a combinatorial matrix — 1-wise, pairwise, full. That
axis does not predict wall-clock. **Execution cost does**, and it partitions the
platforms cleanly:

| group | platforms | needs |
| --- | --- | --- |
| **host-executable** | `Linux`, `ZephyrNativeSim`, `ThreadxLinux`, `FreertosPosix` | nothing but a host compiler |
| **cross-build** | every RTOS target | cross toolchain (Zephyr targets: the 9.2 GB SDK) |
| **cross-run** | same, booted | the above + QEMU |
| **interop** | native + real ROS 2 | a ROS install and a router |
| **hardware** | `s32z270`, FVP | a board or a licensed model |

`ZephyrNativeSim` builds with `ZEPHYR_TOOLCHAIN_VARIANT=host` — **plain host
gcc, no SDK**. So the host-executable group carries roughly 50 runtime cells,
including three RTOS APIs, at host speed on a hosted runner. Today `just ci`
runs `NROS_TEST_SCOPE=native` and gets none of it.

### The lanes

| lane | contents | cost | where |
| --- | --- | --- | --- |
| **L0 source** | the 43 buildless gates | seconds | pre-commit, PR |
| **L1 unit** | affected crates: check, clippy, unit tests. No fixtures | 2–5 min | PR |
| **L2 host-exec** | the four host-executable platforms, all workloads | 10–20 min | PR / merge queue |
| **L3 cross-build** | compile + link + **symbol** checks for cross targets. No QEMU | 10–30 min | merge queue, affected platforms only |
| **L4 cross-run** | QEMU boot + e2e | hours | post-submit, tier-1 boards; nightly, the rest |
| **L5 interop** | real ROS 2 peers, router | slow + flaky | nightly, isolated |
| **L6 hardware** | real boards | manual | pre-release |

L5 gets its own lane specifically because it is the flakiest and would
otherwise poison merge-queue batches.

## Test methods: pick the cheapest witness per defect class

The important question is not "which platforms do we run" but "what is the
cheapest thing that catches this class of bug". Most of this repo's recurring
embedded defects do **not** need QEMU:

| defect class | recorded as | cheapest witness | lane |
| --- | --- | --- | --- |
| 32-bit layout / sizes mirror | 0088→0114→0122→0123→0245→0268 | one 32-bit **cross build** + static assert | L3 |
| linker sections, staticlib DCE | 0155, 0163 | `rust-rtos-link-check` | L3 |
| static RAM ceiling | phase-392 | **`just mem-report --check` on the cross ELF** | L3 |
| allocation on a no-alloc path | 0816 | `check-no-alloc-image` — `nm` the ELF | L3 |
| ABI / vtable drift | 0238, 0331 | static asserts + `check-rmw-abi-shape` | L0/L1 |
| RTOS API misuse (mutex pools) | 0129, 0139 | host-executable port | L2 |
| allocator provenance, cross-heap free | 0811 | symbol check, then one cross-run | L3, L4 |
| discovery, session, protocol | 0157, 0161 | host-executable port | L2 |
| priority inversion, starvation | 0623 | cross-run | L4 |
| WCET, real timing | — | hardware | L6 |

**The payoff: nearly every embedded-specific class has a build- or symbol-level
witness.** A 32-bit ELF that never boots still proves layout, linkage, static
RAM and allocation-freedom. QEMU is needed for scheduling, timing and real
transport behaviour — and for little else.

Two of those witnesses landed this week and are not yet wired into any lane:
`scripts/nros-mem-report.py --check` and `scripts/check-no-alloc-image.py`.
Pointing them at cross ELFs in L3 converts phase-392's whole campaign from a
manual measurement into a gate.

### Other method rules

1. **One witness per class, not the cross product.** 32-bit layout needs *one*
   32-bit target, not four. Platform *coverage* can be sampled; property
   *witnesses* cannot be, and must always run.
2. **Prefer compile-time to runtime.** `bound_fits`, the FFI mirror asserts and
   `const { assert!(...) }` cost nothing and cannot flake. This tree already
   does it well — extend it rather than adding runtime cases.
3. **Build-only is not second-class.** It is the whole of L3 and it catches the
   majority of the classes above.
4. **Every gate ships a self-test** proving it fails on what it forbids. One
   session found a `check-c` probe that had been vacuous for weeks, a walk gate
   reading ⅓ of the tree, and a `const` assertion of mine that could only be
   true.
5. **Quarantine flakes on sight**, do not investigate first.

## Board tiers: what they mean, and what they do not

`packages/boards/board-support.toml` defines tiers as *verification depth* —
tier 1 is "`just ci` exercises it", tier 2 is "nightly only". So they are
descriptive of CI wiring, and using them to *decide* CI membership would be
circular. Audience — who actually ships this — is a separate axis and is not
recorded anywhere today.

The ordering is nonetheless evidence-backed. Runtime cells per platform:

| platform | cells | kind |
| --- | ---: | --- |
| `FreertosMps2` | 19 | cross ARM |
| `ThreadxLinux` | 18 | host |
| `NuttxArm` | 17 | cross ARM |
| `ZephyrNativeSim` | ~14 | host |
| `ThreadxRiscv64` | 13 | cross RISC-V |
| `NuttxRiscv` | 9 | cross RISC-V |
| `ZephyrQemuCortexM` | 3 runtime (+4 build-only) | cross ARM |
| `FreertosPosix` | **2** | host |

Consequences worth acting on:

- **The host port does not cover the cross port, in either direction.**
  `ZephyrNativeSim` has more workloads and more RMWs than `ZephyrQemuCortexM`,
  which runs only pubsub at runtime — yet native_sim cannot see 32-bit layout,
  linker sections, the picolibc arena, or any static-RAM ceiling. And issue 0589
  is native_sim-*only*. Overlap, not containment. Same for ThreadX
  (`ThreadxLinux` vs `ThreadxRiscv64`). NuttX has no host port at all.
- **`FreertosPosix` is a two-cell smoke**, not a verified platform. FreeRTOS is
  the inverse of Zephyr/ThreadX: its cross port is the rich one.
- **RISC-V earns one lane, not two.** 22 cells across two platforms, and it is
  the only non-ARM/non-x86 architecture — worth keeping as the witness that the
  platform layer is not accidentally ARM-shaped. `NuttxRiscv` mostly re-tests
  `NuttxArm`'s RTOS on a second arch; `ThreadxRiscv64` has more cells.
- **`QemuBaremetal` is the no-RTOS `no_std` witness** and currently is not
  delivering that: issue 0816 found all 13 bare-metal Rust leaves enable
  `alloc`. Either give it a genuinely no-alloc fixture and wire
  `check-no-alloc-image` at it, or drop it — today it costs without proving.
- **`Esp32Qemu` at tier 2 = nightly**, which matches the stated ESP-IDF policy.
  Consistent; it should not grow.

## Landing: batch, do not serialize

Use GitHub's **native merge queue** (branch protection + the `merge_group`
event). It batches, tests the *speculated* trunk rather than each PR's stale
base, and ejects failures. Public repos have it at no cost; no third-party tool
is needed for this shape.

- Batch size 4 to start. Batches of 4 mean roughly a quarter as many CI runs at
  the cost of one extra bisection round.
- **Partition the queues by path** — docs / platform-port / rmw / core. A single
  queue becomes the bottleneck at high PR rates; partitioning is the documented
  fix and maps onto `paths:` filters.
- T3 stays post-submit with culprit-finding and auto-revert. At hours-long
  latency, reverting beats blocking.

This directly fixes the reported problem. Agents stop running the expensive
tiers locally, and no agent's green is invalidated by another's push, because
the queue tests the speculated trunk.

## Flake quarantine is a prerequisite, not a follow-up

A batch red is ambiguous between a real defect and a flake, and bisection
amplifies the cost. Quarantine means the test **still runs and still records**
but no longer blocks; it does not mean deleted.

Today's candidate: `action_raw_goal_ships_one_cdr_header` — 60 s timeout
in-sweep, 3.6 s solo, 5/5 passes. A 16× margin, load-induced.

`_nextest-tolerant` and the skip budget are most of the mechanism already. What
is missing is a registry and the does-not-block half.

## Agent protocol

1. **Claim** the work: `refs/claims/issue-NNNN` carrying agent id + lease, using
   the same atomic-ref trick as `just issue-new`. Renew while working; the claim
   lapses if the agent dies. A shared markdown task list has the lost-update
   race that `issue-new` was written to fix — reuse the ref.
2. **Isolate**: one git worktree per agent, always.
3. **Verify locally: T0 + T1 only.** No agent runs `just ci`.
4. **Push a branch, open a PR, enqueue.** Never push to `main`.
5. The queue owns L3; `main` owns L4.

**The local verb has to change too, or none of this happens.** CLAUDE.md
currently instructs every agent to "run the TIER your change earns" and names
`just ci` / `ci-matrix` / `ci-full`. Agents follow that faithfully — this session
ran the full treadmill four times. If the lanes land without redefining those
verbs, agents will keep paying the old cost and the queue will re-test what they
already ran. Redefine `just ci` as **L0+L1+L2** (the honest pre-push check),
keep `just ci-full` for the pre-release sweep, and update CLAUDE.md in the same
change — not afterwards.

## Shared hotspots to remove

Parallel agents collide on shared registries. Ours:

- `docs/issues/README.md` — a hand-maintained index of files that already carry
  frontmatter. It conflicts on nearly every rebase, and its gate compares
  distinct ids against files, so it cannot see a duplicate: `main` currently
  carries **two open `#0824` rows** and reports OK. **Generate the open list**,
  the way the pool inventory and RMW matrix are generated.
- The three numbered series are already race-free for *ids* thanks to
  `issue-new` / `phase-new`; it is the indexes that hurt.

## Gates must be able to fail

In one session this tree produced: a `check-c` expected-failure compile that
passed for two weeks because its probe file did not exist; a walk gate that
scanned three directories while stating a repo-wide rule; and a `const`
assertion of mine whose only claim was `min(a, b) <= b`. In a batched world a
vacuous gate is worse than none — it makes green cheaper *and* emptier. Every
gate ships with a self-test that proves it fails on the thing it forbids.

## Migration: what each current recipe becomes

The lanes are not new machinery so much as a re-cut of what exists.

| today | becomes | change needed |
| --- | --- | --- |
| `just check-fast` (43 gates) | **L0** | none |
| `just check-build` (20 gates) | **L1** | narrow to affected crates |
| `just ci` (`NROS_TEST_SCOPE=native`) | **L2** | widen scope from `native` to the four host-executable platforms; stop requiring a full fixture build |
| `just rust-rtos-link-check` | **L3** | join it with `mem-report --check` and `check-no-alloc-image` on the same cross ELFs |
| `just ci-matrix` (1-wise) | **L4** | select by board tier + change footprint instead of 1-wise |
| `just ci-matrix-nightly` (pairwise) | **L4 nightly** | unchanged in spirit |
| `just ci-full` | **L6 / pre-release** | unchanged |
| interop cells (`interop::CELLS`) | **L5** | split out of the general sweep |

New recipes the workflow files above assume, none of which exist yet:
`just ci-l1`, `just ci-l2`, `just ci-l3`, `just ci-l4-tier1`, and `just claim`.
Adding them is the real work; the YAML is thin by convention
([ci-workflow-reorg.md](ci-workflow-reorg.md)) and stays that way.

The two structural edits are: **L2 stops requiring fixtures it does not use**,
and **L3 becomes a real lane** rather than a single link check — because that is
where most embedded defect classes are cheapest to catch.

## Setting it up on GitHub

Three things to configure, all scriptable. This assumes the conventions in
[ci-conventions.md](ci-conventions.md) — a runner is a fresh clone with no
submodules, no ROS, no SDKs — and the thin-`just`-caller rule from
[ci-workflow-reorg.md](ci-workflow-reorg.md): workflow YAML calls recipes, it
never inlines logic.

### 1. Branch protection + merge queue

```sh
OWNER=NEWSLabNTU REPO=nano-ros

# Required checks are the HOSTED lanes only. L3 runs inside the queue and
# reports through the merge_group run, so it must NOT be listed here.
gh api -X PUT "repos/$OWNER/$REPO/branches/main/protection" \
  --input - <<'JSON'
{
  "required_status_checks": {
    "strict": false,
    "contexts": ["l0-source", "l1-unit", "l2-host-exec"]
  },
  "enforce_admins": false,
  "required_pull_request_reviews": null,
  "restrictions": null
}
JSON
```

Then enable the queue itself (Settings → Branches → `main` → *Require merge
queue*), or by ruleset:

| setting | value | why |
| --- | --- | --- |
| merge method | rebase | the repo requires linear history |
| maximum PRs to build | **4** | one batch ≈ a quarter of the CI runs |
| minimum PRs to merge | 1 | do not stall a quiet queue |
| wait time to meet minimum | 5 min | lets a batch actually form |
| status check timeout | > L3's p99 | a timeout is indistinguishable from a red |

`strict: false` matters. "Require branches up to date" forces every PR to rebase
before merging, which is precisely the rebase-race this design removes — the
queue tests the speculated trunk instead.

### 2. Register a runner (works behind NAT)

`scripts/ci/runner-register.sh`, in outline:

```sh
#!/usr/bin/env bash
# usage: runner-register.sh <label>[,<label>...]
set -euo pipefail
LABELS="${1:?labels required}"
OWNER=NEWSLabNTU REPO=nano-ros

scripts/ci/runner-doctor.sh "$LABELS"      # refuse to lie about capabilities

TOKEN="$(gh api -X POST "repos/$OWNER/$REPO/actions/runners/registration-token" \
         --jq .token)"

./config.sh \
  --url "https://github.com/$OWNER/$REPO" \
  --token "$TOKEN" \
  --labels "self-hosted,linux,$LABELS" \
  --ephemeral \
  --unattended \
  --replace
sudo ./svc.sh install "$RUNNER_USER" && sudo ./svc.sh start
```

Only outbound 443 is used — the runner long-polls GitHub. Nothing needs to
reach it, so NAT, DHCP and a laptop uplink are all fine. The registration token
is short-lived (about an hour) and is *not* a secret to store.

`--ephemeral` means the runner takes one job and de-registers; a supervisor
re-registers it. That is what stops one job leaving state for the next, and it
is not optional on a public repo.

### 3. Runner roles

Register each machine for what it has, not what it is:

```sh
scripts/ci/runner-register.sh nros-qemu,nros-sdk-zephyr,nros-big   # builder
scripts/ci/runner-register.sh nros-ros2                            # interop
```

## The workflow files

Four files, replacing the current five. Each job is a thin `just` caller.

**`pr.yml`** — hosted, every PR. This is the whole pre-merge gate.

```yaml
name: pr
on:
  pull_request:
concurrency:
  group: pr-${{ github.event.pull_request.number }}
  cancel-in-progress: true          # supersede an agent's own re-push
jobs:
  l0-source:
    runs-on: ubuntu-22.04
    steps:
      - uses: actions/checkout@v4
      - run: just check-fast         # buildless; no submodules needed

  l1-unit:
    runs-on: ubuntu-22.04
    steps:
      - uses: actions/checkout@v4
      - uses: actions/cache@v4
        with:
          path: target
          key: l1-${{ runner.os }}-${{ hashFiles('Cargo.lock', 'packages/**/*.rs') }}
      - run: just ci-l1              # affected crates only

  l2-host-exec:
    runs-on: ubuntu-22.04
    steps:
      - uses: actions/checkout@v4
        with:
          submodules: false          # init only what the lane needs
      - run: just ci-l2              # Linux + native_sim + threadx-linux + freertos-posix
```

`l2` needs the Zephyr *sources* for native_sim but **not** the SDK — it builds
with host gcc. That is the whole reason this lane fits on a hosted runner.

**`queue.yml`** — self-hosted, merge queue only.

```yaml
name: queue
on:
  merge_group:
jobs:
  l3-cross-build:
    runs-on: [self-hosted, linux, nros-sdk-zephyr]
    steps:
      - uses: actions/checkout@v4
      - run: just ci-l3              # cross build + link + mem-report + no-alloc
      - if: always()
        uses: actions/upload-artifact@v4
        with: { name: l3-logs, path: tmp/ci-*.log }
```

The upload is not decoration: a NAT'd runner cannot be reached, so its logs must
leave with the run or the failure is undiagnosable.

**`post-submit.yml`** — self-hosted, after landing.

```yaml
name: post-submit
on:
  push:
    branches: [main]
jobs:
  l4-cross-run:
    runs-on: [self-hosted, linux, nros-qemu]
    steps:
      - uses: actions/checkout@v4
      - run: just ci-l4-tier1        # tier-1 boards only
```

**`nightly.yml`** — the full L4 matrix plus L5 interop, on schedule. L5 stays
here and nowhere else, because it is the flakiest lane and must never gate a
batch.

## What a contributor or agent actually does

```sh
# 1. Claim the work — atomic, race-free, and it expires if the agent dies.
just claim issue-0827

# 2. Isolate.
git worktree add ../nros-0827 -b fix/0827

# 3. Work, then verify locally. `just ci` is now L0+L1+L2 — minutes, no fixtures,
#    no cross toolchain. This is the ONLY verification an agent runs.
just ci

# 4. Push a branch and open a PR. Never push to main.
git push -u origin fix/0827
gh pr create --fill

# 5. Enqueue and walk away.
gh pr merge --auto --rebase
```

From there nothing is the author's problem:

| stage | who | on failure |
| --- | --- | --- |
| PR lanes L0–L2 | hosted runners | author fixes; queue never sees it |
| merge queue L3 | self-hosted | queue ejects the PR, re-forms the batch, notifies the author |
| landed | — | — |
| post-submit L4 | self-hosted | culprit-find, auto-revert, issue filed against the author |

**What changes for an agent, concretely:** it no longer runs `just ci-matrix` or
`ci-full`, no longer rebuilds fixtures before pushing, and no longer rebases to
chase a moving `main`. It claims, works, runs one fast verb, and enqueues. The
expensive tiers become someone else's asynchronous problem, which is the point —
today ten agents each pay for them serially.

**What still requires judgement:** a red in L4 post-submit lands on `main` and
gets auto-reverted. Agents must treat an auto-revert as a first-class signal —
reclaim the issue, reproduce locally against the reverted commit — rather than
re-pushing the same change.

## Sizing: a one-line fix and a phase wave are not the same change

Agent output ranges from a typo in an issue file to a multi-wave phase touching
twenty files across three crates. Routing them identically is what makes ten
agents starve each other — most of a day's output is docs and small fixes, and
none of it should queue behind a Cyclone build.

| class | looks like | claim | lanes | queue |
| --- | --- | --- | --- | --- |
| **express** | docs, comments, an index row, a typo — footprint ∅ | none | L0 | own partition, batch 16, lands in minutes |
| **minor fix** | one issue, one or two crates | `refs/claims/issue-NNNN` | L0–L2 | normal partition, batch 4 |
| **phase wave** | many files, may touch `packages/core`, may add fixtures | `refs/claims/phase-NNN-WN` | L0–L3 | own batch; full-footprint changes serialise |

Three consequences worth stating.

**Express is the pressure valve.** A docs-only change has an empty coordinate
footprint, so it needs L0 and nothing else. Path filters already give this
almost free (`pr-checks.yml` has a `changes` job today). Without it, ten
doc-filing agents contend for the same expensive lane they never needed.

**A phase wave lands as several PRs, not one.** Today's phase-392 W3 went out as
four commits — the bound helper, the routing fix, the macro, the doc — each
independently green. That is the shape to encourage: a wave is a *sequence of
landable increments*, and each one that lands is one that can no longer be
invalidated by someone else's push. The alternative, a single 20-file PR, is
maximally exposed to every conflict and every flake in one shot.

**Stacked waves are where GitHub's queue is weakest.** W3b depends on W3a; the
native merge queue has no notion of a stack, so the options are: land W3a first
and let W3b rebase (simplest, costs one queue cycle of latency), or keep the
dependency inside one PR. Prefer the former, and note that this is exactly
finding 1 below — the claim tool should know the wave order so an agent is not
asked to discover it by compile error.

**Review scales with class, not with CI.** An express change needs no human
eye; a phase wave that changes an ABI or a public API does, because the failure
mode of agent work is confident-and-wrong rather than broken, and no amount of
green catches that. Three retractions in this session — a wrong root cause in
#0844, an over-broad staleness rule, a `const` assertion that could only be true
— all passed every gate they were subject to.

## How it behaves under concurrency

A tabletop run of the design against eight scenarios, drawn from what actually
happened in one session rather than invented. It holds in three and breaks in
five. Findings 2, 3 and 8 are grounded in observed events; 6 and 7 are reasoned
from the mechanism and have not been seen.

### Where it holds

**Disjoint topics.** Two agents on #0810 and #0814. Both claim, both run L0–L2
locally in minutes, both enqueue, **one** L3 run covers both. That is the
batching payoff.

**Semantic conflict — the case that justifies the whole design.** Agent A
changes `alloc_payload_block`'s routing; Agent B adds a caller in another file.
Textually disjoint, so per-PR CI passes both and `main` breaks. The queue
compiles them *together* and catches it. Testing each PR against its own base
cannot.

**A flake inside a batch** — survivable, but only because quarantine lands
first. Without it, one `action_raw_goal` timeout ejects and re-tests four
innocent PRs.

### Where it breaks

**1. Claims carry no dependency edges.** Agent A claims phase-392 W3a, Agent B
claims W3b. Both succeed — different refs — but W3b's macro needs W3a's
`max_serialized_bound`, and B branching from `main` finds it absent. B holds a
valid claim on blocked work. Doing W3a→W3b sequentially in one session hides
this completely.

*Fix:* `just claim` refuses a wave whose predecessor is claimed-and-unlanded, or
wave order becomes machine-readable in the phase doc.

**2. Claims cover PLANNED work; most collisions are INCIDENTAL.** Two agents
independently wrote the same two missing `check-c` test TUs, and separately both
added the same `#0824` index row. Neither was anyone's assigned issue — both
were reds encountered while doing something else, so no claim would ever have
been taken.

*Fix:* an incidental fix goes out as its own tiny PR, enqueued immediately, so
the second agent rebases onto it instead of rewriting it. Agents check for an
open PR touching the file before fixing a red they stumbled into.

**3. Auto-revert versus long-lived branches.** L4 goes red at commit `X`;
auto-revert lands `X'`. Agent C branched from `X` hours earlier, so C's branch
still *contains* the reverted change. When C enqueues it silently re-introduces
it — and if the defect was an L4 runtime failure, L3 cannot see it and it
re-lands.

*Fix:* short-lived branches are load-bearing here, not a style preference. The
queue should also reject a diff that re-introduces reverted content.

**4. One expensive PR sets the batch's cost.** A `packages/core` change has
footprint = everything; batched with seven cheap PRs, all eight wait for a full
cross build.

*Fix:* batch by **footprint similarity**, not arrival order, and route
full-footprint changes to a serial lane.

**5. The shared index breaks batching outright — not just one PR.** Three agents
file issues concurrently. Ids are race-free thanks to `issue-new`, but all three
edit `docs/issues/README.md`. A merge queue needs the batch to merge cleanly, so
a textual conflict there means **the batch cannot form at all**. With ten
doc-heavy agents this is the common case, not the corner case.

This is what upgrades "generate the issue index" from a cleanup to a
**prerequisite**: batching does not function while a shared registry is
hand-edited.

## Telemetry, and when to abandon this

Every knob here — batch size, which lane a test belongs in, whether a runner is
saturated — needs data that is not currently recorded. Emit per-lane wall-clock
and per-test duration as a build artifact from the start; tuning batch size by
guesswork is how a merge queue becomes the new bottleneck.

Explicit exit criteria, so this is falsifiable rather than a one-way door:

- If median time-to-land does not fall below the current local-verification
  time, the queue is not paying for itself — go back to direct pushes.
- If batch reds are more often flakes than defects, quarantine is failing;
  stop batching until it is fixed, because bisection multiplies that cost.
- If the self-hosted runner becomes a single point of failure for landing
  anything, move L3 back to hosted and accept a narrower L3.

## Open questions

1. **What fraction of the 1541 L2 tests actually need a fixture?** If most do
   not, L1/L2 is a large cheap win available before any runner work. Half an
   hour to measure; it decides the sequencing.
2. Wall-clock split of a current PR run: provisioning vs building vs testing.
   If provisioning dominates — which the 9.2 GB SDK against a 10 GB cache cap
   suggests — one self-hosted runner recovers most of the hours by itself.
3. How many runners, and which takes the `nros-ros2` interop role? L5 is the
   flakiest lane and wants to be isolated from the queue.
4. Is `scripts/zephyr/sdk/` the right home for a 9.2 GB SDK at all? It sits
   inside a directory that scripts recursively scan; that already cost 37
   minutes once (issue 0844). `build/` or `~/.nros/sdk/` — where the other SDKs
   already live — would be the consistent choice.
5. Does `NuttxRiscv` earn its 9 cells alongside `ThreadxRiscv64`'s 13, or is one
   RISC-V witness enough?
6. Does `QemuBaremetal` get a genuinely no-alloc fixture (closing 0816's
   remaining half), or does it get dropped? Today it proves neither.
7. **187 `check-*` recipes exist; 63 are wired into `just check`.** Some of the
   remainder are sub-recipes of those, but not all. Which are reachable from no
   lane at all? An unwired gate is indistinguishable from a deleted one, and
   this tree has already shipped gates that could not fail.

## Sequencing

Ordered so each step pays for the next.

0. **Generate `docs/issues/README.md`'s open list from frontmatter.** Moved to
   the front by the concurrency run above: a hand-edited shared registry stops
   batches from forming at all, so nothing downstream works without it.
1. **Widen L2 to the host-executable platforms and stop requiring fixtures for
   it.** No new infrastructure; it is the largest coverage gain available today,
   and it runs on hosted runners.
2. **Wire L3**: `rust-rtos-link-check` + `mem-report --check` +
   `check-no-alloc-image` over cross ELFs. Converts phase-392 into a gate and
   catches most embedded classes without QEMU.
3. **Path-filter the workflows** so docs-only and source-only changes skip
   everything heavier than L0/L1.
4. **Register one self-hosted runner**, isolated, `merge_group` + `push` only.
   Unlocks L3/L4 at reasonable cost.
5. **Turn on the GitHub merge queue**, batch size 4, partitioned by path.
6. **Flake quarantine** — before trusting any batch red.
7. **Content-addressed fixture cache** on the runner's persistent disk.
8. **Claim refs** for agents — with predecessor checking, per finding 1, and
   paired with the incidental-fix rule from finding 2.

Steps 1–3 need no runner, no queue, and no new services. They are worth doing
even if nothing else here is adopted.

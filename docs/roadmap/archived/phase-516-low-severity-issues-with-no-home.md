# Phase 516 — Open issues whose phases were archived

**Status (2026-10-26). Archived: every issue homed here is closed.** Each
item below was one open issue whose phase was archived while it stayed open.
W1, W2, W3, W4, W6 and W7 are done. W5 is closed `wontfix` while ESP32 is
dormant. W1d and W1e did not land and moved to
[phase-519](../phase-519-rmw-daemons.md) (W1, W2), which owns router-side and
agent-side work.

**Prior:** [phase-513](../phase-513-nightly-reds-with-no-home.md), which homed the
medium-severity reds from the same sweep and left these unhomed on purpose.
This phase gives each of them a home, so every open issue names an active phase
that names it back.

---

## Why a phase

On 2026-10-25, five open issues named no active phase. Each had a home once:
phase-464, phase-395 (two), phase-400 and phase-394. Each of those phases was
archived when its own work landed. An issue with no phase is read by nobody
planning work, so it stays open by default rather than by decision. Each item
here ends in a fix or in an explicit `wontfix`.

## Work items

### W1 — the router `downsampling` rule ([issue 0506](../../issues/archived/0506-transport-band-unbounded-preemption.md))

The device-side half of the ingress budget landed in phase-462. What is left is
RFC-0074's enforcement point 1: a router `downsampling` rule the resolver emits
from a subscription's declared ingress and that is deployed with the router's
config. It is the only mechanism measured to save the device's CPU rather than
bound it. Nothing in the tree emits one yet, and no phase owns router-side
lowering.

**Done when** the resolver emits the rule for a declared ingress, and a cell
shows a router running it drop the excess before it reaches the device.

Design: RFC-0074 "As lowered" (amendment 2026-10-26). The router can carry the
rate and not the burst, a rule cannot select one peer, and co-subscribers on a
topic take the lowest rate. Split:

- [x] **W1a — the producer.** `nros sync` writes
      `<model-stem>.router.json5` beside each resolved model whose subscribers
      declare an ingress: one `downsampling` item, one rule per
      (topic, domain), keyexpr `<domain>/<topic>/<dds type>/*`,
      `freq = rate_hz` (lowest across co-subscribers). No declaration, no
      file; a stale one is deleted. Untargeted unless an image's transport is
      serial (`link_protocols: ["serial"]`). Only zenoh images contribute.
- [x] **W1b — the deploy door.** `nros ws router-config --model <path>` prints
      the rule as a `ZENOH_CONFIG_OVERRIDE` string (or the json5), and every
      `just <platform>::zenohd <model>` runs it (`nros_router_exec`'s second
      argument). Measured on `rmw_zenohd` (Humble, zenoh 1.10.1): a 500 Hz
      `ros2 topic pub` through the router to a `ros2 topic hz` declared at
      50 Hz read 47.1-47.5 Hz with the rule and 500.0 Hz without, and the
      router logged its drop line. Both host nodes had to be in CLIENT mode:
      in rmw_zenoh's default peer mode they linked directly and the rule saw
      nothing (500 Hz with it).
- [x] **W1c — the cell.** `interop_e2e`'s `zenoh_pubsub_router_downsampling`
      (Linux, Rust, Zenoh, Pubsub — an existing `interop::CELLS` coordinate):
      the rule is lowered the way `nros sync` lowers it (model + `system.toml`
      on disk, `router_rule::write_beside`, override read back from the
      artifact) and handed to the router through the new
      `ZenohRouter::start_unique_with_overrides`. A 200 Hz `ros2 topic pub` to
      the native listener declared at 20 Hz: 113-114 samples in 6 s with the
      rule (cap 120) and the router's drop line logged, 1199-1200 without, over
      three runs; with the rule removed the cell is red. Closes issue 0506.
- **W1d — a stated interface.** Deferred, moved to
      [phase-519](../phase-519-rmw-daemons.md) W1 on 2026-10-26. It matters
      only on a router shared with other consumers, and nothing in the tree
      deploys one.
- **W1e — upstream.** Deferred, moved to
      [phase-519](../phase-519-rmw-daemons.md) W2 on 2026-10-26. A maintainer
      files it.

### W2 — the api-parity ledger stops conflicting ([issue 1091](../../issues/archived/1091-ledger-shards-conflict-on-sorted-key-insert.md))

Two branches that add adjacent keys to a sorted ledger shard always conflict.
The issue names the layout that does not. Land the open ledger PRs first, then
migrate in one commit against a quiet tree.

**Done when** the issue's acceptance holds: two such branches merge with no
conflict, `just check api-parity` is green, and its `--self-test` still rejects
a row in the wrong stage.

**Done (2026-10-26).** The ledger is one file per row,
`docs/reference/api-parity-ledger/<topic>/<row file>.json`, read and written
only through `scripts/api_parity/ledger_files.py`. The directory is the topic,
so the shard identity survives. The file name is derived from the row's own
`key`, so a duplicate key collides in git instead of surviving. The migration
moved 3169 rows and was lossless against origin/main's loader. It also found
`cpp:spin_once` filed in two shards, which the old loader had merged
silently. Two branches adding adjacent keys now rebase and merge clean, where
the old `pubsub.json` conflicted. `just check api-parity` is green. The
self-test now plants a row under the wrong topic directory and requires the
misfiling, mutation-tested. Measurements are in the issue's Resolution.

### W3 — `check-doc-commit-citations` reads history, not the object store ([issue 1205](../../issues/archived/1205-doc-commit-citations-local-fetch-state.md))

The gate resolves a hash against whatever the clone has fetched. A fetched PR
branch therefore makes it demand removal of baseline entries that every other
clone still needs. Ask `git merge-base --is-ancestor <hash>` against the default
branch, keep the shallow-clone skip, and give the gate a verdict in CI.

**Done when** a run with a fetched PR branch and a run without one agree, and
the issue's second measurement has a lane that runs the gate.

**Done (2026-10-26).** The gate resolves against the default branch's history,
so a fetched branch changes no verdict. CI already unshallows the base branch
for it (`gate.yml`, commits only). The baseline shrank from 16 entries to 2.
Issue 1205 resolved.

### W4 — the seven hand-set sizing knobs ([issue 1808](../../issues/archived/1808-sizing-knobs-with-no-static-derivation.md))

phase-412 derived every pool the entity inventory can answer. Seven knobs on
the safety island are still set by hand, each with its own blocker.

**Done when** each knob is either derived from what the image declares, or
recorded as permanently operator-stated with its reason in the knob's help
text.

**Done (2026-10-26).** Three derived (the Zephyr task slots, and both XRCE
service buffers, after XRCE gave the parameter and lifecycle services slots of
their own), three ruled permanently operator-stated with the measurement named
in their help (the Zephyr heap, the task stack, `CONFIG_MAIN_STACK_SIZE`), and
the graph cache was already closed. Issue 1808 resolved.

### W5 — esp32-qemu heap accounting ([issue 1823](../../issues/archived/1823-esp32-qemu-heap-used-bytes-reports-zero.md)) — CLOSED `wontfix`

`heap_used_bytes` / `heap_total_bytes` report 0 on esp32-qemu. ESP32 is dormant
([issue 1525](../../issues/archived/1525-esp32-c-cpp-has-no-build-road.md)), so do
not work this until it is revived.

**Done when** both methods read the port's heap, the way mps2-an385 does, or
the issue is closed `wontfix` because ESP32 was retired.

**Closed `wontfix` (2026-10-26)** by maintainer decision, while ESP32 is
dormant rather than retired. Reopen issue 1823 if ESP32 is revived.

### W6 — C++ runner status lines logged at ERROR ([issue 2049](../../issues/archived/2049-cpp-levels-runner-info-lines-logged-as-errors.md))

Filed by phase-162, which was archived on 2026-10-25 with it open. The C++
tier and level runners print "multi-tier run", "multi-level run" and "level
budget ... enforced by" through `cpp_diag!`, which is `log_error!`, so the
boot report counts every multi-level boot as an error.

**Done when** those lines log at INFO and a `levels-cpp` `linux_s1` boot
reports no error.

**Done (2026-10-25).** A `cpp_info!` beside `cpp_diag!` logs the three at
INFO; a `linux_s1` boot prints no `ERROR`. Issue 2049 resolved.

### W7 — the spin-quantization warning on a spawned level's timer ([issue 2050](../../issues/archived/2050-spin-quantization-warning-names-a-spawned-level-timer.md))

Also filed by phase-162. The audit warns that `attitude`'s 1 ms timer runs in
10 ms bursts against a spin period that is neither level's. phase-162's run
under `SCHED_FIFO` showed the timer keeps 1 kHz, so the warning is the wrong
half.

**Done when** the issue's own condition holds: the audit judges each timer
against its level's spin period, and a `linux_s1` boot prints no warning for
`attitude`.

**Done (2026-10-25).** The audit walks only its own worker's band, so the boot
level no longer judges a spawned level's timer; a `linux_s1` boot prints no
quantization warning. Issue 2050 resolved.

## Issues homed here

| Issue | Item | Severity |
| --- | --- | --- |
| [0506](../../issues/archived/0506-transport-band-unbounded-preemption.md) | W1 | unrated |
| [1091](../../issues/archived/1091-ledger-shards-conflict-on-sorted-key-insert.md) | W2 | unrated |
| [1205](../../issues/archived/1205-doc-commit-citations-local-fetch-state.md) | W3 | unrated |
| [1808](../../issues/archived/1808-sizing-knobs-with-no-static-derivation.md) | W4 | low |
| [1823](../../issues/archived/1823-esp32-qemu-heap-used-bytes-reports-zero.md) | W5 | low |
| [2049](../../issues/archived/2049-cpp-levels-runner-info-lines-logged-as-errors.md) | W6 | low |
| [2050](../../issues/archived/2050-spin-quantization-warning-names-a-spawned-level-timer.md) | W7 | low |

## Acceptance

- [x] Each issue above is resolved, closed `wontfix` with its reason, or moved
      to a phase that owns its work.

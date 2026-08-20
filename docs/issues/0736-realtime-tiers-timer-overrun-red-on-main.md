---
id: 736
title: "`realtime_tiers` fails SOLO and passes in the sweep — two different
  assertions in two days, neither of them a clean flake"
status: open
type: bug
area: testing, core
related: [phase-359, issue-0623, issue-0636]
---

## Two failures, recorded in the order they were seen

**Form 1 (2026-08-20, before rebasing onto ~19 upstream commits):** the runtime
timer contract.

```
[WARN] nros: contract violation: timer-overrun-runtime timer measured=1 declared=0
[WARN] nros: contract violation: timer-overrun-runtime timer measured=2 declared=0
```

3/3 solo, ~47 s per run. `declared=0` means the entry baked no runtime for that
timer, so any measurement over zero violates — the monitor reporting "you told
me this takes no time and it took 2".

**Form 2 (2026-08-21, after the rebase and a native fixture rebuild):** form 1
is gone and a different row fails.

```
realtime_tiers: 1 of 16 row(s) FAILED:
  nuttx-arm/rust: high-tier /ctrl counter 2 is not >= 3x the low-tier /telem
  counter 20 — the 10 ms tier is not outrunning the 100 ms tier
```

Also 3/3 solo, also ~47 s. `/ctrl` received 3 messages; `/telem` received 21.

## The part that is stranger than either failure

**It PASSES in the full sweep** — 21.6 s, green, in the same `just ci` run
whose junit reports one unrelated real failure. Solo it fails 3/3 at 47 s.
That is the reverse of this repo's usual QEMU story (CLAUDE.md: retest a QEMU
red SOLO before filing, because in-sweep lanes flake under load), so the normal
diagnosis does not apply and neither does dismissing it as load.

The 47 s vs 21.6 s split is the thing to chase first: the same binaries take
twice as long solo, which means the solo run is doing something the sweep is
not — or waiting for something the sweep has already warmed.

## Confounder that is live and must be cleared first

The `nuttx-arm/rust` row's fixture is NOT rebuilt by `just build-test-fixtures
lane=native`, so every run above executed a binary built before this tree's
current core. That is the museum-binary condition CLAUDE.md warns about, and
until the nuttx lane is rebuilt (`just nuttx build-fixtures`, or a tier-2
build) form 2 cannot be attributed to anything.

Do that before theorising about the tier ratio.

## Not caused by phase-359 W10

Form 1 was checked directly: stash the `packages/core` + `packages/api` edits,
`just setup-cli`, rebuild native fixtures from upstream `main`, run — failed
identically on the untouched tree.

## Reproduce

```
just build-test-fixtures lane=native
cargo nextest run -p nros-tests --test realtime_tiers_e2e --retries 0   # fails
just ci                                                                # passes
```

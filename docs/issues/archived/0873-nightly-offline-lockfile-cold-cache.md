---
id: 873
title: "All three nightly platform jobs fail on `generate-lockfile --offline` against
  a cold registry — an infrastructure fault reported as platform overclaim"
status: resolved
type: bug
area: ci
related: [issue-0863, issue-0676, phase-395]
---

## Problem

`threadx_linux`, `nuttx` and `freertos` fail in every scheduled `nightly` run,
and all three fail the same way:

```
error: no matching package named `panic-semihosting` found      # threadx, nuttx
error: no matching package named `esp-backtrace` found          # freertos
note: offline mode (via `--offline`) can sometimes cause surprising resolution failures
Error: configure failed: `NROS_CARGO_FLAGS= cargo generate-lockfile --offline` exited status 101
```

Same signature as issue 0863 one lane over: **a cold cargo registry cache makes
`--offline` resolution fail during SELECTION**, with no download attempted.

## Why this is not the platforms' fault, and why that matters

`just tier-health` reports seven platforms as OVERCLAIMED, five of them because
this lane is red. That reads as "these platforms are not supported to the tier
they claim" when the truth is "our CI cannot provision the lane that would prove
it".

The distinction decides the remedy, and getting it backwards is expensive: the
tier policy this repo adopted from Rust says demotion is a deliberate act with a
record, precisely because auto-demoting on a red is the pressure that makes
people silence tests. Demoting five platforms for a cargo cache would be exactly
that mistake, and it would record something false about the platforms.

## Mechanism

`nros build` runs `cargo generate-lockfile --offline` when the workspace root is
GENERATED (`cmd/build.rs`), and `--offline` is deliberate — issue 0676 wants the
build frozen.

The failure is not about the platform being built. `examples/workspaces/rust` is
ONE cargo workspace, so resolving it resolves EVERY member — including
`esp32_entry`, which needs `esp-backtrace`. A freertos build therefore fails on
an esp32 dependency it never uses, purely because they share a workspace root.

## What has NOT been established

- **Whether a warm cache hides it entirely.** A developer host has these crates
  from earlier builds, which is consistent with this being CI-only, but no one
  has run the lane on a deliberately cold cache to confirm that is the only
  difference.
- **The right fix.** Several are plausible and they differ in what they give up:
  a `cargo fetch` before the offline step (keeps the frozen property, costs
  network at a defined point); vendoring; splitting the workspace so a platform
  build does not resolve unrelated members. Removing `--offline` is the one
  option that is clearly WRONG — it discards what 0676 bought.

## Not to do

Do not demote the affected platforms in `board-support.toml`. The claim is not
what is broken.

## Fix — the offline step escalates instead of failing

`cmd/build.rs::run_configure` runs the configure step, and on failure
retries it ONCE with `--offline` dropped (`Handoff::without_offline`,
`None` when the step never asked for offline — so nothing re-runs an
identical command and reports the second failure as new information).

The reasoning that picks this over the alternatives: **`--offline` is an
OPTIMIZATION on this path, never a semantic choice.** The step resolves a
lock for a root this process just generated, and issue 0676's frozen
property belongs to the BUILD, which stays `--locked` either way. So
offline buys "do not touch the network when the answer is already local",
and nothing else — which is worth keeping as the fast path, and worth
nothing at all when the answer is not local.

The retry cannot change WHAT resolves: the offline cache is a subset of the
registry, so a lock that resolves offline resolves identically online. It
changes only whether resolution can happen. A genuinely missing package
still fails, now with the registry's own error rather than cargo's
`offline mode (via --offline) can sometimes cause surprising resolution
failures` note — which is a strict improvement on the diagnostic that sent
this issue to the platforms in the first place.

Not chosen, and why:

- **`cargo fetch` before the offline step** — puts network at a defined
  point, but at EVERY build, including the warm-cache case that is the
  common one and currently costs nothing.
- **Vendoring** — a large permanent artifact for a transient cache
  problem.
- **Splitting `examples/workspaces/rust`** — addresses the real
  amplifier (a freertos build resolving `esp-backtrace` because they share
  a root) and is worth doing on its own merits, but it is a layout change
  with its own fallout, and it would not fix the general cold-cache case.

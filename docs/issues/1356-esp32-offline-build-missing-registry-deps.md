---
id: 1356
title: "The esp32 nightly build runs `--offline`/`--frozen` against a cache that has
  neither `esp-backtrace` nor `allocator-api2` — the same cold-cache class as archived
  issue 0873, recurring on a different dependency set"
status: open
type: bug
area: ci, esp32
severity: medium
found: 2026-09-12
related: [0873, 1025, 1070, 1158]
---

## What happens

Nightly run **34680021029** (schedule, 07:10), job **103517111645** (`esp32`),
step `Build (esp32)`. The build resolves its entry and then cannot resolve its
dependencies:

```
nros build demo_bringup:esp32 --workspace . --offline -- --profile nros-relwithdebinfo …
nros build: demo_bringup:esp32 -> board esp32-c3-baremetal (platform esp32), driver cargo
error: no matching package named `esp-backtrace` found
error: failed to download `allocator-api2 v0.3.1`

Caused by:
  attempting to make an HTTP request, but --frozen was specified
error: recipe `build-examples` failed with exit code 101
```

Two distinct symptoms, one cause: the job builds offline, and the crates the
esp32 entry needs are not in the cache it was given. `esp-backtrace` is not
present at all (`no matching package named`), and `allocator-api2 v0.3.1` is
known but not downloaded, so `--frozen` refuses the fetch rather than making it.

## Why offline is right and the cache is the bug

The `--offline`/`--frozen` posture is deliberate — it is what makes a lockfile a
promise rather than a suggestion, and `NROS_CARGO_FLAGS` injects `--locked`
project-wide for the same reason (issues 0359/0378). So the fix is not to drop
the flag; it is that the lane must populate what its own manifests resolve to
before the build starts.

This is the class archived issue **0873** ("nightly offline lockfile, cold
cache") already closed once. It is recurring on a different dependency set,
which is the signal that the previous fix addressed the instances rather than
the mechanism: nothing checks that the warmed cache actually covers the esp32
entry's resolution.

## What this is NOT

- Not a missing-crate-version problem in the manifests. The same entry builds
  where the cache is warm; the resolution is not in dispute.
- Not the disk exhaustion of issue 1353 — this job fails in seconds, on
  resolution, with no `No space left on device` anywhere in its log.
- Not issue 1025 (the esp32 packer deriving the wrong fixture artifact dir),
  though it is the same lane and that one also hid behind a first failure.

## What would close it

1. Establish what warms the cargo cache for the esp32 nightly job and why the
   esp32 entry's graph is outside it — the entry is generated (`nros build`
   writes the selection facade and the entry), so its dependency set may not
   exist at the time the warming step runs. That ordering is the likeliest
   mechanism and should be confirmed, not assumed.
2. Make the gap loud where it happens: a build that resolves an entry and then
   goes offline should assert its resolution is satisfiable from the cache and
   name the crates that are not, rather than surfacing as two unrelated-looking
   cargo errors.
3. Acceptance is the `esp32` nightly job reaching a verdict on its cells on
   three consecutive nights, and the mechanism — not just these two crates —
   being what changed.

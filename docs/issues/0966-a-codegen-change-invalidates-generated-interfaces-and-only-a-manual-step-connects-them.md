---
id: 966
title: "A codegen change invalidates every consumer's generated interfaces, and only a manual `setup-cli` connects them"
status: open
area: build, codegen
severity: medium
related: [0963, 0965, phase-403]
---

# The staleness check is right, and it is the only thing holding the chain together

## The dependency

```
rosidl-codegen sources  ->  the in-tree `nros` CLI  ->  every consumer's generated interfaces
```

Neither arrow is automatic. `just setup-cli` rebuilds the CLI by hand, and a
consumer's interfaces regenerate only when its own build runs. The single thing
connecting them is `NanoRosCodegenCore.cmake`'s staleness check:

```
Error: in-tree nros CLI is STALE -- its sources changed since it was built
```

## What it costs

Observed three times in one afternoon of phase-403 work, each time from an
ordinary action:

1. editing `rosidl-codegen` (the whole point of the phase),
2. moving the submodule pin forward to a merged `main`,
3. editing it again for the follow-up wave.

Each stop needs the same manual recovery -- rebuild the CLI, rebuild the image
-- and the second case is the interesting one: nothing in the consumer's tree
changed at all. Taking a NEWER upstream is enough to stale the CLI, so a user
who did nothing but update a pin is told their CLI is out of date.

## Why the check must stay

It is doing real work, and it caught a case that would have been silent. The
C emitter's sequence-of-strings dimensions were transposed in all three emission
sites (`string[]` produced `char data[256][64]` instead of `[64][256]`); the fix
lives in codegen. Without the staleness check the consumer would have
regenerated with the OLD binary and reported success, with the tree claiming the
fix was in. Same shape for phase-403's derived bounds: a stale CLI emits the
previous rule's numbers and nothing says so.

So this is not "delete the check". It is that the check is the ONLY thing
holding the chain, and it holds it by stopping the build rather than by fixing
it.

## What would resolve it

Options, none chosen:

1. **Make the CLI a build dependency of codegen**, so a consumer's build rebuilds
   it rather than refusing. Cost: every consumer build can now compile a Rust
   binary, which on the Zephyr lane is a surprise, and it hides the cost of a
   codegen change rather than naming it.
2. **Stamp the generated output with the CLI's source hash** and regenerate when
   it differs, which is what the fixture stamps already do elsewhere in this
   tree. The consumer then rebuilds interfaces without rebuilding the CLI,
   and the CLI rebuild stays explicit.
3. **Keep the refusal and make recovery one step** -- the message names
   `just setup-cli`, so a `--fix` affordance or a recipe that does both would
   turn three manual recoveries into three keystrokes.

(2) matches how this repo already handles derived state elsewhere, and it is
the only one that distinguishes "the CLI is stale" from "your generated code is
stale", which are different problems with different costs.

## Adjacent

The same shape as issue 0963: the build knows a fact -- here, that the CLI
predates its sources -- and can only say so by stopping. 0963 is about numbers
that are computed and never read; this is about a dependency that is detected
and never acted on.

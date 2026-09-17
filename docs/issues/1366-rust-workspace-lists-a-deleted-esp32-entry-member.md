---
id: 1366
title: "`examples/workspaces/rust` lists `src/esp32_entry` as a member and the
  package is gone, so `build-test-fixtures lane=native` cannot complete"
status: open
type: bug
area: build, examples
related: [phase-445, phase-455]
---

## Symptom

`just build-test-fixtures lane=native` does not complete on `main`. Measured
2026-09-13 in the box tree during phase-455 W5; the run stops in the rust
workspace, and every later row in that lane goes unbuilt.

`examples/workspaces/rust/Cargo.toml:29` lists

```toml
    "src/esp32_entry",
```

as a workspace member. The directory does not exist:

```
$ ls examples/workspaces/rust/src/esp32_entry
No such file or directory (os error 2)
```

A cargo workspace refuses to load when a listed member has no manifest, so the
failure is at MANIFEST PARSE — the same absorbing shape as issue 0463: nothing
in the workspace can be read, and the error names the missing path rather than
the reason it is listed.

## Cause

phase-445 W5 deleted the hand-written package and moved its generation to the
image: "`examples/workspaces/rust/src/esp32_entry` deleted; `[image.esp32]`
generates it" (`docs/roadmap/phase-445-board-choice-generates-leaf-config.md`).
The member line was not removed with it.

So the manifest is correct only AFTER `nros sync` has generated the entry for
the esp32 image, and wrong in a fresh clone and in any lane that does not
generate that image first. `lane=native` does not.

## Why it was not caught

The lane that would have caught it is the one it breaks. `check::fast` does not
load example workspace manifests, and the esp32 rows are nightly-only
(`matrix::CELLS` carves the esp32 action cell out entirely), so no merge-gating
event reads this file.

## Fix shape

Two candidates, and they differ in what they promise:

1. **Drop the member line.** Correct if the generated entry is reached through
   the image's own generated manifest rather than as a member of the tracked
   workspace root. Cheapest, and it makes the tracked manifest true in a fresh
   clone.
2. **Keep the line and make the lane generate the entry first.** Correct if the
   entry is meant to be a member. Then `build-test-fixtures` has to run the
   esp32 image's sync before loading the workspace, which couples a native lane
   to an esp32 generation step.

Read RFC-0098's generated-entry rule and phase-445 W5 before picking: whether a
generated entry belongs in the tracked root's member list is that phase's
decision, not this issue's.

**Acceptance.** `just build-test-fixtures lane=native` completes from a fresh
clone with no esp32 step, and a gate reads example workspace manifests for
members that no tracked directory and no generator provides.

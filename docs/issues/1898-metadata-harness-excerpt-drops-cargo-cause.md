---
id: 1898
title: "Tier-2 nightly stopped in `generate-bindings` on a git dependency it could not get, and the metadata harness's one-line excerpt dropped the reason"
status: open
type: bug
area: [ci, tooling, testing]
severity: medium
found: 2026-10-14
related: [1626, 1158, 0699, phase-466]
---

**Home: [phase-466](../roadmap/phase-466-scheduled-lanes-produce-verdicts.md)** — homed 2026-10-17; listed in that phase's "Issues homed here (2026-10-17)".

## Measured

Nightly run **38515326763** (schedule, 05:11 UTC, main `bdd5e3d1e`), job
**115692717152** `tier 2 nightly (pairwise cover)` on
`nano-ros-runner-newslab-server-241`, step `just build tier2-nightly`. The
other 25 jobs in the run passed and 5 were skipped. The lane never reached a
cell (the 1158 axis: it stopped at the build stage).

```
Error: refresh source metadata for `rust_qos_talker_pkg`

Caused by:
    metadata-mode harness failed (exit 101) for component 'reliable_talker': error: failed to get `ros-launch-manifest-model` as a dependency of package `nros-macros v0.6.0 (/home/runner/_work/nano-ros/nros-fixture-tree/packages/core/nros-macros)`

Location:
    nros-cli-core/src/orchestration/metadata_build.rs:716:21
error: recipe `generate-bindings` failed on line 4625 with exit code 1
```

`ros-launch-manifest-model` is a git-tag dependency
(`packages/core/nros-macros/Cargo.toml`:
`git = "https://github.com/NEWSLabNTU/ros-launch-manifest.git", tag = "v0.1.57"`).
The tag moved to v0.1.57 on 2026-10-12 (39c3b1367a). The 10-13 nightlies'
tier-2 jobs failed on other gates (`sizes-header-mirrors`, `check fast`), so
this is the first run that got this far after the bump.

## The finding that does not depend on the cause

**The log cannot say WHY cargo could not get the dependency.** cargo prints
`failed to get <dep>` and then a `Caused by:` chain that holds the real reason:
a network timeout, an unknown tag, an offline or `--locked` refusal, or a
missing git database. `metadata_build.rs` builds the bail message from
`first_diagnostic(&stderr)`, which keeps the first line that starts with
`error` and nothing else (`metadata_build.rs:819`). The cause chain is
discarded, and neither the fixture build nor the job log keeps the harness
stderr. So a transient network failure and a real resolution defect produce
byte-identical reds. This is the same misleading shape 1626 found for
`api-parity`, one tool over.

## What this is NOT

- **Not 1626.** That issue is `api-parity`'s `cargo doc` reaching crates.io.
  This is the metadata harness reaching a GitHub git dependency, at a
  different site. They may share a transient-network root, but nothing here
  shows it.
- **Not the 10-13 tier-2 reds.** Those failed on `sizes-header-mirrors` and
  `check fast`.
- **Not shown to be a broken tag.** The PR and merge-group lanes resolve the
  same manifest. A tag that does not exist would fail them too, and they are
  green.

## What would close this

1. Make the excerpt carry cargo's `Caused by:` chain, or at least its last
   line, whenever the first diagnostic is `failed to get` / `failed to load
   source`. That line is what separates a network flake from a resolution
   defect. Keep the message one line if the caller needs that, by appending
   the root cause.
2. Then look at the next tier-2 nightly. If it reproduces, the root cause
   will be in the log. If it does not reproduce, record this as a transient
   network failure and, if it recurs, weigh a `CARGO_NET_RETRY` for the
   fixture build alongside 1626's remedy 1.

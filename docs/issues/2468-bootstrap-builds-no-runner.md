---
id: 2468
title: "`bootstrap.sh` builds no runner package, so the checkout road's first `nros run` refuses"
status: open
type: bug
severity: medium
area: [install, cli, ux]
related: [phase-527, phase-523]
found: 2026-11-12
---

**Home: [phase-523](../roadmap/phase-523-education-labs-adoption-probes.md).** Found walking the book (`PROBE_TRACK=zenoh just probe bootstrap`, main `1df1191957`). CI never runs that track; the nightly bootstrap probe runs `quickstart` only.

## Symptom

The book's checkout road (book-internals `orientation/contributing.md`: clone, `./scripts/bootstrap.sh`, `source ./activate.sh`, `nros setup`) builds, then the first `nros run` refuses:

```
nros run: runner `native` (package `nros_runner_native`) is not built (looked for /root/nano-ros/packages/runners/target/release/nros-runner-native). Build it with `just setup-cli` in /root/nano-ros
```

That page says `just` is not a prerequisite, and the container has none.

## Cause

phase-527 W3/W5 moved the runners and the `router` / `px4` verbs into their own packages, each its own workspace. `just setup-cli` builds them beside the CLI. `bootstrap.sh`, the `just`-free front door, still builds only `nros` and `nros-launch-resolve`, and so does the page's by-hand block.

## Fix

`bootstrap.sh` builds the same three as `just setup-cli`: `packages/runners` (`--workspace`), `nros_zenoh_tools` and `nros_px4_tools`, failing loudly like the resolver step. The by-hand block in `contributing.md` gains the same three `cargo build` lines.

Acceptance: `PROBE_TRACK=zenoh just probe bootstrap` passes on the fix branch.

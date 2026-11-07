---
id: 2317
title: "On a release every probe sidecar reads stale, and the talker scaffold's contract then sizes its callback table to 0, so First Project does not compile on nros-0.6.0-nros7"
status: resolved
type: bug
area: cli, release, scaffold
purpose: [industrial]
related: [phase-523, issue-2270]
found: 2026-11-07
resolved_in: "issue-2309-heap-bss branch: the SDK root ships probe-source-dirs.txt; the talker contract declares its timer path; other roles write none"
---

**Home: [phase-523](../../roadmap/phase-523-education-labs-adoption-probes.md)** — W3 round two (2026-11-07), industrial stop 2.

## Measured

Industrial round-two probe, `nros-0.6.0-nros7` in a pristine `ubuntu:24.04`
with Jazzy. Following Tutorial › First Project as written:

```
nros build:   …/src/sensor_node/metadata/sensor_node.json: stale against its sources (re-run `nros configure`), so neither its counts nor its registration observations are used
nros build:   resolved → …/resolved.toml (max_cbs 0, nodes 1, …)
error[E0080]: evaluation panicked: nros::main!: the SystemModel registers 1 callback entities but the executor callback table compiled in is smaller …
```

Reproduced in `ros:jazzy` with the release installed by `install.sh`. With the
scaffold's `system.contract.yaml` deleted, it builds. With a contract that also
declares the timer path, it resolves `max_cbs 1`, and `nros run` matches
`Publishing:` after 3.2 s.

## Cause

Two defects stack.

1. **A release never trusts a probe sidecar.** `probe_inputs_key` hashes the
   crates listed in `<nano-ros root>/packages/cli/probe-source-dirs.txt`, and
   `scripts/stage-sdk-root.sh` did not carve that file back into the SDK root.
   So on every release install the key is `None` and every sidecar reads stale.
   The same image built from a checkout reads `max_cbs 1` with no warning.
2. **Issue 2270's scaffold contract was a partial entity declaration.** A
   contract that names a node is that node's entity declaration. The talker's
   declared the publisher, not the 1 Hz timer, and a publisher claims no
   callback slot. Every other role got `params: {}` alone, which declares zero
   entities. With the probe's counts dropped (1.), the derivation read the
   contract alone and counted 0.

Before issue 2270 there was no contract, the derivation refused, and the board's
configured table applied. That is why the release before this one built.

## Resolution (2026-11-07)

- `scripts/stage-sdk-root.sh` carves `packages/cli/probe-source-dirs.txt` back
  into the SDK root, and `need -f` refuses a staged root without it. Every path
  the list names was checked present in a staged root.
- The talker's scaffold contract declares its timer path
  (`paths.on_tick.trigger.timer.rate_hz: 1`, output `chatter`). Every other
  role writes no contract and keeps the documented no-contract road. The test
  `only_a_role_whose_whole_surface_is_known_gets_a_contract` holds that.

A release cut after this lands carries both fixes; `nros-0.6.0-nros7` keeps the
bug.

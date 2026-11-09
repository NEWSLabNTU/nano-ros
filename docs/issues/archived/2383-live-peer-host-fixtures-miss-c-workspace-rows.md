---
id: 2383
title: "live-peer's derived host fixture set misses the C workspace's Cyclone rows since the linux pairs became workspace images, so two cells are UNMET every night"
status: resolved
type: bug
severity: medium
area: [ci, fixtures, interop]
related: [phase-535, phase-466, 1758]
found: 2026-11-09
resolved: 2026-11-10
---

**Home: [phase-535](../../roadmap/phase-535-example-reorg-and-cadence-ci.md)** --
W4 (`f6957c899c`, 2026-11-06) moved the linux example pairs into workspace
images.

## Measured

Scheduled `live-peer regression` was green on 11-05 and 11-06 and has been red
since 11-07. On 11-09 (run 42560467078, `6e35469bde`) the one real failure is
`bridge_zenoh_to_cyclonedds test_zenoh_to_cyclonedds_bridge_to_nano_listener`:

```
[UNMET PRECONDITION] fixture not built: linux/c/listener cyclonedds (run `just cyclonedds setup`):
Test fixture binary not prebuilt:
.../examples/workspaces/c/build/posix-cyclonedds-linux-pubsub_bringup/cmake/linux_listener_cyclonedds_entry
```

The same night's interop cells also resolve
`.../examples/workspaces/c/build/posix-cyclonedds-linux-service_bringup/cmake/linux_service_server_cyclonedds_entry`.

The job's fixture step builds the host workspace rows DERIVED from the cells'
tests (`check-interop-verdicts.py --host-workspace-fixtures`). On 11-09 that
set was eight rows, all Rust or features:

```
workspace-features-rust-params  workspace-features-rust-params-xrce
workspace-features-rust-qos     workspace-rust-native
workspace-rust-native-bridge    workspace-rust-native-bridge-xrce
workspace-rust-native-cyclonedds  workspace-rust-native-xrce
```

No C workspace row is in it, although the cells now resolve C workspace
images.

On 11-08 (run 42445702030) the reds were wider (XRCE interop, and Rust
workspace entries also "not prebuilt"), and 11-07 had both. 11-09 is
narrowest, so read it first. The other nights may have a second cause behind
this one.

## Why (confirmed)

The derivation reads which helpers the cells' tests call, and maps those helpers
to fixture rows. W4 moved `linux/c/<example>` from a per-example leaf to a C
workspace image. The derivation's map was not moved with it, so the C rows
dropped out of the set while the tests kept asking for them.

## What would close it

A scheduled `live-peer` run whose bridge and interop cells all run, green or
red about the code.

## Fix

Confirmed, and it was ONE cause for both nights. W4 moved the linux pairs out
of `[[fixture]]`, and live-peer's host job derives what to build two ways,
neither of which reached a pair image:

- `--host-builds` emits `fixtures-build.sh linux <lang> <rmw>`, which after
  the move builds only the witness leaves still in `[[fixture]]`;
- `--host-workspace-fixtures` found a workspace row only by a quoted row id in
  the test or a helper it calls, and a pair image's id is COMPUTED by
  `nros_tests::fixtures::workspace_image`, so no test names one.

So 11-08's Rust XRCE "not prebuilt" reds are the same miss on another
(lang, rmw), not a second cause.

`scripts/check-interop-verdicts.py --host-workspace-fixtures` now also emits,
read from the manifest (never a hand list):

- every linux pair image of each host cell's own (lang, rmw) — what
  `fixtures-build.sh linux <lang> <rmw>` built for it before the move;
- every pair image named by a `linux_pair_image("<lang>", "<role>",
  Rmw::<R>)` call in the test or a helper it calls, because a cell may resolve
  one OFF its coordinate (the Rust/Zenoh bridge cell's Cyclone listener is
  `linux/c/listener`). A variable argument is a wildcard.

On the 2026-11-10 ledger the derived set goes from 8 rows to 50, including
`workspace-c-native-listener-cyclonedds`,
`workspace-c-native-service-server-cyclonedds` and the Rust XRCE pair images.

## Verified

- `--self-test`: new cases for both rules and for the real
  `bridge-zenoh-to-cyclone-imperative` cell; removing either rule fails them.
- `--list-passing --runner host --host-workspace-fixtures` on the tree: 50
  rows, as above.
- Not yet verified: a scheduled `live-peer` run, which is what closes it. Read
  the next night's run before trusting this.

## Cost

About 42 more workspace rows per live-peer run. They share one cargo target per
workspace, but each is a configure and link; watch free disk against issue
2130, whose builds already took the runner from 106 G to under 49 G.

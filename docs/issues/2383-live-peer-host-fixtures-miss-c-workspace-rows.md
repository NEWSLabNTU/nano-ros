---
id: 2383
title: "live-peer's derived host fixture set misses the C workspace's Cyclone rows since the linux pairs became workspace images, so two cells are UNMET every night"
status: open
type: bug
severity: medium
area: [ci, fixtures, interop]
related: [phase-535, phase-466, 1758]
found: 2026-11-09
---

**Home: [phase-535](../roadmap/phase-535-example-reorg-and-cadence-ci.md)** --
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

## Why (to confirm)

The derivation reads which helpers the cells' tests call, and maps those helpers
to fixture rows. W4 moved `linux/c/<example>` from a per-example leaf to a C
workspace image. The derivation's map was not moved with it, so the C rows
dropped out of the set while the tests kept asking for them.

## What would close it

A scheduled `live-peer` run whose bridge and interop cells all run, green or
red about the code.

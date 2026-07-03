---
id: 140
title: "Native per-host workspace entry (hosted spin) subscription receives nothing — `multihost_runtime_e2e` fails; blocks the 276-W6 zephyr multihost e2e's native half"
status: open
type: bug
area: core
related: [phase-276, phase-211]
---

## Summary

The NATIVE per-host workspace entry's subscription path is dead on current main:
`native_entry_robot2` (baked from `nros::main!(launch =
"demo_bringup:multihost.launch.xml", host = "robot2")`, driven by the env-gated
hosted spin) exits with `message_callbacks=0` while a sibling `robot1` talker
demonstrably publishes on the same router — and while the plain
`examples/native/rust/listener` example, subscribed to the SAME `/chatter`
`Int32` stream on the SAME zenohd, receives every sample. `multihost_runtime_e2e`
(native robot1 + robot2 pair, phase-211.F) fails; it was masked by stale
workspace fixtures (`.inputsig` skip) for an unknown window, so the regression
range is wide.

## Evidence (2026-07-03)

- `multihost_runtime_e2e` FAILs after a fresh `workspace-fixtures-build.sh
  native rust`: robot2 prints `hosted spin complete callbacks=0
  message_callbacks=0`.
- Manual: zenohd on a private port; `native_entry_robot1` logs `talker
  publishing chatter seq=0..7`; concurrently
  - `native_entry_robot2` (hosted spin, 8–12 s budget): 0 callbacks;
  - `examples/native/rust/listener` (same Int32 `/chatter`): **8 received**.
  So router + native publisher + native subscriber machinery are all fine —
  the per-host ENTRY's subscribe/dispatch half is what never fires. robot2 is
  fully silent even at `RUST_LOG=debug` (no receive, no error).
- The zephyr projection of the same launch (276 W6,
  `zephyr_entry_robot1`) boots, bakes only the robot1 slice (host filter
  proven: "entry up (1 nodes)"), and publishes — its e2e
  (`multihost_zephyr_entry_e2e`) fails only on this native robot2 half and is
  `#[ignore]`d on this issue.

## Suspects

Either the hosted-spin loop never dispatches subscription callbacks for
macro-baked entries (executor drive path), or the baked robot2 entry's
subscription is created against a session/registry the spin doesn't poll.
Compare with the working listener example's spin. Note
`multihost_partition_bake` (source-level) still passes — the gap is runtime.

## Impact

- `multihost_runtime_e2e` red (was stale-skip-masked).
- 276 W6 (multihost-on-Zephyr): embedded half landed
  (`zephyr_entry_robot1` fixture + leaf on 17853 + e2e), blocked on this for
  the green light.

---
id: 1828
title: "The zenoh→cyclonedds bridge entry declares two liveliness tokens zenoh rejects as `KeyExpr`, so its bridge endpoints are invisible to `ros2 node list` / `ros2 topic info`"
status: open
type: bug
area: [rmw, zenoh, bridge]
severity: low
found: 2026-10-12
related: [phase-433, 1310]
---

## Measured

`workspace-rust-native-bridge` (`examples/workspaces/bridge-cyclonedds`, one
`[[bridge]] gw` from `zenoh:zen` to `cyclonedds:dds`), built by
`scripts/build/workspace-fixtures-build.sh linux rust --id workspace-rust-native-bridge`
and run against a local `rmw_zenohd` with `NROS_ENTRY_SPIN_MS=2000`, logs
on every run (4 of 4 lines across two runs, two per run):

```
[ERROR] nros: liveliness: declare failed (KeyExpr) for a 107…
```

The entry otherwise runs and exits cleanly ("application complete", rc=0),
and it registers in issue 1310's runtime test. The message is from
`declare_keyexpr` in `nros-rmw-zenoh/src/shim/session.rs`: zenoh-pico refused
the key as malformed — not `Full`, so `ZPICO_MAX_LIVELINESS` is not the knob.
Whatever entity it was for works but cannot be seen by ROS 2 graph tools.

## Not yet known

Which entity, and which segment is malformed. The keyexpr is logged only at
`log::debug!` under `std`, and this image installs no `log` backend, so it
never appears; the error line truncates before the key.

A lead, not a diagnosis: a full entity token carries `RIHS01_` plus 64 hex
digits of type hash (71 bytes) and the type name besides, so 107 bytes is short
for one. A bridge endpoint forwards bytes without a typed message, so an EMPTY
type-name or type-hash segment — `//` in the key, which zenoh rejects — fits
the length. Check before believing it.

## Fix

1. Put the rejected key in the error line itself (it is the only output an
   embedded image has), bounded to the buffer.
2. Fix whichever segment is empty for an untyped bridge endpoint — the
   upstream `rmw_zenoh` convention for an unknown hash is a placeholder
   segment, never an empty one.
3. A unit test in the shim's keyexpr builder: no generated key has an empty
   chunk.

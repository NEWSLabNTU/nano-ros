---
id: 1828
title: "The zenoh→cyclonedds bridge entry declares two liveliness tokens zenoh rejects as `KeyExpr`, so its bridge endpoints are invisible to `ros2 node list` / `ros2 topic info`"
status: resolved
type: bug
area: [rmw, zenoh, bridge]
severity: low
found: 2026-10-12
resolved: 2026-10-12
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

## Resolution (2026-10-12)

The lead held. With the rejected key logged (fix step 1), the bridge printed:

```
liveliness key @0:  @ros2_lv/0/0279059d1e2dfad6e3e951d2788b0
liveliness key @40: c44/0/3/MS/%/%/s0/%chatter/std_msgs::msg
liveliness key @80: ::dds_::Int32_//::,4:,:,:,,
```

The type-hash segment was empty (`Int32_//`). These are the config-driven
bridge's source-side subscriptions (`MS`, node `s0`), whose `BridgeCfg.type_hash`
is `#[serde(default)]`, so it is `""` when no hash is configured. Zenoh refuses an
empty chunk.

- **Step 1, the rejected key is now in the log:** after the error line, the key
  follows in 40-byte records prefixed with their offset, because one `nros_log`
  record holds ~64 bytes and the old line was cut before the key.
- **Step 2, the key builder never emits an empty hash segment:** the four
  liveliness builders (publisher, subscriber, service server, client) take the
  hash through `hash_segment()`, which substitutes `TypeHashNotSupported`, the
  placeholder the Humble data key already uses. The fix is in the builder, not
  in the bridge, so no other untyped caller can reproduce it.
- **Step 3, the test:** `a_key_with_no_type_hash_has_no_empty_chunk` covers all
  four kinds. Mutation, with the substitution disabled: FAILED, printing the
  same `…Int32_//2::…` key; restored, it passes (29/29 shim tests).

End to end, `workspace-rust-native-bridge` against a local `rmw_zenohd`:
**2 `KeyExpr` errors per run before, 0 after**. It still ends with
"application complete", rc=0. Not checked: `ros2 node list` from a ROS host;
the token is now accepted by zenoh, which is the precondition for that.

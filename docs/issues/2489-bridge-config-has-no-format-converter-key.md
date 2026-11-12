---
id: 2489
title: "`nros-bridge.toml` has no `converter` key and no format field: RFC-0088 D3's cross-image format agreement is a TODO left behind by an archived phase"
status: open
type: tech-debt
severity: low
area: [bridge, serdes]
related: [rfc-0088, rfc-0009, phase-421]
found: 2026-11-13
---

**Found by** the phase-555 W2 review of RFC-0088, item B17 in
`docs/roadmap/data/phase-555-w2-rmw-wire-brief.md`.

## Measured

- RFC-0088 D2 says a topic's serialization format crosses images in
  `nros-bridge.toml`. D3 gives the bridge a `converter` per bridged topic when
  the two sides disagree.
- `packages/rmw/bridge/src/config.rs:137-145` still reads `TODO(phase-421 W4)
  … Until W4 lands`. phase-421 is archived with W4 marked landed, yet the
  config has no `converter` key and no format field.
- The runtime half exists: `PubSubBridge::with_converter`, and the format
  check in `PubSubBridge` (`packages/rmw/bridge/src/lib.rs:7-17`). So a
  converter can be wired from Rust, but not from the bridge config a
  workspace writes.

## Next

Either add the key (a `converter` naming a registered converter, plus the
per-side format that D2 says travels here, refused when the two sides
disagree and no converter is named), or amend RFC-0088 D2/D3 to say the format
agreement is checked at runtime only. Then delete the stale TODO either way.
The rest of RFC-0088 is ready for Stable (brief B17).

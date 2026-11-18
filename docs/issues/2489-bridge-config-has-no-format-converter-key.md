---
id: 2489
title: "`nros-bridge.toml` has no `converter` key and no format field: RFC-0088 D3's cross-image format agreement is a TODO left behind by an archived phase"
status: open
type: tech-debt
severity: low
area: [bridge, serdes]
related: [rfc-0088, rfc-0009, phase-421, 2523, phase-555]
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

## Merged from issue 2523 (2026-11-18)

Issue 2523 was filed for the same gap a day later, from the same phase-555
review. It is archived as a duplicate of this one. What it added:

- phase-421 W4 did not add the name-to-converter registry the TODO waits
  for (the `serdes` provider family), so the TODO names a work item that will
  not land. Accepting the key without one would resolve it to nothing.
- The rmw-wire owner amends RFC-0088 D2 under phase-555 W3 and cites this
  issue there.
- If the key is added, an unknown converter name is refused, naming the
  known ones. Either way the TODO in `config.rs` must stop naming
  phase-421 W4.

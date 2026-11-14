---
id: 2523
title: "The bridge config has no `converter` key, so a cross-format bridge can only be built in source"
status: open
type: limitation
severity: low
area: [rmw, serdes]
related: [phase-421, phase-555]
found: 2026-11-14
---

**Home: `packages/rmw/bridge/src/config.rs` (RFC-0088 D3)**

## What is missing

RFC-0088 D3 gives a bridge entry in `nros-bridge.toml` an optional
`converter = "<name>"` key, so that a bridge between two images using
different serialization formats can be declared rather than written. The key
does not exist. `config.rs:137-145` carries it as `TODO(phase-421 W4)`:

> A name can only become a `&'static dyn SerializationFormatConverter` once
> something maps names to converters, and that registry is the `serdes`
> provider family in W4. Accepting the key now would resolve it to nothing.

phase-421 is archived, and W4 did not add the name-to-converter registry. So
the TODO points at a work item that will not land, and a cross-format bridge
is still built in source with `PubSubBridge::with_converter`.

RFC-0088 D2's sentence "the format crosses images in `nros-bridge.toml`" is
therefore not true yet. The rmw-wire owner amends D2 under phase-555 W3, and
cites this issue there.

## Found by

phase-555's design review (rmw-wire brief, B17 and "Untracked work found").
The user ruled that the untracked items be filed as issues (2026-11-14).

## What closes it

Either a registry that maps a converter name to a
`SerializationFormatConverter`, plus the `converter` key parsed against it
(an unknown name refused, naming the known ones), or a ruling that
cross-format bridges stay in source and RFC-0088 D3 says so. Either way, the
TODO in `config.rs` must stop naming phase-421 W4.

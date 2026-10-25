---
id: 2062
title: "An image cannot override one bridged domain's RMW file yet"
status: open
type: limitation
area: [config, rmw, bridge]
severity: low
found: 2026-10-25
related: [phase-514, RFC-0109]
---

## What

RFC-0109 D6 gives an image `rmw_config = { <domain> = "<file>" }`: a
per-image override of one `[[domain]]`'s `config`, for two ECUs on one CAN
bus with different ids (U11) or a router that differs between a QEMU run and
the lab (U9).

phase-514 W3b delivers a domain's file to the session that joins it through
the bridge runtime configuration, `<bringup>/nros-bridge.toml`, which
`nros sync` writes ONCE PER BRINGUP and every image's `nros::main!` bakes.
There is no per-image copy, so an image's override has no carrier: it would
be checked at sync and then read by nothing — indistinguishable from one that
applied. So `nros sync` refuses the table form, naming this issue; the
string form (`rmw_config = "<file>"`, the default session's file) is
delivered on every road.

## What would close it

A bridge configuration per image — `nros-bridge.toml` keyed by image id, or
the bridge sessions carried in the image's own settings beside
`NROS_SESSION_PROPERTIES` — so the planner can resolve each bridged endpoint
with that image's override (`SystemToml::domain_file` already takes the
image).

## Workaround

Give each image its own bringup, or name the file on the `[[domain]]` itself.

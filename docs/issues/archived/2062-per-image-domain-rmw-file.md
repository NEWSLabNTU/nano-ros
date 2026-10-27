---
id: 2062
title: "An image cannot override one bridged domain's RMW file yet"
status: resolved
type: limitation
area: [config, rmw, bridge]
severity: low
found: 2026-10-25
related: [phase-514, RFC-0109]
resolved: 2026-10-27
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

## Resolution

`nros sync` writes a bridge configuration per image when it has to: beside
the bringup's `nros-bridge.toml`, an image whose `rmw_config` table overrides
a bridged domain's file gets `nros-bridge.<image>.toml`, planned with that
image's override (`SystemToml::domain_file(dir, name, image)` threaded through
`resolve_bridge_endpoint` / `bridge_transports` / `bridge_plan_entries`). An
image with no override shares the bringup's file, and a per-image file whose
image no longer overrides anything is removed, so a stale copy cannot be
baked.

`nros::main!` prefers `nros-bridge.<NROS_IMAGE>.toml` when it exists. The
workspace cargo road did not set `NROS_IMAGE` (only the single-package road
did), so `nros build` now states it there too; without it the macro silently
fell back to the bringup's file — measured: an image overriding one domain to
`tcp/127.0.0.1:7466` baked `7447` until the variable was set, and `7466` after,
while its sibling image kept `7447`. The refusal in `nros sync` is gone.

---
id: 2124
title: "A lone `[[domain]]` is not an image's default session, so its `config` is ignored there"
status: open
type: limitation
area: [orchestration, cli, rmw]
severity: low
found: 2026-10-29
related: [issue-2062]
---

## What

RFC-0109 D6 makes `[[domain]]` the way a `system.toml` names a session, with
`config` naming that session's RMW file. With two or more domains, the
non-default ones become bridge sessions and each carries its own file
(phase-514 W3b).

A `system.toml` that declares exactly ONE `[[domain]]` is the common case, and
there the domain is not treated as the image's default session. The
default-session resolver, `default_session_file` in
`packages/cli/nros-cli-core/src/orchestration/rmw_config.rs`, consults:

1. `[image.<id>] rmw_config` (its default-file form),
2. `[system] rmw_config`,
3. the conventional `rmw/<backend>.*` file,

and never reads `[[domain]]`. A lone domain's `config` and `id` therefore do
not reach the default session. The working spelling today is `[system]
rmw_config` (plus `[system] domain_id`), as phase-514 W3b states.

## Consequence

A user who follows the RFC's own shape for a single-domain system writes a key
that resolves without error and is never used. Nothing refuses it.

## Fix shape

When the model holds exactly one `[[domain]]` and the image names no
`rmw_config` of its own, take that domain's `config` and `id` as the default
session's. Then either refuse a `[system] rmw_config` that disagrees with it,
naming both, or (D9's stance) refuse stating both at all. Apply the rule in
the shared resolver so the cargo, cmake and west roads all agree. Acceptance:
a one-domain leaf naming `[[domain]] config = "rmw/zenoh-x.json5"` bakes that
file's table on each road.

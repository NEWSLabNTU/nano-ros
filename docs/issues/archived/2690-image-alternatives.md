---
id: 2690
title: "Images of one host that never run together are refused for sharing a static address"
status: resolved
type: enhancement
area: [orchestration, cli]
severity: medium
found: 2026-11-18
resolved: 2026-11-22
resolved_in: "feat(cli): `[image_alternatives]` lets variants of one host share an address (RFC-0136)"
related: [2679, phase-478, phase-514, phase-586]
---

**Home:** phase-478 I32 (found by the safety island). Built in
[phase-586](../../roadmap/phase-586-image-alternatives.md), design
[RFC-0136](../../design/0136-image-alternatives.md).

## Symptom

A bringup builds one Zephyr host four ways on one TAP network (`fvp-tap`,
`fvp-tap-stats`, `fvp-tap-trace`, `fvp-tap-trace-ring`), all at
`192.168.10.2`. Only one is ever booted. Stating that address in a board
network file (issue 2679) made every `nros build` of the bringup fail in
`image_net::check_collisions`:

```text
images `fvp-tap` (eth0) and `fvp-tap-stats` (eth0) both take 192.168.10.2 on
network "tap" -- two hosts on one network cannot share an address; ...
```

So the address stayed in a `prj-*.conf`, where nothing checks it.

## Cause

`check_collisions` (`packages/cli/nros-cli-core/src/orchestration/image_net.rs`)
treated every pair of images in one `system.toml` as two hosts that may run
together. Nothing in `system.toml` could say otherwise.

## Resolution

RFC-0136 built. A top-level `[image_alternatives] <group> = ["a", "b", ...]`
declares images mutually exclusive (`SystemToml::validate_image_alternatives`
refuses an unknown member, an image in two groups, a group of fewer than two
and a duplicate member). `check_collisions` lets one group share a static
address and a MAC; any other shared address or MAC is still refused, with a
hint naming the table, or naming both groups when the two images are in
different ones. A single-package leaf refuses the table. Exclusivity is
declared, never inferred.

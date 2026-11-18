---
id: 2690
title: "Images of one host that never run together are refused for sharing a static address"
status: open
type: enhancement
area: [orchestration, cli]
severity: medium
found: 2026-11-18
related: [2679, phase-478, phase-514]
---

**Home:** phase-478 I32 (found by the safety island). Design:
[RFC-0136](../design/0136-image-alternatives.md), Draft, for review.

## Symptom

A bringup builds one Zephyr host four ways on one TAP network (`fvp-tap`,
`fvp-tap-stats`, `fvp-tap-trace`, `fvp-tap-trace-ring`), all at
`192.168.10.2`. Only one is ever booted. Stating that address in a board
network file (issue 2679) makes every `nros build` of the bringup fail in
`image_net::check_collisions`:

```text
images `fvp-tap` (eth0) and `fvp-tap-stats` (eth0) both take 192.168.10.2 on
network "tap" -- two hosts on one network cannot share an address; ...
```

So the address stays in a `prj-*.conf`, where nothing checks it.

## Cause

`check_collisions` (`packages/cli/nros-cli-core/src/orchestration/image_net.rs`)
treats every pair of images in one `system.toml` as two hosts that may run
together. Nothing in `system.toml` can say otherwise.

## Fix (proposed)

RFC-0136: a top-level `[image_alternatives] <group> = ["a", "b", ...]`
declares images mutually exclusive. The collision check lets one group share
a static address and MAC; any other shared address is still refused.
Exclusivity is declared, never inferred from names (ruling 2026-11-18).

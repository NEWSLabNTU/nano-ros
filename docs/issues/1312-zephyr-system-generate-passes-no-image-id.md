---
id: 1312
title: "`nros_system_generate.cmake` passes `--target zephyr-<rmw>`, which names no
  deploy block, so the tier resolver answers for the host, not Zephyr"
status: open
type: bug
area: zephyr, cli, codegen
severity: low
related: [issue-1285]
---

## What happens

`zephyr/cmake/nros_system_generate.cmake` calls `nros codegen-system` with
`--target "zephyr-${_rmw}"` (line 182; the comment at line 145 says it maps the
Kconfig RMW "to a --target string the CLI understands"). The tier resolver
(`tier_resolver::derive_target_rtos`) uses `--target` to pick a `[deploy.*]`
block and then reads that block's board id. `zephyr-zenoh` and its siblings
name no block, so no board id is resolved and the resolver falls back to the
HOST default. A Zephyr image baked through this module therefore gets
host-flavoured tier values rather than `[tiers.*.zephyr]` ones.

## Why it is latent

The two bringups the module bakes today declare no tiers, so no image output
changes. It becomes wrong the first time a tiered bringup is baked through this
path.

## History

Before the issue-1285 follow-up (PR #938), the resolver matched the RTOS by
substring. That PR made the resolver read board ids from the board catalog, and
it recorded this case as out of scope rather than reading "zephyr" out of the
`--target` string, which would reintroduce the substring guess.

## Fix

Have the module pass the IMAGE's identity. That is either the image/deploy-block
name the bringup's `system.toml` declares for this Zephyr image, or the board id
directly, if `codegen-system` grows a flag for it. Then the resolver answers
from the catalog: `native_sim/native/64` → zephyr, the mps2/an385 Zephyr boards →
zephyr. Check how the non-Zephyr callers (`nros build` → `codegen-system`) pass
it, and use the same spelling.

## Acceptance

A test bakes a bringup with a `[tiers]` table through the Zephyr module path,
or through its CLI invocation verbatim, and the generated tier code carries the
`[tiers.*.zephyr]` values. The test must fail with today's `--target
zephyr-<rmw>`.

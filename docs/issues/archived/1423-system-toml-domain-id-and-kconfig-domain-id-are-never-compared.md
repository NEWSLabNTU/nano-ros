---
id: 1423
title: "system.toml's domain_id is baked as NROS_SYSTEM_DOMAIN_ID and read by
  nothing; the Zephyr image takes CONFIG_NROS_DOMAIN_ID, and no layer compares
  the two"
status: resolved
resolved_in: phase-460 W4
type: bug
area: [cli, zephyr, cmake]
severity: medium
found: 2026-09-21
related: [issue-0794, issue-0934, phase-460, rfc-0049]
---

# 1423 -- two domain declarations, two readers, no comparison

Digest; the investigation is in the phase-460 doc (W4) and the fixing commit.

## What was observed

Brief B (2026-09-18) recorded the Autoware Safety Island with
`system.toml` saying `domain_id = 2` while `prj-cyclonedds.conf` baked
`CONFIG_NROS_CYCLONE_DOMAIN_ID=10` and the board `.config` carried
`CONFIG_NROS_DOMAIN_ID=10`. Every image built on 10; every document derived
from `system.toml` said 2. Corrected by a person reading the brief, which was
the only check that ran.

## The two roads (verified at 783cdfa14)

`codegen-system` resolves the domain through the `[deploy.<target>]` /
`[system]` ladder and writes `#define NROS_SYSTEM_DOMAIN_ID <n>u`, which no
source file under `zephyr/`, `cmake/`, `packages/api` or `packages/boards`
reads. The image's domain is `CONFIG_NROS_DOMAIN_ID` (`zephyr/Kconfig`,
default 0) via `nros/zephyr/app_config.h`; Cyclone's
`CONFIG_NROS_CYCLONE_DOMAIN_ID` defaults to it.

## What landed (phase-460 W4)

`nros_system_generate` (`zephyr/cmake/nros_system_generate.cmake`) compares
the `NROS_SYSTEM_DOMAIN_ID` it just baked against `CONFIG_NROS_DOMAIN_ID`
and, when in scope, `CONFIG_NROS_CYCLONE_DOMAIN_ID`, and refuses the
configure on disagreement naming all three and the `system.toml`. Agreement
is a STATUS line; a header with no define is fatal; Kconfig not yet in scope
is a WARNING naming the missing comparison. Precedence is unchanged: Kconfig
remains what the image bakes (RFC-0049, which gained a section saying so).

Acceptance: `tests/cmake-domain-agreement-tests.sh`
(`just check system-domain-agreement`) -- `CONFIG_NROS_DOMAIN_ID=2` against a
bringup declaring 10 fails the configure naming both; equal values pass (21
assertions; origin/main's shim fails 15 of them).

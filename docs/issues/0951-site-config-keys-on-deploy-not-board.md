---
id: 951
title: "`[deploy.*]` is three unrelated facts in one table — site config keyed
  on the deploy name, not the board, is the half that duplicates"
status: open
type: tech-debt
area: orchestration, tooling
related: [rfc-0065, rfc-0072, phase-383, issue-0842, issue-0606]
---

## Problem

`[deploy.<name>]` accumulated three facts that have nothing to do with each
other, and each wants a different key:

| fact | example keys | what it is really about |
| --- | --- | --- |
| PLACEMENT | `kind`, `nodes` | a MACHINE |
| build description | `board`, `rmw`, `profile`, `features` | an IMAGE |
| site config | `nros.sdk`, `nros.netstack` | a BOARD |

One table, three keys, so two of the three are always keyed wrong. The
measurable consequence is duplication: **30 authored `[deploy.<n>.nros]` blocks
across 8 files held exactly THREE distinct value-sets.**

```
13 blocks  {netstack=lwip,   sdk={freertos,lwip}}       boards: mps2-an385-freertos
13 blocks  {                 sdk={nuttx,nuttx_apps}}    boards: nuttx-qemu-arm, nuttx-qemu-riscv, qemu-armv7a-nsh
 4 blocks  {netstack=netxduo,sdk={threadx,netxduo}}     boards: threadx-linux
```

The 25 duplicates exist only because the deploy key is sometimes the friendly
name (`[deploy.freertos.nros]`) and sometimes the board spelling
(`[deploy.mps2-an385-freertos.nros]`) — **two keys for one board, which is two
places for one fact to drift.** `board_facts` grew machinery to compare the
candidates and refuse a disagreement, which is detection standing in for a
shape that should not be representable.

Issue 0842 named this in its title — "site config keys on deploy target, not
board" — for a workspace with two FreeRTOS boards whose site block was
reachable from the wrong one. Its root cause turned out to lie elsewhere (the
image inherited the default `rmw`), so the keying survived the fix.

## Why the deploy key is the wrong one, concretely

A deploy name and a board name are not in bijection, in either direction:

* `[deploy.threadx-linux]` ↔ `[image.threadx]`, `[deploy.an536]` ↔
  `[image.mps3_an536]` — a mechanical `deploy.X.nros → image.X.nros` rename
  loses three site blocks in `examples/workspaces/cpp` alone;
* `examples/workspaces/rust` declares eleven `board = "native"` images, so a
  per-image site table would multiply the duplication rather than remove it;
* `examples/workspaces/rust` also carries boardless `[deploy.freertos.nros]`
  and `[deploy.nuttx.nros]` blocks whose board is only discoverable through the
  same-named image.

## Fix

`[board_config.<board>]`, a nano-ros-owned table on `SystemToml`, holding the
existing `SiteConfig` struct. The key is resolved through
`BoardCatalog::resolve_deploy` — the same rule every other board spelling goes
through (issue 0606) — so an alias finds the block exactly as it does
everywhere else, and `[board_config.freertos]` and
`[board_config."mps2-an385-freertos"]` are the same block rather than two.

Two blocks disagreeing about one board becomes **unrepresentable** instead of
detected, which is why `board_facts`'s agree/disagree comparison now has
nothing to catch for that case. It still fires for the genuinely ambiguous one:
blocks on DIFFERENT boards when the caller named neither.

## Status

Landed so far:

* **placement → `[host.<name>]`** — rlm v0.1.21 added the table; 8 machine
  blocks migrated. (The commit that landed this, `70297f148`, cites `#0939` in
  its title by mistake; 0939 is an unrelated metadata-probe bug. This issue is
  the correct number, reserved after the fact.)
* **site config → `[board_config.<board>]`** — 30 blocks became 20, plus 4 the
  generator wrote for a workspace that had none.

Still open — the build half:

* 42 `kind = "embedded"` blocks whose build description moves to `[image.*]`;
* the readers that still consult `[deploy.*]`: `tier_resolver::derive_target_rtos`,
  `planner::schema_build_json`, `codegen_system`'s `.launch` fallback,
  `doctor::check_deploy_targets`, `check.rs`'s counter;
* `SystemToml::resolve_target` has no image rung, so a workspace that deletes
  its deploy blocks silently falls back to the `x86_64` / `native` / `debug`
  defaults;
* `synthesise_self_bringup` writes `image: Default::default()`, so any consumer
  switched to `[image.*]` loses its values for self-bringup packages;
* `DEPRECATED_DEPLOY_FIELDS` now records a DESTINATION per field, because the
  destination is not uniform — the first version sent `domain_id` and `locator`
  to `[image.*]`, a table with no such keys.

## Trap for whoever does the build half

`multi_board` in rlm is `deploy.values().any(|b| b.kind == "embedded")`, and it
forces `target = None` for every placed node. Deleting the embedded blocks
flips it to `false`, so a surviving `kind = "self"` block pins every node to
`Some(Target::Linux)`. That is benign — both `keep()` implementations compute
`board_mentioned` first, and a Linux-pinned model "mentions" only `native` and
`posix`, so an embedded board short-circuits to keeping everything, which is
the shape `examples/workspaces/features` already ships. The case to avoid is a
surviving NON-embedded block that names an MCU board: that makes
`board_mentioned` true and silently narrows every other board's entry to zero
nodes (issues 0356 / 0358 / 0320). No such block exists in-tree today.

## Verification used

Models are the artifact that would show an accident, and `nros sync` is
content-addressed — a plain sync serves a stale cache and reads as "no change".
So: `rm -rf <ws>/build/nros/models && nros sync` across all 31 workspaces,
before and after, then diff. For the site-config move the whole 456-line diff
was `sha256:` provenance lines and nothing else, which is the expected result:
site config is build environment and never reaches a SystemModel.

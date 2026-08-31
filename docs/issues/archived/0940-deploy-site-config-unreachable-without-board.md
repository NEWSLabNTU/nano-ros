---
id: 940
title: "`[deploy.freertos.nros]` in the mixed workspace is unreachable — the block names no `board`, so board-facts errors before reading it"
status: resolved
area: build
severity: medium
found: 2026-08-31
related: [0934, 0941, phase-405, RFC-0072]
---

# A site-config block nothing can reach

`examples/workspaces/mixed/src/demo_bringup/system.toml` declares:

```toml
[deploy.freertos]
kind = "embedded"          # ← no `board`

[deploy.freertos.nros]     # ← site config: netstack + SDK roots
netstack = "lwip"
sdk = { freertos = "{env:FREERTOS_DIR}", lwip = "{env:LWIP_DIR}" }
```

`nros ws board-facts` reaches a `[deploy.<t>.nros]` block one of two ways, and
BOTH require `t.board`:

* `--board <b>` matches against `t.board` (`board_facts.rs:274`);
* `--deploy <t>` then REQUIRES it —
  `target.board.clone().ok_or_else(|| eyre!("[deploy.{deploy_name}] names no `board`"))?`
  (`board_facts.rs:112`).

So this block is dead in both directions. The netstack choice and the two SDK
roots it declares never reach a build.

## Why it is not loud

`nros_resolve_board_facts` treats the failure as SOFT: a configure that cannot
resolve board facts prints a STATUS line and continues, so the image simply
builds without `NROS_BOARD` / `NROS_BOARD_TOML` / `NROS_NETSTACK` rather than
failing. `scripts/check-site-config.py` keys on the same field
(`if board not in BOARDS: continue`), so it stops CHECKING the block instead of
reporting it — the gate and the resolver share the blind spot.

## Found by trying to delete the field it depends on

phase-405 W5 set out to execute RFC-0065 D6's declared deprecation of
`[deploy.*]`'s build fields, of which `board` is one. Probing whether `board`
was removable is what surfaced that it is not merely a build field but the JOIN
KEY for site config — and that one block in the tree is already on the wrong
side of that join.

## Fix, and what it depends on

Two candidates, and the choice belongs with phase-405 W6:

1. **Give the block a board.** `[deploy.freertos] board = "…"` — smallest, but
   it re-states a fact `[image.freertos]` already carries, which is the
   duplication phase-405 exists to remove.
2. **Teach the resolver to reach a board through `[image.*]`**, so a deploy
   block needs no `board` of its own. This is the direction RFC-0065 D6 assumes
   when it says such blocks "become deletable once their `[image.*]` lands" —
   but the resolver does not read images, so as written the sentence is not yet
   true. **D6 should say "once the resolver reads the image".**

Either way, the gate blindness is worth fixing independently:
`check-site-config.py` should REPORT a `.nros` block whose deploy names no
board, rather than skipping it.

## Resolved — SUPERSEDED by #0951 (2026-08-31)

This was fixed twice, and the other fix is better.

phase-405 W6 deleted the 12 unreachable blocks and added an S4 rule to
`check-site-config.py` so a `[deploy.<t>.nros]` on a boardless target became an
error. That made the defect DETECTED.

#0951 made it UNREPRESENTABLE. Its reasoning, which is the right one:
`[deploy.<name>]` carried three unrelated facts — placement is about a MACHINE,
build description about an IMAGE, site config about a BOARD — so one table with
three keys always keyed two of them wrong. Site config moved to
`[board_config.<board>]`.

Their measurement: 30 authored `[deploy.<n>.nros]` blocks across 8 files held
exactly THREE distinct value-sets. The 25 duplicates existed because the deploy
key was sometimes the friendly name (`[deploy.freertos.nros]`) and sometimes the
board spelling (`[deploy.mps2-an385-freertos.nros]`) — two keys for one board.
30 blocks became 20.

So the boardless-deploy shape this issue describes cannot be written any more,
and the S4 rule guarding it was dropped rather than rebased: a gate for a
construct that no longer exists is worse than no gate, because it reads as
coverage.

Structural beats detection — the preference this repo states for itself (issue
0380: "the ban is the structural fix"). Recorded here rather than quietly
dropped, because the W6 commit message still describes the fix that lost.

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

## Resolved — phase-405 W6 (2026-08-31)

**It was twelve blocks, not one.** The sweep found the same shape in 7 bringups
across 6 workspaces, not only in `mixed`:

```
c/demo_bringup            [deploy.freertos]  [deploy.nuttx]
cpp/demo_bringup          [deploy.freertos]
mixed/demo_bringup        [deploy.freertos]
realtime-c/demo_bringup   [deploy.freertos]  [deploy.nuttx]
realtime-c/smp_bringup    [deploy.freertos]  [deploy.nuttx]
realtime-cpp/demo_bringup [deploy.freertos]  [deploy.nuttx]
rust/demo_bringup         [deploy.freertos]  [deploy.nuttx]
```

Ten of the twelve carried content byte-identical to a live board-named sibling
in the same file, which is why it had cost nothing. `rust`'s two had no sibling,
but that workspace has zero `nano_ros_entry` DEPLOY tokens, so nothing reached
them either.

The generator was NOT the source. `scripts/check-site-config.py` keys on
`board` and `continue`s past a boardless target, so it never emitted these
blocks — and never checked them. That blind spot is the actual defect, and it is
the class fix: **S4** now reports a `[deploy.<t>.nros]` whose target names no
`board`. The twelve blocks are deleted (-91 lines), and the gate grew a selftest
on the normal path, leaving `.config/gate-selftest-baseline.txt`.

Verified by MEASUREMENT, not by configure: `nros ws board-facts` was run over
every (bringup × board × --deploy/--board) pair — **192 probes** — against a
worktree at the pre-change commit and against the fixed tree. Output identical.
A deleted alias errors the same both sides (`[deploy.freertos] names no
'board'`), which is the direct proof it was already unreachable; every live
sibling resolves the same `NROS_BOARD` / `NROS_NETSTACK` / `NROS_SDK_*` it did
before.

Issue **0941** carries what this does not fix: `nros_resolve_board_facts` still
fails SOFT, so the next unreachable block will be just as quiet.

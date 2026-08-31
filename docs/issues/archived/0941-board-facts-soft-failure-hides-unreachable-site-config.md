---
id: 941
title: "`nros_resolve_board_facts` fails SOFT, so an unreachable site-config block is silent"
status: resolved
area: build
severity: medium
found: 2026-08-31
related: [0940, 0949, 0950, phase-405, RFC-0072]
---

# A resolution that cannot succeed prints STATUS and continues

`nros_resolve_board_facts` treats an unresolvable `[deploy.<t>.nros]` lookup as
SOFT: the configure prints a STATUS line and carries on, so the image builds
without `NROS_BOARD` / `NROS_BOARD_TOML` / `NROS_NETSTACK` rather than failing.

That is why issue 0940 was invisible. Not one block — **twelve**, across
**seven** bringups in six workspaces, every one of them declaring a netstack and
a pair of SDK roots that reached no build:

```
c/demo_bringup            [deploy.freertos]  [deploy.nuttx]
cpp/demo_bringup          [deploy.freertos]
mixed/demo_bringup        [deploy.freertos]
realtime-c/demo_bringup   [deploy.freertos]  [deploy.nuttx]
realtime-c/smp_bringup    [deploy.freertos]  [deploy.nuttx]
realtime-cpp/demo_bringup [deploy.freertos]  [deploy.nuttx]
rust/demo_bringup         [deploy.freertos]  [deploy.nuttx]
```

They survived because nothing said anything. Ten of the twelve happened to have
a byte-identical live sibling in the same file, so nothing broke either — the
configuration was redundant as well as dead, which is the only reason this cost
nothing so far.

## Why 0940 does not close this

Phase-405 W6 removes the twelve instances and stops the generator emitting a
block for a target that names no `board`. That is the CLASS fix for *those*
blocks. It does not change the fact that a configure which cannot resolve board
facts still proceeds quietly — the next way to produce an unreachable block is
still silent.

## The judgement this needs

Not every soft failure here is wrong. A native configure legitimately resolves
no board facts, and hardening this without separating those cases turns every
host build into an error. So the work is:

* enumerate which configures legitimately reach `nros_resolve_board_facts` with
  nothing to resolve;
* make the remainder loud — a `FATAL_ERROR` naming the deploy target and which
  of the two lookup paths was attempted (`board_facts.rs:112` vs `:274`);
* keep the STATUS line for the legitimate set, saying so explicitly rather than
  by omission.

Split from 0940 deliberately: 0940's fix is mechanical and lands now, this one
needs the enumeration first and should not hold it up.

## Resolved — phase-405 (2026-08-31)

**The enumeration first, because that was the whole risk.** `nros ws
board-facts` was scripted over every `(bringup x board x {--deploy, --board,
neither})` triple the cmake wrapper can ask — **414 probes** across the 16
workspaces in `examples/workspaces/`. Failures fell into six message classes,
and 357 of them are LEGITIMATE: a workspace that does not target a board, a
`native`/`robot1`/`zephyr` deploy that names no board and carries no site
config, a bringup with no deploy blocks at all.

That measurement produced the discriminator, and it is not the one this issue
guessed at. It is **not** "does the deploy name a board" — it is "does a
BOARDLESS deploy also declare a `.nros` block". One predicate, not an
enumeration of exceptions, which matters because enumerating exceptions is what
broke ordinary builds twice before.

**What changed.** `board_facts.rs` gained a `mod reason` — eleven
machine-readable codes emitted as a `board-facts[<code>]` prefix. cmake used to
classify by matching PROSE, which is a second opinion that drifts silently in
the benign direction. `_nros_board_facts_report` is now FATAL for the two
named-wrong codes, STATUS saying **"EXPECTED"** in words (and naming the three
variables it is deliberately not setting) for the four named-expected ones, and
WARNING for anything else — including an unrecognised message, so a new code
cannot land as a silent pass. `scripts/check-board-facts-reasons.py` cross-checks
the CLI's codes against cmake's arms in both directions, with a selftest on the
normal path that parses the REAL file (a `mod reason` that stopped parsing would
otherwise report zero of everything).

**Verified by configuring, not by grep** — `check fast` never runs cmake. Ten
real cmake cases with a fresh build dir each (the memo is `CACHE INTERNAL`, so a
reused dir answers the memo instead of re-running the verb): native → STATUS
rc=0; an orphan asked by `--deploy` → **FATAL rc=1** naming the deploy and the
path; the same orphan by `--board` → WARNING; a live sibling → five values
delivered. Then on real workspaces via `nros build` + `cmake -U
"NROS_BOARD_FACTS_ENV*"`: `mixed` native, `c` native, `c` freertos — all rc=0.

Confirmed on the post-0940 tree: the twelve former orphans now classify as
`deploy-names-no-board` (STATUS) rather than `unreachable-site-config` (FATAL),
and a synthetic orphan re-planted into `c/demo_bringup` trips BOTH the runtime
FATAL and the static `check-site-config.py` S4 rule. Defence in depth, on
purpose — S4 catches it in the index before a build ever runs.

**Deliberately not claimed:** no in-tree configure currently passes `--deploy
<boardless-with-.nros>` at a scope where the resolver runs, so the FATAL was
proven on a synthetic case. It is armed for the next such block, which is what
this issue asked for; it is not evidence that an in-tree build was breaking.

A drafted warning for "configure has `NANO_ROS_BOARD` but resolved a boardless
deploy" was **removed before landing** — it fired on every native workspace
configure, because `native` is itself a board name. The real signal is a
boardless `kind = "embedded"` deploy and belongs in the CLI; with no embedded
configure measurement behind it, it is a sentence in the STATUS message rather
than a verdict.

Two adjacent defects surfaced and were filed rather than folded in: **0949**
(for a migrated workspace `_ws` resolves to the generated root, which has no
`system.toml`, so board facts are never delivered at all) and **0950**
(`macro_deploy_token` hands an mps2 image `DEPLOY freertos`, and `--deploy`
outranks `--board`, so it resolves a boardless target while `--board` alone
would have answered).

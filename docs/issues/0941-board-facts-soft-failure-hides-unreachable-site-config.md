---
id: 941
title: "`nros_resolve_board_facts` fails SOFT, so an unreachable site-config block is silent"
status: open
area: build
severity: medium
found: 2026-08-31
related: [0940, phase-405, RFC-0072]
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

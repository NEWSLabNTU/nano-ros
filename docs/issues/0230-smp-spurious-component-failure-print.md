---
id: 230
title: "SMP: spurious 'ComponentNode failed at ? (code=0)' boot diagnostic while the component actually runs"
status: open
type: bug
area: core
related: [phase-292, rfc-0044]
---

## Summary

On the ASI FVP SMP-4 image (phase-292 W2 validation), every boot prints

```
[nros] FATAL: ComponentNode "controller" failed at ? (code=0) — halting boot
```

yet the component is fine: all entities created, spin proceeds, the full
closed-loop demo delivers. `what=?` and `code=0` mean the entry's
post-construct `ok()` check observed a false flag with NO recorded error
site — consistent with a cross-CPU visibility race between the core that
runs the component ctor (setting/clearing the error state) and the core
that runs the entry's check, with no synchronization on the flag.
Single-core images never print it.

## Impact

Cosmetic-but-scary: a FATAL line that claims "halting boot" on a healthy
boot. Also implies the check can MISS a real failure the same way.

## Fix direction

Make the ComponentNode error state (`ok_` flag + site + code) either
atomics with acquire/release, or force the check onto the constructing
thread / behind an explicit barrier in the entry seam.

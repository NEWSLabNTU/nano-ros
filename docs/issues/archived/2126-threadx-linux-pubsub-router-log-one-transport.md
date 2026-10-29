---
id: 2126
title: "threadx_linux Rust pubsub cell finds one `New transport opened` line where two nodes connected"
status: resolved
type: bug
area: [testing, ci]
severity: low
found: 2026-10-29
related: [phase-466, 2125]
resolved: 2026-10-29
---

**Home: [phase-466](../../roadmap/phase-466-scheduled-lanes-produce-verdicts.md)**, since 2026-10-29.

## Measured

`test_rtos_pubsub_e2e::platform_3_Platform__ThreadxLinux::lang_1_Lang__Rust`
failed in both tier-2 nightlies on 2026-10-28 (40636297544, 40648207713):

```
[threadx_linux rust] messages published: 70, received: 70 (need 70)
[threadx_linux rust] router sessions opened: 1 (max 3)
... the router log records fewer than two `New transport opened` lines, yet both nodes demonstrably talked to it
```

Delivery was complete. Only the session count was wrong.

## Cause (believed)

The same as [issue 2125](2125-freertos-quiet-link-readiness-missing.md). The
quiet-link cell for threadx-linux runs the same listener image, baked with
router port 9000, and ran alongside this cell. Its router start killed this
cell's router and re-created `zenohd-9000.log`, so the log this cell read
held only what the newcomer had recorded.

Not separately confirmed: the nightly kept no router log from the failing run.

## What would close it

The 2125 fix, and a tier-2 nightly where this cell passes. If it still reads
one transport with no concurrent cell on port 9000, the cause is something
else and this issue reopens on its own.

## Resolution

No code of its own. The port collision fixed for issue 2125 in #2802 was the cause.

Confirmed by the tier-2 nightly of 2026-10-29, run [40833557587](https://github.com/NEWSLabNTU/nano-ros/actions/runs/40833557587): `test_rtos_pubsub_e2e::platform_3_Platform__ThreadxLinux::lang_1_Lang__Rust` passed in 72.6 s.

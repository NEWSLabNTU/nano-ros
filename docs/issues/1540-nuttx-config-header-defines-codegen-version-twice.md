---
id: 1540
title: "The NuttX config snapshot defines `NROS_CODEGEN_VERSION` twice; both gates read the first, GCC uses the last"
status: open
type: bug
area: [build, api]
severity: medium
found: 2026-09-28
related: [phase-472, 1115, 0088]
---

## What happens

`packages/api/nros-c/include/nros/nros_config_generated_nuttx.h`:

```
28:  #define NROS_CODEGEN_VERSION 8
29:  #define NROS_CODEGEN_VERSION_MIN 2
113: #define NROS_CODEGEN_VERSION 8
114: #define NROS_CODEGEN_VERSION_MIN 2
```

VERIFIED: both definitions are present, and with both at `8` GCC reports
`int v = 8;` and does not warn, because an identical redefinition is legal.

## Why it is armed rather than broken

Today the two agree. `check-config-header-producers` and
`check-config-fallback-macros` both read the FIRST match
(`re.search(r"^#define\s+NAME\s+(\S+)")`); GCC uses the LAST. A version bump
that edits line 28 alone passes both gates while every NuttX image compiles
against line 113. Demonstrated by the phase-472 bucket-01 auditor: a half-applied
bump passes both gates and GCC still sees `8`.

That is issue 1115's exact shape — a NuttX-only config snapshot silently
diverging from the per-build header — which already cost two days of NuttX
builds once.

## Fix

Delete the duplicate. Both gates refuse more than one definition of a macro.
Phase-472 W6.

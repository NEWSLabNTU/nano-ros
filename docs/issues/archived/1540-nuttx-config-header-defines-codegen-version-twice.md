---
id: 1540
title: "The NuttX config snapshot defines `NROS_CODEGEN_VERSION` twice; both gates read the first, GCC uses the last"
status: resolved
resolved: 2026-09-28
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

## Resolution (2026-09-28)

**Why there were two.** Two fixes for the same nightly `nuttx` break were
authored 19 s apart on 2026-09-07: `db1fc637c0` (phase-413 W2) added the pair
near the head of the file, and `c8ad7c3508` (issue 1115) added it after the
size macros. They touched different hunks, so the second rebased onto the
first without a conflict and both copies landed. Nothing generates this file,
so the fix belongs in the file and the gates, not in a producer. The first
block also named a gate that does not exist
(`check-nuttx-fallback-config-macros`). That is the block removed.

**Scope checked.** The C++ twin `nros_cpp_config_generated_nuttx.h` defines
the pair once (only 1115 touched it). No other macro is duplicated in either
snapshot. `NROS__NUTTX_FALLBACK_ASSERT` has three definitions, one per
`#if`/`#elif`/`#else` arm.

**Gates.** `check-config-header-producers` reads every definition of the pair.
It refuses a header or template producer that defines either one more than
once, and it compares every literal copy against `codegen_version.rs`. `.rs`
producers are exempt from the count only: `nros-build-helpers/src/cpp.rs`
holds two inline emitters. `check-config-fallback-macros` refuses any macro
defined more than once outside a conditional arm of a reachable fallback,
whatever the value, and its exact-value check reads the LAST definition, the
one that compiles.

**Mutation evidence** (anchors checked with `git diff`; `gcc -E -P
-DNROS_PLATFORM_NUTTX` through the dispatching stub):

| Mutation | producers | fallback-macros | gcc sees |
| --- | --- | --- | --- |
| clean tree | 0 | 0 | 8 / 2 |
| extra `#define NROS_CODEGEN_VERSION 9` after the real one | 1 | 1 | 9 |
| original file, line 28 -> 9, `codegen_version.rs` -> 9 | 1 | 1 | 8 |
| original file, copies agree | 1 | 1 | 8 |

On the half-applied bump, with the C++ twin bumped too so that only the C
duplicate is in play, the pre-fix gates return 0 and the new ones return 1.

**Class sweep.** A scan for unconditional duplicate `#define`s over the 270
tracked headers and templates outside `third-party/` found one more:
`packages/boards/nros-board-threadx-qemu-riscv64/config/nx_user.h` defines the
value-less flag `NX_ENABLE_EXTENDED_NOTIFY_SUPPORT` twice. A flag with no
value cannot diverge, so it was left as it is.

**Not verified.** No NuttX image was built. `just check c` passes, but that
lane never reaches the NuttX snapshot; the header was checked with `gcc -E`
only.

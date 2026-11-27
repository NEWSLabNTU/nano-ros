---
id: 2998
title: "The stack-floor walk types almost no C on aarch64: clang rejects GCC's `-mabi=lp64`, so 370 of 374 TUs keep the coarsest binding"
status: open
type: bug
severity: medium
area: [stack-floor, tooling, zephyr]
related: [issue-2897, issue-2658, phase-592]
found: 2026-11-27
---

## What happens

The walker types C call sites by recompiling each linked C translation unit
with clang from its GCC compile command (`clang_c_command` in
`scripts/nros-stack-floor.py`), keeping an allowlist of target flags. On a
`qemu_cortex_a53` image (realtime-c `smp_sntp_bringup`, zephyr-sdk 0.16.9)
the walk reports:

```
bind: C call-site types from 4 of 387 linked C TU(s); 383 clang cannot compile
keep every function C can hold ... (duration.c: unknown target ABI 'lp64'; ...)
```

Every function those TUs define keeps the coarsest binding ("by machine"),
which is why the image's floors run through edges such as
`dns_resolve_name -> publish_trampoline` (issue 2897's table: 166-174 KB
upper bounds against 8-32 KB stacks).

## Cause

The allowlist keeps `-mabi=...` because it is ABI-defining on Arm and
RISC-V, where clang accepts GCC's values. On aarch64 GCC passes
`-mabi=lp64`, its default, and clang rejects it ("unknown target ABI"):
clang's aarch64 target has no `-mabi` values of GCC's spelling.

## Fix direction

The class, not the flag: a target flag clang rejects for the target is
dropped only when it is a no-op for GCC itself (GCC predefines the same
ABI-fixing macros with and without it); otherwise it is kept and the TU
stays untyped, with the reason. Then re-measure the a53 walk and the typed
TU count.

## Acceptance

On the `workspace-zephyr-c-realtime-smp-sntp-resync` fixture, the typed C
TU count is near the linked count, and `-mabi=ilp32` (which changes the
ABI) is still refused rather than dropped.

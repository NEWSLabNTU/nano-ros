---
id: 2998
title: "The stack-floor walk types almost no C on aarch64: clang rejects GCC's `-mabi=lp64`, so 370 of 374 TUs keep the coarsest binding"
status: resolved
resolved: 2026-11-27
resolved_in: "the change that adds this line"
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

## Fixed (2026-11-27)

The class, not the flag. Each distinct GCC target-flag set is probed once
(`clang_target_flag_drops`): a flag clang rejects for the target is dropped
only when it is a no-op for GCC itself, i.e. GCC predefines the same
ABI-fixing macros (`__SIZEOF_*__`, `__LP64__`/`__ILP32__`, char and wchar
signedness, the Arm PCS and FP macros, RISC-V float ABI and xlen, byte
order) with and without it. Otherwise it is kept and the TU stays untyped,
with the reason in the walk's note. Comparing GCC against clang instead
does not work: the two spell `__ARM_FP` and `__WCHAR_UNSIGNED__`
differently whatever the flags.

Probed with the real aarch64-zephyr-elf GCC (zephyr-sdk 0.16.9) and clang:
`-mabi=lp64` is dropped; `-mabi=ilp32` is kept (it changes `long`,
`__LP64__`, `__ILP32__`).

Measured on the `workspace-zephyr-c-realtime-smp-sntp-resync` fixture
(host 133):

| | before | after |
| --- | ---: | ---: |
| C TUs typed | 4 of 387 | 387 of 387 |
| C sites typed by clang | 0 | 333 |
| sites bound to every function C can hold | 370 | 37 |
| OPEN floor | 169,368 B | 133,992 B (-21 %) |
| SNTP_RESYNC floor | 173,592 B | 133,008 B (-23 %) |
| TASK_HIGH floor | 172,744 B | 138,904 B (-20 %) |

`report.json` gains `c_typing` (`{linked, typed, dropped_flags}`), and
`tests/zephyr_stack_floor_aarch64.rs` asserts `typed == linked` on that
fixture. The floors are still upper bounds; the rest is RFC-0143's.

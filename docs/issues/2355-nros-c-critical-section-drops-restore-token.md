---
id: 2355
title: "`nros-c`'s `critical_section` impl releases with a literal 0 on every target but arm and aarch64, and on ThreadX riscv64 0 means `TX_INT_DISABLE`"
status: open
type: bug
area: [api, platform, threadx]
severity: low
found: 2026-11-07
related: [issue-2340, phase-548]
---

**Home: none yet.** Found while tracing issue 2340. It played no part in
that issue.

## What it is

`packages/api/nros-c/src/lib.rs`, `mod platform_critical_section`: where
`critical_section::RawRestoreState` is `()` (every target but arm and
aarch64, which select `restore-state-u32`), `acquire_key` discards the token
`nros_platform_critical_section_acquire()` returned, and `release_key` calls
`nros_platform_critical_section_release(0)`.

The token is the port's prior posture, and only the port knows what 0 means.
On ThreadX riscv64, `nros_platform_critical_section_release(token)` is
`tx_interrupt_control(token)`, and `TX_INT_DISABLE` is `0x00000000`
(`nros-board-threadx-port-riscv64/port/inc/tx_port.h`). So the first critical
section an image entered through this impl would leave interrupts masked for
good. That is the exact symptom of issue 2340, from a different cause.

## Why it is not fixed yet

It is not measured on an image that runs. The rv-virt-threadx C++ talker that
issue 2340 traced links no `critical_section` consumer: `nm` finds no
`_critical_section_1_0_acquire`. A Rust image on that board gets
`nros-platform-critical-section` instead, which selects `restore-state-u32`
and passes the token through. Which C or C++ image on a non-arm,
non-aarch64 target reaches this impl, if any, has to be found before it is
changed. The rv-virt-threadx C service client that issue 2357 traced links
none either, which is how that red was ruled out as this bug. On hosted targets, `restore-state-bool` also arrives through
`critical-section/std`, and the arch gating in `nros-c`'s and `nros-cpp`'s
`Cargo.toml` exists because of that conflict.

## Next

Find a consumer: link an image whose graph calls `critical_section::with`
on riscv64 ThreadX through `nros-c`. Then either select `restore-state-u32`
for `all(target_arch = "riscv64", target_os = "none")` in both manifests, or
keep the tokens in the impl, one per nesting level, under the section.

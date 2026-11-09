---
id: 2355
title: "`nros-c`'s `critical_section` impl releases with a literal 0 on every target but arm and aarch64, and on ThreadX riscv64 0 means `TX_INT_DISABLE`"
status: resolved
type: bug
area: [api, platform, threadx]
severity: low
found: 2026-11-07
resolved: 2026-11-09
resolved_in: "nros-c keeps each port token in a per-nesting-level stack on targets whose RawRestoreState is not u32, and releases with the saved token"
related: [issue-2340, phase-548]
---

**Home: [phase-548](../../roadmap/phase-548-three-languages-every-board.md)** Found while tracing issue 2340. It played no part in
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

## Resolution (2026-11-09)

The class is "a release that does not restore the posture its acquire
saved". Every `critical_section::Impl` in the tree was checked:

- `nros-platform-critical-section` selects `restore-state-u32` on every
  target, so the token rides in `RawRestoreState`.
- `nros-platform-mps2-an385`'s PRIMASK impl returns and honours its own
  token.
- `nros-cpp` registers no impl. Its runtime is `nros-c`'s.
- `nros-c` on arm and aarch64 selects `restore-state-u32` and passes the
  token through.
- `nros-c` everywhere else was the one bad site, and the only literal
  passed to `nros_platform_critical_section_release` anywhere outside
  `third-party/`.

On the C side, every port's release is correct once it gets the right
token. ThreadX (`tx_interrupt_control(token)`) and Zephyr
(`irq_unlock(token)`) restore it. On Zephyr riscv a literal 0 was the same
bug. FreeRTOS (`taskEXIT_CRITICAL`, which keeps its own nesting count) and
POSIX (a recursive mutex) ignore the token.

The fix does not select `restore-state-u32` for more targets. That would
re-open the `restore-state-bool` conflict that `critical-section/std`
brings onto hosted targets. Instead, `nros-c` keeps the port's token
itself: `saved_cs_tokens::TokenStack` holds one slot per nesting level
(`MAX_NESTING` = 16) and is touched only inside the section, so the
section is its lock. The matching release hands the saved token back. A
nesting deeper than 16 releases the section it just entered and then
panics, so there is no unpaired release.

The consumer this issue asked for has still not been found: no in-tree C
or C++ image on a non-arm target was found to link `nros-c`'s impl. So the
proof is the mechanism, under a model of the ThreadX port:
`nested_sections_on_a_mask_port_end_with_interrupts_as_they_were` (a
`tx_interrupt_control`-shaped port, three levels deep; interrupts are
unmasked only by the outermost release) and
`each_release_gets_the_token_of_its_own_acquire` (LIFO pairing and the
nesting bound). On the old code, the first test's final assertion fails,
because a release with 0 leaves `TX_INT_DISABLE`.

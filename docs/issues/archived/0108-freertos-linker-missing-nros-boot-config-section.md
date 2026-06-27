---
id: 108
title: "FreeRTOS MPS2-AN385 linker script omits `.nros_boot_config` → overlaps `.data`, fixture build fails"
status: resolved
type: bug
area: freertos
related: [phase-266, rfc-0045]
resolved_in: "5a6407bd2 (2026-06-28)"
---

> **RESOLVED (2026-06-28, `5a6407bd2`).** Added a `.nros_boot_config :
> { KEEP(*(.nros_boot_config*)) } > FLASH` output section to
> `nros-board-mps2-an385-freertos/config/mps2_an385.ld`, placed before `.data`
> (between `.eh_frame_hdr` and `.eh_frame`, so it precedes `_etext`) — mirroring
> the `.eh_frame_hdr` fix the same script already carried for this overlap class.
> Verified end-to-end: `just freertos::build-examples` builds `qemu_freertos_entry`
> green (links + `built:`), zero `.nros_boot_config overlaps .data`. (Needed
> `CARGO_INCREMENTAL=0` to dodge an unrelated local rustc incremental-cache ICE.)

## Summary

`just ci` → `build-test-fixtures` fails on the FreeRTOS lane: linking
`qemu_freertos_entry` (the `examples/qemu-arm-freertos/rust/*_entry` workspace
entries) dies with

```
rust-lld: error: section .nros_boot_config load address range overlaps with .data
error: could not compile `qemu_freertos_entry` (bin)
```

The `.nros_boot_config` output section is **not placed** by the FreeRTOS board's
linker script, so `rust-lld` drops it at the default output address, which
overlaps `.data`/RAM.

## Cause

Phase-266 (`8088e77c0`, "266-W4c: OwnedSpin embedded boards take node name from
`.nros_boot_config`") introduced a build-time-baked boot config that
`nros::main!()` emits into a `#[link_section = ".nros_boot_config"]` static
(see `packages/core/nros-platform-api/src/boot_config.rs`, RFC-0045). Every
linker layout that targets an embedded entry must give that read-only section a
FLASH load address.

The bare-metal / RTIC MPS2-AN385 entries link with cortex-m-rt's `link.x`, which
absorbs the section. The **FreeRTOS** entries instead link the board-owned
`packages/boards/nros-board-mps2-an385-freertos/config/mps2_an385.ld`
(`-Tmps2_an385.ld`, via each entry's `.cargo/config.toml`), and that script was
never updated for the new section — `grep -c nros_boot_config mps2_an385.ld` = 0.

This is the **same bug class** the script already documents + fixed for
`.eh_frame_hdr` (its comment: an unplaced read-only section "falls to the default
output and overlaps `.data`/RAM ... `section .eh_frame_hdr load address range
overlaps with .data`"). `.nros_boot_config` just wasn't added alongside it.

NOT related to `[patch.crates-io]` / issue 0094/0100 — it's purely linker memory
layout; surfaced during the post-issue-0100 `just ci` (2026-06-27).

## Fix direction

In `packages/boards/nros-board-mps2-an385-freertos/config/mps2_an385.ld`, add a
`.nros_boot_config` output section in FLASH **before** `.data`, mirroring the
existing `.eh_frame_hdr` placement, e.g.:

```ld
    /* Baked boot config (phase-266 / RFC-0045) — read-only; MUST get a FLASH
     * load address or it overlaps .data, same as .eh_frame_hdr below. */
    .nros_boot_config :
    {
        KEEP(*(.nros_boot_config))
        KEEP(*(.nros_boot_config*))
    } > FLASH
```

Verify with `just ci` → `build-test-fixtures` (FreeRTOS lane) or a direct
freertos-entry fixture build. Audit the other RTOS boards that ship a hand
linker script (threadx, nuttx, esp32) for the same missing section.

## Evidence

Found running `just ci` after the issue-0100 example collapses landed
(2026-06-27). `check` + `rust-rtos-link-check` pass; the failure is isolated to
the FreeRTOS workspace-entry link step in `build-test-fixtures`. The `nros`
build itself + every other lane (qemu, nuttx) compile clean.

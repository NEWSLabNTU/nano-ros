---
id: 2309
title: "mps2-an385's 128 KiB HEAP static lands in .data, so it costs flash, while its comment says .bss"
status: open
type: bug
area: platform, memory
purpose: [industrial, education]
related: [phase-523, issue-2265]
found: 2026-11-07
---

**Home: [phase-523](../roadmap/phase-523-education-labs-adoption-probes.md)** — found while answering issue 2265 (probe stop I15).

## Measured

`build/cargo-fixtures/qemu-arm-baremetal/thumbv7m-none-eabi/nros-relwithdebinfo/qemu-serial-talker`,
read with rustup's `llvm-nm -S -C` and `llvm-size -A`:

```
20000018 000206b8 D nros_platform_mps2_an385::memory::HEAP
.data                  134248   536870912
.bss                   237092   537005168
```

`D` is initialised data. The 132,792-byte allocator is therefore in `.data`,
whose load image sits in flash and is copied into RAM at reset: 128 KiB of
flash and a 128 KiB copy at every boot, for an arena that starts empty.
`packages/platform/nros-platform-mps2-an385/src/memory.rs` says the opposite:
"`HEAP` is `.bss` (zero-init → no flash cost)".

Likely cause, not yet confirmed: `zpico_alloc::FreeListHeap::new()` writes a
non-zero header (a free-list head, a size), so the static's initialiser is not
all zeroes and the compiler places it in `.data`.

## Next

Make the arena zero-initialised (initialise the header lazily on first
allocation, or keep the header out of the arena static), then assert the
placement: `llvm-nm` reports `B`/`b` for `HEAP`, and `.data` shrinks by about
128 KiB. Check the other bare-metal platforms that hold a `FreeListHeap`
static the same way, since this is a property of the allocator, not of the
board.

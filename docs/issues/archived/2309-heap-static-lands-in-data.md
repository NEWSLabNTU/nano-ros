---
id: 2309
title: "mps2-an385's 128 KiB HEAP static lands in .data, so it costs flash, while its comment says .bss"
status: resolved
type: bug
area: platform, memory
purpose: [industrial, education]
related: [phase-523, issue-2265]
found: 2026-11-07
resolved_in: "not a defect: measured on a stale fixture; a fresh image places HEAP in .bss"
---

**Home: [phase-523](../../roadmap/phase-523-education-labs-adoption-probes.md)** — found while answering issue 2265 (probe stop I15).

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

## Resolution (2026-11-07): not reproduced, filed from a stale image

The measured image was built on 2026-08-28. It predates phase-391 W3
(`60b4e0c1eb`, the same day), which made `FreeListHeap::new()` all-zero by
inverting `slab_used_bitmap` so that 0 means free. The one nonzero byte in the
measured initialiser was exactly that field: `0xff` at offset `0x206b5`, the
old "set = free" sense.

A fresh build from `main` (`nros new hs --board mps2-an385-baremetal
--template talker`, then `nros build`) measures:

```
20007fc0 000206d0 b nros_platform_mps2_an385::memory::HEAP
.data                    1980
.bss                   172540
```

`b` is `.bss`. The heap costs no flash, and the allocator's comment was right.
The book's static-memory page, which repeated the claim, is corrected (132,816
B, `.bss`).

The process lesson is the one CLAUDE.md already states (issues
0859–0862): check an artifact's mtime against the code it is blamed on before
filing. This issue skipped that step. No guard is added: asserting "all-zero"
in a unit test would read the struct's padding, which is undefined behaviour.
The fresh-build measurement above is the evidence.

---
id: 1977
title: "Every mps2-an385 bare-metal C/C++ image panics with `memory allocation of 128 bytes failed` right after Ethernet comes up"
status: resolved
type: bug
area: [baremetal, executor]
severity: high
found: 2026-10-19
resolved: 2026-10-19
related: []
---

## What

`test_qemu_baremetal_capi_e2e` fails all six cases (C and C++ pub/sub, service,
action). Every image boots, brings up LAN9118, prints `Ethernet ready.`, and
then:

    nros: PANIC panicked at …/library/alloc/src/alloc.rs:665:9:
    memory allocation of 128 bytes failed

so the listener never prints `I heard:`.

## Reproduced on clean main

Measured on `origin/main` at 941132a82c (no local changes), fixtures freshly
built with `just qemu build-fixtures`:

    cargo nextest run -p nros-tests --test emulator \
        test_qemu_baremetal_capi_e2e::case_1_c_pubsub \
        test_qemu_baremetal_capi_e2e::case_3_cpp_pubsub

Both fail, with the panic above in every image. The C++ cases passed 3/3 on the
phase-506 W4c branch (based before 798e717835), so the regression landed after
that.

## Suspect (not confirmed)

phase-397 W2 (3e2bf80531, 1b8dde47fd) is the only executor change in that window
that adds a heap allocation on the spin path (`Arc::new(WakeCtx)` in
`wake_ctx_ptr`), on a target whose heap is small. A 128-byte request failing
says the arena is already exhausted rather than that this request is large, so
the allocation that tips it may be anywhere. Next step: bisect 798e717835..main
on `case_3_cpp_pubsub`.

## Resolution (2026-10-19)

Not heap exhaustion. Phase-397 W2 made the executor's `WakeCtx`
`#[repr(align(64))]` (an entity waker carries its slot in the pointer's low
bits) and put it in an `Arc`, so `Arc::new` asks for a 128-byte, 64-aligned
block. `PlatformGlobalAllocator` answered every `align > 8` with null (the
platform ABI has no alignment parameter), which `handle_alloc_error` reports as
"memory allocation of 128 bytes failed". A host `std` build uses the system
allocator, so only the no_std images broke.

Measured, not inferred:

- **bisect** 798e717835..941132a82c on `case_3_cpp_pubsub`: 68840b9add good,
  1b8dde47fd bad, with 3e2bf80531 and 4b4ddc65a1 unbuildable for thumbv7m
  (`AtomicU64`, which 1b8dde47fd fixed) — so phase-397 W2 as a unit.
- **not a size bump**: the same image with `NROS_HEAP_SIZE=1048576` (the
  symbol table shows `FreeListHeap<1048576>`) failed on the same 128 bytes.

Fixed by 6286759329 ("the global allocator serves over-aligned layouts"),
found independently on an S32K344 under Zephyr: an over-aligned request is
carved out of a `size + align` raw block with the raw pointer stored just below
the aligned one. On d5fe58a518, `test_qemu_baremetal_capi_e2e` passes 6/6
(C and C++ pub/sub, service, action), freshly built fixtures.

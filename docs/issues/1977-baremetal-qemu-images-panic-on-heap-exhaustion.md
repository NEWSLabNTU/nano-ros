---
id: 1977
title: "Every mps2-an385 bare-metal C/C++ image panics with `memory allocation of 128 bytes failed` right after Ethernet comes up"
status: open
type: bug
area: [baremetal, executor]
severity: high
found: 2026-10-19
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

---
id: 342
title: "orchestration_tiers_freertos bypasses both sanctioned seams: a hand-rolled qemu command with no bypass rationale, and the only bare port literal among 14 start_slirp call sites"
status: open
type: bug
severity: medium
area: testing
related: [rfc-0051, issue-0327]
---

## Finding (quick audit E8/E9 + deep audit E9, 2026-07-28)

One test file bypasses both test-harness seams, and in each case a sibling in the
**same file** shows the sanctioned way.

### 1. Hand-rolled emulator invocation (E9)

`packages/testing/nros-tests/tests/orchestration_tiers_freertos.rs:68` —
`multi_tier_freertos_firmware_builds_and_boots_run_tiers` runs

```rust
Command::new("timeout").args(["10","qemu-system-arm","-cpu","cortex-m3","-machine","mps2-an385", …])
```

outside the `nros_tests::qemu` interpreter, with **no sanctioned-bypass doc-comment**
(the E1-exception pattern used elsewhere, e.g. `zpico_drift_gate.rs:22`).

The next test in the same file (`:119`) uses
`QemuProcess::start_mps2_an385_networked` — which proves the interpreter covers this
board, so the bypass is not a capability gap. It also means this invocation misses
whatever the interpreter centralises: the `-icount shift=auto` convention
(`docs/reference/qemu-icount.md`), boot-deadline handling, and log capture.

### 2. The only bare port literal among 14 call sites (E8)

`orchestration_tiers_freertos.rs:116` — `ZenohRouter::start_slirp(7447)`.

Every one of the other 13 `start_slirp` call sites passes an allocator constant or a
derived helper. 7447 also lands **inside** the allocator's own FreeRTOS window
(7400–7799, platform index 1), so it can collide with an allocator-assigned port as
the matrix grows — the exact class phase-295 W4 eliminated by routing every baked port
through `nros_tests::alloc`. The fixture's deploy locator is hand-mirrored in a comment
at `:114` (`tcp/10.0.2.2:7447`), so the test and its fixture can silently disagree.

## Fix

1. Route the boot-only test through `qemu::QemuProcess` — add a non-networked
   `start_mps2_an385` variant with a boot deadline if one is missing — or add the
   E1-exception-style rationale the header currently lacks, naming why the interpreter
   cannot serve it.
2. Derive the port from `nros_tests::alloc::port_of(FreertosMps2, Rust, <workload>)`
   and re-bake the fixture's deploy locator from the same call, so the hand-mirrored
   comment at `:114` becomes unnecessary.

## Note

Both halves were found in the quick run and recorded in
`docs/development/audit-findings-2026-07-28.md`; the deep run re-derived the E9 half
independently and added the "sibling test in the same file uses the interpreter"
evidence. Filed now because neither half got an issue in the quick run's batch.

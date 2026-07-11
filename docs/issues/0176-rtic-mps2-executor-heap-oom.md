---
id: 176
title: "RTIC mps2-an385 images OOM at runtime — executor backing (74888 B) exceeds the 64 KB default heap"
status: open
type: bug
area: baremetal
related: [issue-0163, phase-271]
---

## Summary

Every `deploy = "rtic-*"` example on `qemu-arm-baremetal` (mps2-an385) **boots
but panics at runtime** with a heap OOM the moment the executor is opened:

```
memory allocation of 74888 bytes failed
```

Downstream, the RTIC e2e tests then fail their delivery asserts (e.g.
`RTIC QEMU action client: goal was not accepted`) because the image aborted
before publishing. All four run tests fail identically:

- `test_qemu_rtic_pubsub_e2e`
- `test_qemu_rtic_service_e2e`
- `test_qemu_rtic_action_e2e`
- `test_qemu_rtic_mixed_priority_pubsub_e2e`

(`packages/testing/nros-tests/tests/emulator.rs`; nextest, 4/4 FAIL.)

## Root cause

The mps2-an385 static heap defaults to **64 KB** for zenoh-pico/xrce builds
(`nros-platform-mps2-an385/src/memory.rs` `DEFAULT_HEAP_SIZE`, the
non-`link-tls`/non-`dds-heap` arm). The RTIC boot path opens the executor with
a backing storage of **74888 bytes** — a *single* allocation larger than the
whole 64 KB heap → the free-list allocator returns null → `alloc` panics.

The `memory.rs` header comment records that 64 KB was historically sufficient
(a zenoh-pico `tcp/` client's working set is ~12–16 KB), so the 74888-byte
executor backing is a **growth** relative to that budget — likely the
`Executor<'s>` per-entry storage rework (phase-271 / #110) or a later
executor-storage bump. Non-RTIC mps2 examples pass because their boot path
does not allocate the full executor backing on the heap the same way.

This was latent until now: rtic 2.3.0 made the `nros::main!` RTIC `#[local]`
resources fail to compile (fixed separately — the `__NrosLocalCell` Send shim),
so these images never built/ran and the OOM was masked behind a build failure.

## Reproduce

```
just build-test-fixtures     # (qemu lane builds the rtic fixtures)
cargo nextest run --manifest-path packages/testing/nros-tests/Cargo.toml \
    -E 'test(/test_qemu_rtic/)'
# → all 4 fail: "memory allocation of 74888 bytes failed"
```

## Fix direction (needs a decision)

MPS2-AN385 has ample RAM (16 MB), and the `HEAP` static is `.bss` (zero-init →
no flash cost), so either fix is cheap:

1. **Raise the default heap** for the non-tls/non-dds arm in
   `nros-platform-mps2-an385/src/memory.rs` (e.g. 64 → 128 KB, matching the
   `link-tls` arm). Broad but safe; every non-configured mps2 example gets the
   headroom. `NROS_HEAP_SIZE` still lets a size-critical node shrink it.
2. **Set `NROS_HEAP_SIZE` per RTIC example** (`.cargo/config.toml` `[env]`) to
   ~128 KB. Contained to the RTIC examples.
3. **Investigate the executor-storage growth** — confirm whether 74888 B is
   expected for the RTIC entry or whether the RTIC path over-allocates vs the
   non-RTIC `Executor::open` path, and right-size it. This is the real
   root-cause fix; (1)/(2) are the pragmatic unblock.

Any of these needs a qemu-fixture rebuild + a rerun of the four
`test_qemu_rtic_*_e2e` to verify.

## Notes

- Not caused by the rtic-2.3.0 Send fix (a zero-cost `__NrosLocalCell` newtype
  cannot change allocation size); that fix only let the images build again,
  exposing this.
- `just check` (build/clippy) is green — this is a runtime-only defect on the
  `test-all` e2e lane.

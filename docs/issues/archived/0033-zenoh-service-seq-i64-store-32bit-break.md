---
id: 33
title: zenoh service shim stores i64 reply-seq into AtomicI32 — breaks all 32-bit embedded targets
status: resolved
type: bug
area: rmw
related: [phase-237]
resolved_in: "SeqScalar width cast (nros-rmw-zenoh/src/shim/{mod,service}.rs)"
---

**Problem.** `nros-rmw-zenoh` failed to compile for **any 32-bit target**
(riscv32 / esp32, Cortex-M / stm32 / freertos / threadx-arm) after the Phase
237 seq-keyed reply work landed:

```
error[E0308]: mismatched types
  --> packages/zpico/nros-rmw-zenoh/src/shim/service.rs:223:20
223 |     slot.seq.store(seq, Ordering::Relaxed);
    |              ----- ^^^ expected `i32`, found `i64`
```

**Cause.** `AtomicSeqCounter` is width-split (`shim/mod.rs`):
`AtomicI64` on `target_has_atomic = "64"`, `AtomicI32` otherwise (32-bit has no
native 64-bit CAS). The FFI `zpico_queryable_take_reply_seq` always returns
**i64**, and the 237 code stored it directly:

```rust
let seq = zpico_queryable_take_reply_seq(buffer_index as i32);  // i64
slot.seq.store(seq, Ordering::Relaxed);                          // AtomicI32 on 32-bit
```

On 64-bit (native dev host, CI native lane) this is `AtomicI64.store(i64)` — fine,
so it passed everywhere the workspace is normally checked. On 32-bit it's
`AtomicI32.store(i64)` → hard compile error. Surfaced building the esp32-baremetal
talker (`riscv32imc`) during a local `just test-all` shake-out.

**Fix.** Add a scalar alias `SeqScalar` mirroring `AtomicSeqCounter`'s width
(`i64` / `i32`) and narrow the FFI value at the store:

```rust
slot.seq.store(seq as SeqScalar, Ordering::Relaxed);
```

Reply-slot indices are small and fit i32; symmetric with the existing `.into()`
widening on the load side. **Validated**: `cargo check -p nros-rmw-zenoh`
(native, 64-bit) still clean AND the esp32 `riscv32imc` talker links
(`nros-fast-release`). The other `seq.store` site (test module) stores a
native-width `fetch_add` result, so it was unaffected.

**Follow-up risk.** No 32-bit `cargo check` gate ran on the 237 PR — the native
workspace check can't catch a `target_has_atomic`-split type error. The
`rust-rtos-link-check` gate (`just ci`) builds embedded zenoh examples and would
have caught this; worth ensuring it covers a service-using example.

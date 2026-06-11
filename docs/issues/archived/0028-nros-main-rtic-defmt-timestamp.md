---
id: 28
title: nros::main!() RTIC examples miss defmt::timestamp! → undefined _defmt_timestamp
status: resolved
type: bug
area: build
related: [phase-216, issue-0024]
resolved_in: 533230d85
---

**RESOLVED (`533230d85`).** Option 1 (shared provider): added one
`defmt::timestamp!("{=u64:us}", { 0 });` to `nros-board-rtic-stm32f4/src/lib.rs`
— the crate every RTIC example links. The plain `#[entry]` `talker` links a
*different* board crate (`nros-board-stm32f4`) and carries its own, so no
duplicate-symbol risk. Validated: `stm32f4-rtic-service-server` links clean.

The stm32f4 RTIC examples that use the `nros::main!()` shape (Phase 216.B.5)
fail to link because they pull in `defmt` (via `defmt_rtt as _`) but never
define a `defmt::timestamp!`, so the `_defmt_timestamp` symbol is undefined.

**Symptom** (full `build-test-fixtures`, stm32f4 leaf):

```
rust-lld: error: undefined symbol: _defmt_timestamp
  ...
error: could not compile `stm32f4-rtic-service-server` (bin "stm32f4-rtic-service-server")
```

Also hits `stm32f4-rtic-action-client` (and the other `*-rtic` service/action
examples).

**Cause.** `defmt` requires the final binary to define exactly one
`defmt::timestamp!(...)` provider. The plain `#[entry]` examples do — e.g.
`examples/stm32f4/rust/talker/src/main.rs:45`:

```rust
defmt::timestamp!("{=u64:us}", { 0 });
```

But the RTIC examples collapse their whole body to `nros::main!();`
(`examples/stm32f4/rust/service-server-rtic/src/main.rs:41`) + `use defmt_rtt as _;`,
and `nros::main!()` does not emit a `defmt::timestamp!`. So nothing provides
`_defmt_timestamp`.

**Fix options.**

1. **Per-example (mechanical, safe):** add
   `defmt::timestamp!("{=u64:us}", { 0 });` to each `*-rtic` example's
   `main.rs` (mirrors the talker). Lowest risk; touches ~4 files.
2. **Macro-level (DRY, but careful):** have `nros::main!()` emit a default
   `defmt::timestamp!` — but only when defmt is actually in use. Emitting it
   unconditionally would force a `defmt` dependency on every `nros::main!()`
   consumer (incl. non-defmt platforms), so it must be feature-gated (e.g. a
   `defmt` feature on the board/macro crate). Preferred long-term, needs the
   gate designed.

**Note.** Fixing this alone does **not** green the stm32f4 fixture leaf — the
plain `stm32f4-bsp-talker` separately overflows RAM via the Phase 231 size-class
buffers ([issue 0024](0024-esp32-dram-overflow-size-class-buffers.md)). Both
must land for stm32f4 to build.

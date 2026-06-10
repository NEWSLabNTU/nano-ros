---
id: 24
title: esp32 .bss overflows DRAM — Phase 231 size-class receive buffers too large
status: open  # fix applied, pending CI confirmation
type: bug
area: build
related: [phase-231, rfc-0038, phase-230]
---

The `esp32` cell of `platform-ci` fails to **link** the per-platform Entry
firmware: the static receive-buffer blocks introduced by Phase 231 (RFC-0038
size-class receive buffers) do not fit in esp32 DRAM.

**Symptom** (`platform-ci` → `Build (esp32)`, riscv32imc-unknown-none-elf):

```
rust-lld: error: section '.bss' will not fit in region 'DRAM': overflowed by 54000 bytes
rust-lld: error: section '.noinit' will not fit in region 'DRAM': overflowed by 54000 bytes
rust-lld: error: section .stack ... exceeds available address space
```

**Cause.** `nros-rmw-zenoh`'s `shim/subscriber.rs` holds the per-subscriber
size-class payload rings as `static mut` blocks sized from build-time
constants (generated into `OUT_DIR/buffer_config.rs` by
`nros-rmw-zenoh/build.rs` from `ZPICO_*` env vars):

- `LARGE_PAYLOAD_BLOCKS = MAX_LARGE_SUBSCRIBERS × SUBSCRIBER_RING_DEPTH × SUBSCRIBER_LARGE_SIZE`
  = `2 × 4 × 16384` = **128 KiB**
- `SMALL_PAYLOAD_BLOCKS = ZPICO_MAX_SUBSCRIBERS × SUBSCRIBER_RING_DEPTH × SUBSCRIBER_BUFFER_SIZE`

The Phase 231 default `SUBSCRIBER_LARGE_SIZE = 16384` (large size class) makes
the large block dominate `.bss`. esp32's DRAM budget (esp-alloc carves the
heap from the rest) cannot absorb the extra ~54 KiB. Not caused by the
phase-230 alloc funnel (FreeRTOS-gated; esp32 untouched) — surfaced once
platform-ci builds were unblocked (issue: nuttx-libc provisioning).

**Fix direction.** The buffer config is already per-build via `ZPICO_*` env
vars. Give the esp32 build a smaller config (shrink `ZPICO_SUBSCRIBER_LARGE_SIZE`
and/or `ZPICO_MAX_LARGE_SUBSCRIBERS` / `ZPICO_SUBSCRIBER_RING_DEPTH`) so the
static blocks fit DRAM. esp32 cannot meaningfully buffer 16 KiB messages
anyway. Longer term, RFC-0038 should document a per-RAM-budget size-class
profile (the same knobs already exist).

**Fix applied (2026-06).** Added `ZPICO_SUBSCRIBER_LARGE_SIZE = "4096"` to the
`workspace-rust-esp32` row `env` in `examples/fixtures.toml` (the esp32_entry
firmware — the only esp32 cell that links; the standalone `examples/esp32/rust/*`
are `staticlib`/`rlib`, no link). Cuts the large block from 128 KiB to
`2 × 4 × 4096` = 32 KiB (~96 KiB saved vs the 54 KiB overflow). The env reaches
the build via `workspace-fixtures-build.sh` (`export $envstr` before
`cargo build -p esp32_entry`). Not verifiable in the dev env (no esp toolchain);
the platform-ci esp32 cell is the confirmation. Archive once green.

**Also affects stm32f4 (found 2026-06-11, full `build-test-fixtures` run).** The
same Phase 231 size-class blocks overflow stm32f4 RAM, not just esp32 DRAM —
`examples/stm32f4/rust/talker` (`stm32f4-bsp-talker`) fails to link:

```
rust-lld: error: section '.bss' will not fit in region 'RAM': overflowed by 36904 bytes
rust-lld: error: section '.uninit' will not fit in region 'RAM': overflowed by 37928 bytes
```

The esp32 fix above does **not** cover stm32f4: the stm32f4 fixture rows
(`examples/fixtures.toml`, `platform = "stm32f4"`, plain `dir`/`target` cargo
builds) carry no `env`, so they still build the 128 KiB large block. **Fix
direction:** same knob — add `ZPICO_SUBSCRIBER_LARGE_SIZE` (≈4096) to the
stm32f4 fixture rows. ⚠️ Verify the manifest `env` field actually propagates to
the direct-cargo stm32f4 rows (confirmed only for the esp32 *workspace* path via
`workspace-fixtures-build.sh`; the non-workspace fixtures-build path may consume
`env` differently). Reinforces the "per-RAM-budget size-class profile" direction:
small-RAM targets (esp32, stm32f4, …) all need a shrunk large block, so a
profile keyed on the board's RAM budget is more robust than per-row env
overrides.

---
id: 64
title: esp32-c3 QEMU — Load-access-fault (mtval=0xffffffff) in zenoh-pico config init crashes session bring-up
status: open
type: bug
area: platform
related: [phase-248, phase-249]
---

## UPDATE 2026-06-15 — the Load-access-fault is FIXED; a separate heap OOM remains

The `0xffffffff` Load-access-fault is **resolved**: `Context::with_config` now
stages the locator into a fixed-address `.bss` static and the retry closure reads
its **link-constant address** each attempt, instead of a captured `&[u8]` whose
pointer field was overwritten by the backoff-poll wild write. Verified: the esp32
listener now connects + subscribes with **zero** Load-access-faults across full
192 s runs (was faulting every run).

**Newly exposed residual (was masked by the crash):** the esp32 firmware now hits a
heap OOM during session setup — `memory allocation of 8 bytes failed` (alloc error
handler), so the talker publishes 0. Prime suspect: the retry path leaks — each
`zpico_init_with_config` runs `z_config_default(&g_config)` **without freeing** the
previous attempt's `_z_str_clone` allocations, so a few failed-open retries exhaust
the esp32 heap. Candidate fixes: free/clear `g_config` before re-defaulting on
retry; or reduce retry churn; or bump the esp32 heap. Tracked here (issue stays
`open` until esp32 live e2e is green).

## Symptom

The networked `esp32_emulator` live tests are red:
`test_esp32_talker_listener_e2e`, `test_esp32_to_native`, `test_native_to_esp32`,
`test_esp32_workspace_entry_e2e` (4/8 of the file; the build/detection 4 pass).
Intermittent — a node sometimes connects, sometimes the firmware faults.

## Real cause (re-diagnosed 2026-06-15)

The prior `phase-89.4-followup` TODO ("OpenETH smoltcp never emits the final ACK /
handshake stalls / `Transport(ConnectionFailed)`") is **stale**. With a full QEMU
backtrace the listener reaches `Waiting for messages...` — `Executor::open` +
subscribe succeed, so TCP + the zenoh session open work. The real failure is a
firmware CPU exception during session **init**:

```
Exception 'Load access fault' mepc=0x4203e302, mtval=0xffffffff
  libc_stubs::strlen                         (esp32-qemu platform, the faulting load)
  <- zenoh-pico _z_str_size / _z_str_clone    (collections/string.c:165 / :189)
  <- _zp_config_insert                        (protocol/config.c:36)
  <- zpico_init_with_config                   (zpico-sys/c/zpico/zpico.c:833)
  <- nros_rmw_zenoh::zpico::Context::with_config (nros-rmw-zenoh/src/zpico.rs:347)
```

`mtval=0xffffffff` is a deref of an all-ones **pointer value** (not a walk-off-end:
a valid esp32 DRAM/flash string ptr that walked off would fault near the segment
end ~0x3fce0000, not 0xffffffff). So a config-string **value** handed to
zenoh-pico's config intmap is the literal pointer `0xffffffff`.

esp32-c3 (QEMU OpenETH) **only** — the identical `with_config` path is runtime-green
on freertos / threadx / native. So it is memory corruption local to the bare-metal
esp32 session-init path, NOT networking.

## Ruled out (static analysis)

- **Stale global `g_config`** — `zpico_init_with_config` runs `z_config_default(&g_config)`
  every call (zpico.c:770).
- **Non-NUL-terminated locator/property values** — all are NUL-terminated stack
  buffers in `SmoltcpSession::new` (`shim/session.rs`); `c_props` is zeroed and only
  `&c_props[..prop_count]` (valid entries) is passed; the property loop guards NULL.
- **Dangling stack pointers from retry** — `connect_with_retry` (zpico.rs:275) loops
  **synchronously** with backoff inside `with_config`'s scope; the captured buffers
  stay live.
- **Too-small main stack** — ~18 KB (`_stack_start` 0x3fcce400 − `_stack_end`
  0x3fcc9a4c); the ~4.2 KB `SmoltcpSession::new` frame (key_bufs/val_bufs 2×256×8) is
  large but the fault signature is a bad pointer *value*, not a stack-guard hit.

## ROOT CAUSE PINNED (2026-06-15, esp_rom_printf instrumentation)

Instrumented `zpico_init_with_config` with `esp_rom_printf` (the ROM UART printer —
no libc, gated `#if defined(__riscv) && __riscv_xlen == 32`), logging each insert's
value pointer. The trace at the fault:

```
ZDBG enter mode=0x3c001cdb locator=0x3fcbd00c nprops=0 props=0x0   <- attempt 1: OK, init succeeds
ZDBG mode=0x3c001cdb  zid=0x3fcbcf38  locator=0x3fcbd00c          <- all inserts valid
ZDBG enter mode=0x3c001cdb locator=0xffffffff nprops=0 props=0xffffffff  <- attempt 2 (retry)
ZDBG locator=0xffffffff
Exception 'Load access fault' mtval=0xffffffff                    <- _z_str_clone(0xffffffff)
```

So:

1. **It is the retry path.** `Context::with_config` wraps `zpico_init_with_config` +
   `zpico_open` in `connect_with_retry` (zpico.rs). Attempt 1's `init` succeeds with a
   valid locator; `zpico_open` **fails** (retryable — transient connect) → 300 ms
   `connect_backoff_ms` → attempt 2 re-invokes `init` — now with `locator=0xffffffff`.
2. **The corrupted thing is the closure's captured pointer, not the buffer.** On
   attempt 2 the *argument* `locator` is `0xffffffff`. `mode` (a `'static` **flash**
   pointer `0x3c001cdb`) survives; `props` (recomputable from `is_empty()`) survives;
   only the **locator's captured `&[u8]` fat-pointer field** is overwritten — a
   targeted ~4-byte wild write to one slot in `connect_with_retry`'s frame.
3. **Single-threaded** — the esp32-c3 board uses the poll/`spin_once` cooperative
   model (`smoltcp_network_poll`, no zenoh-pico read/lease tasks; the earlier
   "race with read/lease tasks" lead is ruled out). The clobber window is
   `connect_backoff_ms` → `z_sleep_ms`, which drives the **network poll** (OpenETH
   RX/TX + the `before_poll` RXEN toggle) during the backoff. A wild write on that
   poll/DMA path lands on the locator's captured pointer slot.
4. The locator buffer lives at `0x3fcbd00c` — **below** `_stack_end` (0x3fcc9a4c),
   i.e. lower DRAM, not the main stack.

**Tried + INSUFFICIENT:** recomputing `locator_ptr`/`props_ptr` *inside* the retry
closure (instead of capturing precomputed thin pointers). It fixed `props`
(0xffffffff→null) but NOT `locator` — the captured `locator: Option<&[u8]>` ref is
*itself* corrupted, so re-deriving `loc.as_ptr()` still yields 0xffffffff.

## Next step

The fix must make the locator survive the backoff clobber. Options:
- **Memory watchpoint** on the locator's captured-pointer slot to catch the exact
  wild write (the definitive next move) — needs gdb. **The QEMU gdbstub is currently
  unusable in this CI sandbox:** every `qemu-system-riscv32 … -S -gdb tcp::1234`
  (and `-gdb unix:`) is `SIGTERM`'d within ~1 s by the harness (plain QEMU runs
  fine). Run the watchpoint on a workstation without that restriction.
- **Static-stage** the locator (+ properties) into fixed-address storage the retry
  closure reads, instead of a captured stack/DRAM `&[u8]` (immune to the capture
  clobber). Risk: `static mut` thread-safety + it is a shared (all-platform) shim.
- **Find + fix the wild write** on the `z_sleep_ms`/poll/OpenETH path (the real bug —
  something writes `0xffffffff` to a DRAM slot during the backoff poll).

## Fixed alongside (this issue's commit `651f7f579`)

- Replaced the stale OpenETH TODO in `esp32_emulator.rs` with the above diagnosis.
- `just esp32 build-fixtures` now stages the `esp32_entry` workspace fixture (was
  only built by `build-examples`, though `test_esp32_workspace_entry_e2e`'s skip
  message points at `build-fixtures`).

---
id: 64
title: esp32-c3 QEMU — Load-access-fault (mtval=0xffffffff) in zenoh-pico config init crashes session bring-up
status: resolved
type: bug
area: platform
related: [phase-248, phase-249]
---

## RESOLVED 2026-06-15 — heap 48 KB → 16 KB; talker+listener e2e GREEN

Final fix in the stack-budget hardening pass: **shrink the esp-alloc Rust heap 48 KB
→ 16 KB** (`nros-board-esp32-qemu/src/node.rs`). The measured Rust-heap live peak is
≈20 B (zenoh-pico runs its own 32 KB `z_malloc` FreeListHeap), so 16 KB stays >100× the
peak while freeing 32 KB of DRAM to the stack. Since `link.x`'s `.stack` fills DRAM from
end-of-`.bss` to the top, that grows the stack ≈66 KB → ≈98 KB — finally deep enough for
the nros + zenoh-pico connect/spin/dispatch/poll chain. The 48 KB instruction-fault on the
first timer fire (corrupted closure fn-ptr near `_stack_end`) was the last stack-depth
overflow; 16 KB clears it.

Verified:
- **Talker alone vs zenohd:** boots, connects, publishes `/chatter` steadily
  (`Published: 0 … 994+`), 0 faults / 0 OOM / 0 panic, logger routes every line.
- **Two-node e2e GREEN:** `cargo nextest run -p nros-tests --test esp32_emulator
  test_esp32_talker_listener_e2e` → `PASS [109.081s]` (talker publishes, listener
  receives, `received_count ≥ 1`).

Full fix chain that landed this: OpenEth `new_in_place` (no 11 KB by-value stack temp) +
locator `.bss`-static staging + `CONFIG_PROPERTY_SIZE` 256→64 (no_std) + esp-println
`log::Log` logger + the heap 96→48→16 KB rebalance. Root class across all symptoms
(Load-access-fault / OOM-wipe / instruction-fault) was a single one: the ~18 KB stack,
starved by an oversized `.bss` heap array, overflowing into `.bss` along the deep connect
path. The executor timer-dispatch was healthy throughout.

## UPDATE 2026-06-15 — the Load-access-fault is FIXED; a separate heap OOM remains

The `0xffffffff` Load-access-fault is **resolved**: `Context::with_config` now
stages the locator into a fixed-address `.bss` static and the retry closure reads
its **link-constant address** each attempt, instead of a captured `&[u8]` whose
pointer field was overwritten by the backoff-poll wild write. Verified: the esp32
listener now connects + subscribes with **zero** Load-access-faults across full
192 s runs (was faulting every run).

**Newly exposed residual (was masked by the crash):** the esp32 firmware now hits a
heap OOM during the talker's session setup — `memory allocation of N bytes failed`
fired by the **Rust** global allocator (`alloc.rs`, the esp-alloc 96 KB heap;
`node.rs` `heap_allocator!(size: 96 * 1024)` on the non-`dds-heap` path). The
**listener connects + subscribes fine** (no OOM); only the talker OOMs — so it is
talker-setup or retry-churn dependent, not a flat sizing miss.

`z_open` consumes `g_config` via `z_config_move` each attempt, so g_config is NOT
the leak; the exhausted heap is the **Rust** 96 KB one, not zenoh-pico's 32 KB
`z_malloc` FreeListHeap.

**Can't just grow the heap:** bumping 96 KB → 128 KB fails to link —
`.bss will not fit in region 'DRAM': overflowed by ~14 KB`. esp32-c3 DRAM is already
near-full at 96 KB. So the OOM must be fixed by **reducing** allocation (find the
talker-path / per-retry allocator churn and trim it, or shrink another DRAM
consumer to free heap room), not by enlarging the heap.

Issue stays `open`: the Load-access-fault (the original title) is fixed; this OOM is
the remaining blocker to a green esp32 live e2e.

## UPDATE 2026-06-15 (later) — OOM root-caused + FIXED; remaining is timer-dispatch

The OOM was NOT a sizing/leak issue. `esp_alloc::HEAP.stats()` instrumentation showed
the 96 KB region is added (`Size: 98304`) right after `heap_allocator!`, then wiped to
`Size: 0` **across the `OpenEth::new(...)` call** in `init_ethernet`. ROOT CAUSE:
`OpenEth::new()` returns an ~11 KB struct by value (tx_buf + rx_bufs[4] + rx_frame);
the caller materialises it as an 11 KB **stack temporary** on the ~18 KB esp32-c3
stack, which **overflows into `.bss`** and silently corrupts whatever lives there —
here the esp-alloc heap metadata, and (during connect) the zenoh locator pointer. So
the 0xffffffff Load-access-fault AND the heap OOM are the SAME bug: an 11 KB stack
overflow.

**FIXED (`OpenEth::new_in_place`)**: construct the driver directly in its static
storage (write_bytes zero-fill + two scalar fields), no by-value temporary. Verified:
the esp32 talker now boots fully — 0 faults, 0 OOM — and reaches `Application setup
complete — entering spin loop` (previously crashed/OOM'd before that). (The earlier
locator `.bss`-static workaround is now redundant given this root-cause fix; can be
reverted in a follow-up to keep the shared shim clean.)

**Remaining blocker — the Rust 1 Hz timer never fires.** With the firmware healthy,
the talker spins but `on_tick` never runs (0 `Published:` even after adding the log
line; the node pkgs were also silent — added `log::info!("Published:/Received:")` to
the talker/listener so the e2e has observable lines). Timers fire via
`executor.spin_once`, which needs `clock_us` (esp-hal `Instant::now()` / esp32-c3
SystemTimer) to advance. **Prime suspect: the esp32c3 QEMU fork's SystemTimer does
not advance** — RULED OUT. A `clock_us` log in the spin loop showed the clock advances
normally (`clock_us` 2.0s → 4.0s → 6.0s across spins 200/400/600). So the failure is in
the **Rust bare-metal executor timer-dispatch**, not the clock:

1. The 1 Hz timer **never fires** — 0 `on_tick` dispatches in the first 6 s (≈6 expected),
   so `executor.spin_once` is not dispatching the registered 1000 ms timer despite the
   clock advancing. (The timer is declared via `create_timer_for_callback_name` in the
   talker `register()`.)
2. `spin_once` then **hangs at ~6 s** (the CDBG trace stops at spin 600; each `spin_once(10)`
   takes ~10 ms, so the loop blocked indefinitely after ~600 iterations).

These are deep executor-runtime gaps on the esp32 bare-metal Rust path, which was never
runtime-verified (the green freertos/threadx cells checked boot/setup, or used C
examples with explicit publish loops — not the Rust timer-driven dispatch).

## FINAL ROOT CAUSE 2026-06-15 — NOT an executor bug; the talker OOM-panics in register

The "timer never fires / spin_once hangs" conclusion above is **superseded**. Traced with
atomic debug counters (esp32 `log::info` does NOT route — the framework "Waiting for
messages" never prints either — so log probes were useless; counters are the only reliable
signal). The two-node e2e only surfaces the **listener** QEMU output, which registers one
subscription + spins fine — **the executor timer-dispatch path is healthy**. Running the
**talker alone** in QEMU against a zenohd shows it connects, then **panics with a heap OOM
during `register()`** — decoded backtrace:

```
ExecutorNodeRuntime::register_dispatch_slot (node_runtime.rs:400)
  -> Talker::register (talker/src/lib.rs:29 — the create_timer line)
  -> DeclaredNode::declare_entity (node.rs:893)
  -> ExecutorSink::create_entity (node_runtime.rs)
  -> alloc::raw_vec::handle_error  (a Vec grow)
  panicked at alloc.rs:573 — "memory allocation of N bytes failed"
```

The talker registers the **publisher** OK, then the **timer's** `create_entity` Vec-grow
exhausts the Rust 96 KB esp-alloc heap — already mostly consumed by `Executor::open`'s
zenoh-session setup. The talker never reaches the spin loop, so its 1 Hz timer never fires.
(The listener's single subscription just fits + spins, which is why the e2e's listener-side
output looked healthy and the earlier "setup complete" was the listener.)

Same Rust-heap-vs-full-DRAM budget wall: the OpenEth in-place fix stopped the heap *wipe*
(Size 98304 holds), but 96 KB is genuinely too small for the talker's session + publisher +
timer registration, and DRAM is full so the heap can't grow (bumping → `.bss overflows
DRAM`). **Fix = reduce Rust-heap allocation on the esp32 path** (the session/keyexpr/RX
buffers consumed during `Executor::open`, and/or free a DRAM consumer — e.g. the 32 KB
zpico FreeListHeap or OpenEth's 4-slot rx ring — to grow the esp-alloc heap). A heap-budget
tuning effort on a DRAM-constrained target; the crash/OOM-wipe class is fixed, this is the
residual sizing wall.

## UPDATE 2026-06-15 (heap-budget tuning) — it's NOT a heap budget; it's stack budget

`esp_alloc::HEAP.used()` measured **20 bytes** after `Executor::open` — the Rust heap is
barely touched (zenoh uses its own 32 KB `z_malloc` heap). The OOM was a heap *wipe*, not a
budget miss: `HMEM` showed `used=0 free=0` after `Executor::open`, i.e. the EspHeap metadata
gets clobbered **again** during `Executor::open` (after the OpenEth fix stopped the init-time
wipe). The clobber is `SmoltcpSession::new`'s ~4 KB `key_bufs`/`val_bufs` stack frame +
the deep zenoh-pico C connect chain overflowing the ~18 KB stack into `.bss`.

Key insight from the linker `stack.x`: **the stack is whatever DRAM is left after `.bss`**
(`.stack` fills RWDATA from end-of-bss to top). So the **96 KB esp-alloc heap array starves
the stack to ~18 KB.** Rebalancing — **heap 96 KB → 48 KB frees 48 KB → stack grows to
~66 KB** — let the talker **register fully and reach the spin loop** (verified, no OOM).

Landed three real fixes from this pass:
1. **heap 96 KB → 48 KB** (`nros-board-esp32-qemu/src/node.rs`) — frees DRAM so the stack
   grows; the Rust heap stays >2× the measured peak.
2. **`CONFIG_PROPERTY_SIZE` 256 → 64 for no_std** (`nros-rmw-zenoh/src/shim/mod.rs`) — the
   256 is for std TLS cert paths; no_std esp32 carries only small values. Cuts the
   `SmoltcpSession::new` stack frame ~3 KB.
3. **esp-println `log::Log` logger** (`init_logger` in `register_log_writer`, + the
   `log-04` esp-println feature) — esp32 `log::info!` was **completely unrouted** (no
   `log::Log` installed → every record silently dropped; verified `log::error!` now prints).
   Required for the examples' `Published:`/`Received:` lines.

**Still red — a deeper pervasive stack corruption.** With the above, the talker registers +
the logger routes, but the timer's first fire (~1 s into the spin loop) jumps to a stack
address — `Exception 'Instruction access fault' mepc=0x3fcbda50` — i.e. the timer
**dispatch works** (reaches `(entry.callback)()` in `timer_try_process`) but the callback
closure's storage is corrupted. The esp32-c3 stack is still too shallow for the deep
nros + zenoh-pico spin/dispatch/poll paths even at ~66 KB; the exact symptom (no-fire / OOM
/ instruction-fault) shifts with the build layout — classic stack overflow. **Next: a
systematic stack-budget hardening pass** — trim the big frames along the connect/spin/poll
chain (and/or grow the stack further by shrinking the 32 KB zpico heap + the 48 KB esp-alloc
heap once their real peaks are measured). The executor timer-dispatch itself is healthy.

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

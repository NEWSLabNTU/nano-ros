---
id: 1759
title: "threadx-linux images still link libstd for the process entry, panic handler and allocator, though every crate below the bin is `no_std`"
status: open
type: tech-debt
area: [boards, threadx, core]
severity: low
found: 2026-10-09
related: [issue-0644, issue-1742, rfc-0077, phase-359]
---

## What was measured

On `threadx-linux` the only crate that brings in `std` is the example's `src/main.rs`, the Linux process entry. Everything below it is already `no_std`:

- `nros` is taken with `default-features = false, features = ["alloc", …]`. Issue 0644 did this.
- `nros-board-threadx-linux` is `#![no_std]`.
- Each example's node library (`src/lib.rs`) is `#![no_std]`.
- Configuration is compile-time. `config.rs` reads `option_env!`, and nothing reads `std::env` at runtime.

`std` is pulled in because `src/main.rs` is an ordinary bin and `nros::main!` picks its entry form by `target_os`. This target is `x86_64-unknown-linux-gnu`, so the macro emits the hosted `fn main()`. The board descriptor says `entry_kind = "hosted-main"`.

libstd then supplies three things the image needs:

| need | supplied by | consequence |
| --- | --- | --- |
| process entry (crt0 → `main`) | `lang_start` | the bin cannot be `#![no_main]` |
| `#[panic_handler]` | libstd | issue 1742: `PANIC platform` only "works" because std's handler wins and never reaches `nros_platform_panic`. `PANIC halt` hits E0152, so the carrier refuses it on this board. |
| `#[global_allocator]` | libstd `System` (glibc `malloc`) | the sim never uses the ThreadX byte pool that real ThreadX hardware allocates from, so its memory figures are not ThreadX's |

## Precedent

The NuttX FFI bins are already `#![no_std]` + `#![no_main]` (phase-359 W7). The macro emits `#[unsafe(no_mangle)] extern "C" fn main(argc, argv)` for them, and the C runtime calls it. The rv-virt-threadx sibling declares its own panic handler and allocator. threadx-linux is the remaining ThreadX image that still links libstd.

## Direction

1. **Entry.** Choose the macro's C-runtime entry arm from the BOARD's facts rather than from `target_os = "nuttx"` alone. Move threadx-linux to `entry_kind = "board-run"`, so the entry builder emits `#![no_std]` + `#![no_main]`.
2. **Panic.** Use `nros::main!(panic = …)` with the default `platform`, which routes to `nros_platform_panic`. Then `halt` links too, and issue 1742's refusal of `halt` on threadx-linux can be lifted.
3. **Allocator.** The board supplies a `#[global_allocator]` over `nros_platform_alloc` (the ThreadX byte pool), as on rv64. The issue-0644 comment called this rerouting "for no reason". The reason now: the sim exercises the same allocator as the hardware, and the byte-pool peak line and `mem-report` describe ThreadX.
4. **C side unchanged.** The ThreadX Linux port's C side (pthreads, signals, glibc) is unchanged; only the Rust side drops libstd. Prebuilt `core`/`alloc` exist for the host triple, so no `build-std` is needed.

**Acceptance:**
- every `examples/threadx-linux/rust/*` builds with no `std::` symbols in the ELF;
- talker/listener and action server/client deliver against a router;
- `PANIC halt` and `platform` both link, and `platform` reaches `nros_platform_panic`;
- the boot prints the byte-pool peak.

**Follow-up:** `freertos-posix` is in the same position (`entry_kind = "hosted-main"` with a C runtime). Apply the same pattern once threadx-linux has landed.

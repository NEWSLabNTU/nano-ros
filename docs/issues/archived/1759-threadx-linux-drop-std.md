---
id: 1759
title: "threadx-linux images still link libstd for the process entry, panic handler and allocator, though every crate below the bin is `no_std`"
status: resolved
type: tech-debt
area: [boards, threadx, core]
severity: low
found: 2026-10-09
related: [issue-0644, issue-1742, issue-1763, rfc-0077, phase-359]
resolved_in: "threadx-linux Rust images are no_std: board-run entry, platform panic, byte-pool allocator (issue 1759)"
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

## Resolution

Every `examples/threadx-linux/rust/*` image and both generated workspace entries
(`workspaces/rust`, `workspaces/realtime-rust`, image `threadx`) are now
`#![no_std]` + `#![no_main]` and link no libstd.

- **Entry: decided by a board fact, not by `target_os`.** The descriptor now says
  `entry_kind = "board-run"`, and `BOARD_PATHS` says `links_std = false`. The
  existing gate `the_links_std_column_agrees_with_the_descriptors_entry_kind`
  keeps those two in step. `nros::main!`'s C-ABI `main(argc, argv)` arm used to
  be `#[cfg(target_os = "nuttx")]` for every board. It is now
  `c_runtime_main_cfg(entry_links_std)`: `not(target_os = "none")` for a
  `#![no_std]` entry, and NuttX-only for a hosted one, which is what an unknown
  out-of-tree board gets. The CLI's Rust entry template (`entry.rs.jinja`) has
  the same rule, and the `mps2-an385-freertos` parity golden moves one line. The
  builder writes `#![no_std]` + `#![no_main]` for the generated entries, because
  the descriptor's kind changed.
- **Panic.** `nros::main!`'s panic gate (the list of hosted OSes) applies only
  when the entry links `std`. A `#![no_std]` entry gets
  `panic_to_platform!()` / `panic_halt!()` with no cfg. The leaves say
  `nros::main!(panic = "platform")`.
- **Allocator.** There is a new board feature, `nros-board-threadx-linux/image-runtime`,
  which turns on `nros-platform/global-allocator` (`nros_platform_alloc` over
  the ThreadX byte pool). It is NOT a default. Two test bins
  (`logging-smoke-`, `pool-exhaustion-threadx-linux`) are `std` programs that
  boot the board from a hosted `fn main`, and libstd allocates before
  `tx_kernel_enter` creates the pool. The descriptor's `board_features` names it
  for generated entries, and the six leaves name it themselves.
- **What libstd had supplied, now stated by the board.** Two things were
  measured missing at link and nowhere else:
  - `-lc`. rustc links the gnu target `-nodefaultlibs`, and `libc`-via-libstd
    had been the only `-lc`, so the link failed on `__libc_start_main`,
    `memcpy` and `socket`. Fixed by `cargo:rustc-link-lib=c` in the board's
    `build.rs`.
  - `rust_eh_personality`. The host sysroot's prebuilt `liballoc` is built for
    unwinding. The board defines an empty one under `image-runtime`. It is never
    called, because nothing unwinds.

  No `build-std` was needed.
- **C side unchanged.** The ThreadX Linux port is unchanged. The C, C++ and mixed
  images keep a `std` runtime staticlib, so 1742's `PANIC halt` refusal still
  applies to them, and correctly so. Split to issue 1763.
- **`check-board-build-target`** assumed "`board-run` ⇒ cross triple". It now
  exempts `board-run` on a host-simulator platform (`threadx-linux`) that
  configures no `[target.*]`. Self-test rows cover the exemption, the same row
  on a cross platform, and a host-simulator row that does configure a triple.

### Evidence (2026-10-09)

**No libstd symbol in any ELF.** `nm -C` over the six role binaries, in both
fixture groups (`build/cargo-fixtures/threadx-linux{,-3263301353}`), and over
the two workspace entries: 0 symbols match `std::|lang_start`, out of about 2,530
per image. Each image has `T main`, the `__nros_panic` handler,
`nros_platform_panic` and `_tx_byte_allocate`. The same scan over the `std` test
bin `logging-smoke-threadx-linux` finds 177, so the scan is not blind.
`__rust_alloc` in the talker jumps through a GOT slot that relocates to
`nros_platform_alloc`.

**`platform` reaches `nros_platform_panic`.** `rust_begin_unwind` in the talker
formats the message and then calls through a slot that relocates to
`nros_platform_panic` (0x36ca0). That symbol is the weak ThreadX port
definition: log writer, then `exit(1)` on Linux.

**`halt` links.** A copy of the talker leaf with `nros::main!(panic = "halt")`
builds with `nros build` (rc=0) and has 0 std symbols. Its
`rust_begin_unwind` calls `nros_platform_critical_section_acquire` and spins.

**Delivery against `rmw_zenohd`.** `rtos_e2e` with `test(ThreadxLinux)` ran 9 of
9 passed:

| cell | Rust | C | C++ |
| --- | --- | --- | --- |
| pubsub | 70/70 | 70/70 | 70/70 |
| service | 1 response | 1 response | 1 response |
| action | accepted + completed | accepted + completed | accepted + completed |

The generated `workspaces/rust` threadx entry was booted by hand against a
router on 9030: talker seq 0..18, `Application setup complete`. The
`realtime-rust` threadx entry was booted the same way on 9091: tiers `high` and
`low` both dispatching.

**Byte-pool peak at boot.** The pool now carries the Rust heap too:

| image | peak | pool | before (Rust on glibc, issue 1145) |
| --- | --- | --- | --- |
| rust talker (`rtos_e2e`) | 184,552 B | 4,105,736 B | 181,144 B |
| rust listener (`rtos_e2e`) | 184,272 B | 4,105,736 B | — |
| `workspaces/rust` threadx entry | 189,528 B | 4,194,304 B | — |

The talker line reads `nros: byte pool peak 184552 of 4105736 bytes (3921184 free)`.

**Mutation proofs.** For each mutation, "applied" was checked by diff:

| mutation | check | rc |
| --- | --- | --- |
| `BOARD_PATHS` threadx-linux back to `true` (descriptor `board-run`) | `board_key_table::the_links_std_column_agrees_with_the_descriptors_entry_kind` | 101, names `threadx-linux` |
| `c_runtime_main_cfg` back to `target_os = "nuttx"` for all | `nros-macros a_no_std_board_gets_the_c_runtime_main_on_every_os_target` | 101 |
| template back to unconditional `#[cfg(target_os = "nuttx")]` | `emit_rust::only_a_board_whose_entry_links_std_gets_the_hosted_main` | 101 |
| gate exemption removed | `check-board-build-target` (self-test) | 1 |
| clean tree | all four | 0 |

**Follow-up:** `freertos-posix` (still `hosted-main`) needs the same pattern.
The threadx-linux C/C++ staticlib is issue 1763.

# nros-platform-esp32s3-qemu

Bare-metal platform for **ESP32-S3** (Xtensa LX7 dual-core) running
under QEMU. Pairs with the
[`nros-board-esp32s3-qemu`](../../boards/nros-board-esp32s3-qemu)
board crate (Phase 117.2 — landing alongside). Used for the
Phase 117 DDS-on-ESP32 bring-up: ESP32-C3 (riscv32imc, 400 KiB SRAM,
no PSRAM) can't fit dust-dds's `DcpsDomainParticipant` builtin
entities; ESP32-S3 (512 KiB SRAM + 8–16 MiB octal PSRAM) gives the
heap headroom RTPS needs without trimming protocol surface.

## Role

Implements the trait family in
[`nros-platform-api`](../../core/nros-platform-api) for ESP32-S3 +
Espressif QEMU's `esp32s3` machine: `esp_hal::time::Instant` for the
monotonic clock, [`zpico-alloc::FreeListHeap`](../../zpico/zpico-alloc)
for the heap (1 MiB PSRAM region under `dds-heap`), single-threaded,
networking through the board crate via
[`openeth-smoltcp`](../../drivers/openeth-smoltcp) +
[`nros-smoltcp`](../../drivers/nros-smoltcp).

## Source layout

| File | Role |
|------|------|
| `src/lib.rs` | `Esp32s3QemuPlatform` zero-sized type + trait impls. Critical section uses Xtensa `rsil` / `wsr.ps` (not RISC-V `mstatus`). |
| `src/clock.rs` | `esp_hal::time::Instant` → ms/us. |
| `src/memory.rs` | `FreeListHeap`. `dds-heap` flips 32 KiB internal-SRAM heap → 1 MiB PSRAM heap (`.ext_ram.bss` section). |
| `src/random.rs` | `nros-baremetal-common` xorshift32 PRNG (ESP32-S3 hardware RNG via esp-hal). |
| `src/net.rs` | `nros_smoltcp::define_smoltcp_platform!(Esp32s3QemuPlatform)`. |

## When to use

- Phase 117 DDS-on-ESP32 bring-up: full RTPS protocol surface
  against ROS 2 peers without trimming builtin entities.
- A reproducible Xtensa target for verifying the bare-metal smoltcp
  multicast path under QEMU (mirrors the ESP32-C3 RISC-V coverage).

## Toolchain

Xtensa rustc support is out-of-tree (`esp-rs/rust` fork). Stable
rustc cannot build this crate. Install via
[`espup`](https://github.com/esp-rs/espup):

```bash
cargo install espup
espup install --targets esp32s3
. $HOME/export-esp.sh    # adds +esp toolchain + sets LIBCLANG_PATH
```

Then build with `cargo +esp build --target xtensa-esp32s3-none-elf …`.
See `book/src/reference/build-commands.md` (Phase 117.0 section) for
the full setup flow.

## Caveats

- **PSRAM is ~10× slower than internal SRAM.** Heap allocations land
  in PSRAM under `dds-heap`; `Arc` / `Weak` refcount atomics
  consequently live there too. If discovery latency drags, pin
  discovery actors to internal SRAM via custom `esp-alloc` region
  selectors at the board level — this crate's heap is intentionally
  generic.
- **Xtensa toolchain caveat.** Out-of-tree rustc fork; CI must run
  `espup install` before any crate build that touches this platform.
- **`rsil` critical section masks every interrupt below INTLEVEL 15.**
  Use only inside trait impls where the original RISC-V analogue
  masks `MIE` — long-running critical sections will block the
  esp-hal scheduler.

## See also

- [Custom Platform porting guide](../../../book/src/porting/custom-platform.md)
- Sibling C3 platform crate: [`nros-platform-esp32-qemu`](../nros-platform-esp32-qemu)
- Source on GitHub: <https://github.com/NEWSLabNTU/nano-ros/tree/main/packages/platforms/nros-platform-esp32s3-qemu>

# Supported Boards

Procurement-grade compatibility matrix. Each row lists a real
vendor + board model and reports nano-ros's status on it. Rows
marked **Tested** boot in CI; **Ready** rows compile and run but
have no in-CI gate yet; **Untested** rows compile per the
architecture support but no one has reported booting nano-ros on
them.

| Vendor       | Board                | MCU / SoC          | Arch       | Default RTOS  | Status   | Example / board crate                                            |
|--------------|----------------------|--------------------|------------|---------------|----------|-------------------------------------------------------------------|
| ARM          | MPS2-AN385 (QEMU)    | Cortex-M3          | Armv7-M    | FreeRTOS / bare | Tested  | `examples/qemu-arm-freertos/`, `examples/qemu-arm-baremetal/`     |
| STMicro      | STM32F4-Discovery    | STM32F407          | Cortex-M4F | FreeRTOS / bare | Tested  | `packages/boards/nros-board-stm32f4/`                              |
| STMicro      | STM32H7-Nucleo       | STM32H743          | Cortex-M7F | FreeRTOS / Zephyr | Ready  | Use FreeRTOS / Zephyr starter with `nros-board-freertos` overlay   |
| STMicro      | Pixhawk 4 (FMUv5)    | STM32F765          | Cortex-M7F | NuttX (PX4)   | Ready    | `integrations/px4/module-template/`                                |
| STMicro      | Pixhawk 6X / 6C      | STM32H753          | Cortex-M7F | NuttX (PX4)   | Ready    | `integrations/px4/module-template/`                                |
| Nordic       | nRF52840-DK          | Cortex-M4F         | Armv7E-M   | Zephyr        | Untested | Zephyr starter — supply `-b nrf52840dk_nrf52840`                  |
| Nordic       | nRF5340-DK           | Cortex-M33 (dual)  | Armv8-M    | Zephyr        | Untested | Zephyr starter — supply `-b nrf5340dk_nrf5340_cpuapp`             |
| Espressif    | ESP32-C3-DevKit      | RISC-V (RV32IMC)   | RISC-V     | bare / ESP-IDF | Tested  | `examples/qemu-esp32-baremetal/rust/`, `integrations/nano-ros/` |
| Espressif    | ESP32-C6-DevKit      | RISC-V             | RISC-V     | ESP-IDF        | Untested | Same ESP-IDF path as C3                                            |
| NXP          | LPC55S69-EVK         | Cortex-M33         | Armv8-M    | Zephyr        | Untested | Zephyr `-b lpcxpresso55s69_cpu0`                                  |
| NXP          | MIMXRT1170-EVK       | Cortex-M7 + M4     | Armv7-M    | FreeRTOS / Zephyr | Untested | FreeRTOS starter + vendor BSP                                  |
| TI           | LP-CC1352P7          | Cortex-M4F         | Armv7E-M   | FreeRTOS / TI-RTOS | Untested | FreeRTOS starter + TI driver overlay                         |
| RP2040       | Raspberry Pi Pico    | Cortex-M0+         | Armv6-M    | bare / FreeRTOS | Untested | Bare-metal Cortex-M3 path — Cortex-M0+ has only 4 NVIC priority levels (per-callback OS-priority dispatch is disqualified — pub/sub still works fine) |
| QEMU         | `virt` RISC-V64      | rv64gc             | RISC-V     | ThreadX       | Tested   | `examples/threadx-riscv64/`                                       |
| QEMU         | Cortex-A9 (Versatile)| Cortex-A9          | Armv7-A    | Zephyr / NuttX | Tested   | Zephyr `-b qemu_cortex_a9`, NuttX `qemu-armv7a`                    |
| Arm FVP      | `Base_RevC AEMv8R` (SMP) | Cortex-A SMP   | Armv8-R    | Zephyr 3.7    | Tested (build); license-gated runtime | See [ARM FVP getting-started chapter](../getting-started/arm-fvp.md); `just zephyr build-fvp-aemv8r{,-cyclonedds}` + `run-fvp-aemv8r{,-cyclonedds}` |
| Linux host   | (sim)                | x86-64 / aarch64    | x86 / Arm  | ThreadX sim   | Tested   | `examples/threadx-linux/`                                          |
| Linux host   | (native)             | x86-64 / aarch64    | x86 / Arm  | POSIX         | Tested   | `examples/native/`                                                  |

## How to add a new board

1. **Pick the matching RTOS path.** Cortex-M3 / M4 / M7 + RTOS → use
   FreeRTOS or Zephyr starter. Cortex-M0+ → bare-metal starter
   (limited; no NVIC priority headroom). Cortex-A / RISC-V64 → NuttX
   or Zephyr. Xtensa / RISC-V32 + Wi-Fi → ESP-IDF or esp-hal.
2. **Find or write a board crate.** Existing crates under
   `packages/boards/nros-board-*/` cover most QEMU + reference dev
   kits. Real-hardware boards need a thin board crate that supplies
   startup, linker script, and `BoardIdle::wfi()` (bare-metal) or
   wraps the RTOS's BSP (FreeRTOS / Zephyr).
3. **Run the existing example tree.** Each row above points at the
   canonical example dir. Cross-compile the talker / listener and
   verify against stock ROS 2 + `RMW_IMPLEMENTATION=rmw_zenoh_cpp`.
4. **Report back.** Open an issue with the working build + flash +
   run commands so the row moves from *Untested* / *Ready* to
   *Tested*.

## Caveats by chip family

- **Cortex-M0+** (RP2040, STM32F0, nRF51): only 4 NVIC priority
  levels. Per-callback OS-priority dispatch (a research scheduler
  shape originally proposed by Choi et al. as **PiCAS**, RTAS '21)
  is disqualified on this class; nano-ros's user-space EDF / FIFO
  scheduler is the only option. Pub/sub works fine.
- **Xtensa ESP32 / ESP32-S2 / ESP32-S3**: needs the `esp-rs` fork
  of rustc (`rustup target add` does not cover Xtensa; install via
  https://github.com/esp-rs/rust-build).
- **Cortex-A9 / A53**: hosted-RTOS only (NuttX, Zephyr). Heap +
  libc required; bare-metal Cortex-A is not in the coverage matrix.
- **PX4 boards**: NuttX is the underlying kernel; the PX4 module
  template is the canonical entry path — see
  [PX4 Autopilot](../getting-started/px4.md).

## See also

- [Quick board check (intro)](../introduction.md) — hobbyist-grade
  one-liner per chip.
- [Embedded Starters](../getting-started/freertos.md) — per-RTOS
  walkthrough.
- [Platform Differences](./platform-differences.md) — per-platform
  capability deltas.

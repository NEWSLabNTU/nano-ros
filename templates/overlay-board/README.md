# Overlay Board Crate Template

Skeleton for a vendor / community overlay on top of one of the
nano-ros generic board crates (`nros-board-freertos`,
`nros-board-threadx`, `nros-board-nuttx`,
`nros-board-baremetal-cortex-{m,a}`).

Copy this directory, rename the placeholders, and fill in the
vendor-specific bits. See `book/src/porting/vendor-overlay.md` for
the full cookbook + contract.

## Files

| File | Purpose | Edit |
|---|---|---|
| `Cargo.toml.template` | Crate manifest — deps on the generic crate. | Replace `{NAME}`, `{VENDOR}`, `{KERNEL}`, `{REPO}`, `{AUTHOR}`. |
| `src/lib.rs.template` | Re-exports `Config` / `run` from the generic crate + `#[no_mangle]` hook stubs. | Replace `{KERNEL_CRATE}`. Fill in vendor HAL calls. |
| `build.rs.template` | cc-rs HAL-source injection scaffold + env-var contract. | Replace `{VENDOR_SDK_DIR}` + the C source list. |
| `config/linker.ld.template` | Optional per-board linker script. | Fill in flash/RAM layout, vector table, etc. |
| `config/board_config.h.template` | Optional vendor kernel-config header. | E.g. `FreeRTOSConfig.h` or `tx_user.h` overrides. |

## Quick-start

```bash
cp -r templates/overlay-board nros-board-stm32f4-freertos
cd nros-board-stm32f4-freertos
mv Cargo.toml.template Cargo.toml
mv src/lib.rs.template  src/lib.rs
mv build.rs.template    build.rs
# edit each file: replace {NAME}, {VENDOR}, {KERNEL_CRATE}, etc.
```

## Naming

`nros-board-<vendor>-<chip-or-board>-<rtos>`. Examples:

- `nros-board-stm32f4-freertos`
- `nros-board-nxp-mimxrt1064-freertos`
- `nros-board-renesas-synergy-s7g2-threadx`

Crates.io has no namespacing — the `nros-board-` prefix is the
informal namespace.

## Generic-crate dependency table

| Your kernel | Generic crate | SDK env vars the generic crate needs |
|---|---|---|
| FreeRTOS + lwIP | `nros-board-freertos` | `FREERTOS_DIR`, `FREERTOS_PORT`, `LWIP_DIR`, `FREERTOS_CONFIG_DIR` (optional) |
| ThreadX + NetX Duo | `nros-board-threadx` | `THREADX_DIR`, `THREADX_CONFIG_DIR`, `NETX_DIR`, `NETX_CONFIG_DIR` |
| NuttX | `nros-board-nuttx` | `NUTTX_DIR` |
| bare-metal Cortex-M + smoltcp | `nros-board-baremetal-cortex-m` | `BOARD_LINKER_SCRIPT_DIR` |

Your overlay's `build.rs` reads ADDITIONAL env vars
(`STM32_HAL_DIR`, `NXP_SDK_DIR`, `NV_SPE_FSP_DIR`, …) for its own
HAL sources; the generic crate's env vars stay the user's
responsibility.

## Canonical in-tree precedents

- `packages/boards/nros-board-orin-spe/` — FSP-FreeRTOS overlay
  (NVIDIA FSP V10.4.3 kernel; IVC link instead of lwIP).
- `packages/boards/nros-board-mps2-an385-freertos/` — stock FreeRTOS
  kernel + LAN9118 Ethernet driver.

Read both for working code.

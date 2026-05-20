//! ESP32-S3 board-bringup smoke.
//!
//! Links `nros-board-esp32s3` into a real ELF and drives its `run`
//! entry: esp-hal `#[main]` → board `init_hardware` (chip + heap + clock
//! + RNG + serial) → the user closure → exit. No RMW / zenoh-pico — this
//! verifies the board entry + esp-hal startup + the full Xtensa link,
//! the strongest check short of physical-hardware boot.
//!
//! Build: `source ~/export-esp.sh && cargo build --release` — the
//! `esp` channel (rust-toolchain.toml) provides rustc + the xtensa
//! target; `export-esp.sh` puts the `xtensa-esp32s3-elf-gcc` linker on
//! PATH. Produces a real Xtensa ELF. Flash/run needs a physical
//! ESP32-S3 (`espflash flash --monitor`).

#![no_std]
#![no_main]

use esp_backtrace as _;
use nros_board_esp32s3::{esp_println, prelude::*};

nros_board_esp32s3::esp_bootloader_esp_idf::esp_app_desc!();

#[entry]
fn main() -> ! {
    run(Config::default(), |_config| {
        esp_println::println!("esp32s3 board bringup: run() reached, board init OK");
        Ok::<(), core::convert::Infallible>(())
    })
}

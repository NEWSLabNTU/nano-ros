//! Hardware init + `run()` entry for ESP32-S3 (serial transport).

use esp_hal::rng::Rng;

use nros_platform_esp32s3::random;

use crate::config::Config;

/// Derive an RNG seed from the locator so two boards on the same bus get
/// distinct zenoh-pico session IDs.
fn identity_seed(config: &Config) -> u32 {
    let mut seed = 0x9e37_79b9u32;
    for byte in config.zenoh_locator.as_bytes() {
        seed ^= u32::from(*byte);
        seed = seed.rotate_left(5).wrapping_mul(0x85eb_ca6b);
    }
    seed ^ config.baudrate
}

/// Initialize ESP32-S3 hardware + the serial transport.
///
/// Sets up peripherals, the heap allocator, the monotonic clock, and the
/// hardware RNG. Automatically called by [`run`].
pub fn init_hardware(config: &Config) {
    esp_println::println!("");
    esp_println::println!("========================================");
    esp_println::println!("  nros ESP32-S3 Platform");
    esp_println::println!("========================================");
    esp_println::println!("");

    esp_println::println!("Initializing ESP32-S3...");
    let _peripherals = esp_hal::init(esp_hal::Config::default());

    // esp-alloc carves the zenoh-pico / nros heap out of DRAM.
    esp_alloc::heap_allocator!(size: 96 * 1024);

    // Register the monotonic clock with the shared busy-wait sleep loop;
    // without it `sleep_ms` no-ops and zenoh-pico's connect polls zero times.
    nros_platform_esp32s3::sleep::init_clock();

    let rng = Rng::new();
    random::seed(rng.random() ^ identity_seed(config));

    init_serial(config);

    esp_println::println!("");
}

/// Serial transport uses zenoh-pico's built-in serial — no driver crate.
/// The zenoh locator (e.g. `serial/UART_0#baudrate=115200`) selects the
/// UART at session open.
fn init_serial(config: &Config) {
    esp_println::println!("Initializing serial transport...");
    esp_println::println!("  Baud: {}", config.baudrate);
    esp_println::println!("  Locator: {}", config.zenoh_locator);
    esp_println::println!("Serial ready.");
}

/// Run an application with the given configuration. Never returns.
pub fn run<F, E: core::fmt::Debug>(config: Config, f: F) -> !
where
    F: FnOnce(&Config) -> core::result::Result<(), E>,
{
    // Phase 173.1 — delegate to the shared direct-exec driver.
    nros_board_common::run::<Esp32S3, F, E>(config, f)
}

/// Phase 173.1 — board ZST carrying the `Board` super-trait impls.
pub struct Esp32S3;

impl nros_board_common::BoardInit for Esp32S3 {
    type Config = Config;

    fn init_hardware(cfg: &Config) {
        init_hardware(cfg);
        register_log_writer();

        // Phase 248 C5a (#60 T4) — the board owns RMW selection: register the
        // linked zenoh backend into the CFFI vtable here, before the user closure
        // opens an executor. Xtensa bare-metal (`target_os = "none"`) is
        // linkme-blind + runs no `.init_array`, so the auto-register section is a
        // no-op; this explicit, idempotent call is the registration path (mirrors
        // `nros-board-esp32-qemu`). Gated on the board's own `rmw-zenoh` feature.
        #[cfg(feature = "rmw-zenoh")]
        if let Err(err) = nros_rmw_zenoh::register() {
            esp_println::println!("zenoh RMW register failed: {:?}", err);
        }
    }
}

impl nros_board_common::BoardPrint for Esp32S3 {
    fn println(args: core::fmt::Arguments<'_>) {
        esp_println::println!("{}", args);
    }
}

impl nros_board_common::BoardExit for Esp32S3 {
    fn exit_success() -> ! {
        // ESP32 has no process exit — spin forever.
        #[allow(clippy::empty_loop)]
        loop {
            core::hint::spin_loop();
        }
    }

    fn exit_failure() -> ! {
        #[allow(clippy::empty_loop)]
        loop {
            core::hint::spin_loop();
        }
    }
}

/// Register an `esp_println`-backed writer with the platform log slot.
fn register_log_writer() {
    fn writer(severity: u8, name: &[u8], message: &[u8]) {
        let label = match severity {
            0 => "TRACE",
            1 => "DEBUG",
            2 => "INFO",
            3 => "WARN",
            4 => "ERROR",
            5 => "FATAL",
            _ => "?",
        };
        let name_str = core::str::from_utf8(name).unwrap_or("");
        let msg_str = core::str::from_utf8(message).unwrap_or("");
        if !name_str.is_empty() {
            esp_println::println!("[{}] {}: {}", label, name_str, msg_str);
        } else {
            esp_println::println!("[{}] {}", label, msg_str);
        }
    }
    nros_platform_esp32s3::register_log_writer(Some(writer));
}

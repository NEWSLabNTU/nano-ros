//! Platform entry point for NuttX QEMU ARM virt.
//!
//! NuttX is POSIX-compatible with `std` support, so this is much simpler than
//! bare-metal board crates. NuttX boots the kernel, initializes hardware
//! (virtio-net, serial console), and starts the application — no custom
//! hardware init needed in Rust.

use crate::config::Config;

/// Initialize hardware for NuttX.
///
/// On NuttX, the kernel handles all hardware and network initialization
/// before `main()` runs. This function is a no-op, provided for API
/// consistency with other board crates.
pub fn init_hardware(config: &Config) {
    // Board bringup and network init are handled by nsh_initialize() in entry.rs.

    // Seed /dev/urandom with the IP address to avoid duplicate Zenoh session IDs.
    // NuttX xorshift128 PRNG starts with a fixed seed → two QEMU instances
    // generate identical /dev/urandom output → identical ZIDs → zenohd rejects
    // the second connection (Close with MAX_LINKS reason).
    // Writing to /dev/urandom re-seeds the xorshift128 state.
    {
        use std::io::Write;
        if let Ok(mut f) = std::fs::OpenOptions::new()
            .write(true)
            .open("/dev/urandom")
        {
            let _ = f.write_all(&config.ip);
        }
    }
}

/// Run an nros application on NuttX.
///
/// This is the main entry point for NuttX applications. Call this from `main()`
/// with a configuration and a closure that sets up your nros executor.
///
/// NuttX handles all hardware and network initialization before `main()` runs.
/// Inside the closure, use `Executor::open()` to create an executor with full
/// API access (publishers, subscriptions, services, actions, timers, callbacks).
///
/// # Example
///
/// ```ignore
/// use nros::prelude::*;
/// use nros_nuttx_qemu_arm::{Config, run};
///
/// fn main() {
///     run(Config::default(), |config| {
///         let exec_config = ExecutorConfig::new(config.zenoh_locator)
///             .domain_id(config.domain_id);
///         let mut executor = Executor::open(&exec_config)?;
///         let mut node = executor.create_node("my_node")?;
///         // Full Executor API: publishers, subscriptions, services, actions...
///         Ok(())
///     })
/// }
/// ```
pub fn run<F, E: core::fmt::Debug>(config: Config, f: F) -> !
where
    F: FnOnce(&Config) -> Result<(), E>,
{
    init_hardware(&config);

    println!(
        "nros NuttX platform starting (IP: {}.{}.{}.{}, zenoh: {})",
        config.ip[0], config.ip[1], config.ip[2], config.ip[3], config.zenoh_locator
    );

    // Wait for NuttX networking to become ready.
    // NuttX's poll()/select() don't work correctly with Rust's connect_timeout,
    // so we use a fixed delay. With QEMU -icount shift=auto, this is real time.
    std::thread::sleep(std::time::Duration::from_secs(5));

    // Flush stdout before calling user closure
    use std::io::Write as _;
    let _ = std::io::stdout().flush();

    match f(&config) {
        Ok(()) => {
            println!("Application completed successfully.");
            let _ = std::io::stdout().flush();
            std::process::exit(0);
        }
        Err(e) => {
            eprintln!("Application error: {:?}", e);
            let _ = std::io::stdout().flush();
            std::process::exit(1);
        }
    }
}

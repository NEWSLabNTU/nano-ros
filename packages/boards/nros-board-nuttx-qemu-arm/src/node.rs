//! Platform entry point for NuttX QEMU ARM virt.
//!
//! NuttX is POSIX-compatible with `std` support, so this is much simpler than
//! bare-metal board crates. NuttX boots the kernel, initializes hardware
//! (virtio-net, serial console), and starts the application — no custom
//! hardware init needed in Rust.

use crate::config::Config;

/// Initialize hardware for NuttX.
///
/// On NuttX, the kernel handles most hardware and network initialization
/// before `main()` runs (NETINIT_IPADDR baked into the kernel
/// defconfig). This function:
///   * re-seeds `/dev/urandom` from `config.ip` so two QEMU instances
///     don't collide on Zenoh ZID / dust-dds GUID prefix;
///   * pushes `config.ip` into the live `eth0` interface via
///     `SIOCSIFADDR` so each instance overrides the kernel-baked IP
///     (otherwise both default to 10.0.2.30 from defconfig and DDS
///     SPDP source-IP collides; Phase 97.4.nuttx).
pub fn init_hardware(config: &Config) {
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

    // Override the defconfig-baked IP so sibling instances differ.
    apply_ip_config(config);
}

/// `SIOCSIFADDR` / `SIOCSIFNETMASK` / `SIOCSIFDSTADDR` (defconfig:
/// gateway) the configured `eth0` so the kernel routes the right
/// subnet without hard-coded NETINIT_IPADDR mismatches between
/// sibling QEMU guests.
fn apply_ip_config(config: &Config) {
    use std::os::unix::io::RawFd;

    // ioctl numbers — NuttX defines these as `_SIOC(N) = 0x0700 | N`,
    // *not* the Linux 0x89xx range (see
    // `nuttx/include/nuttx/net/ioctl.h`). Mismatching numbers fail
    // silently because the kernel returns ENOTTY and Rust ignores
    // the ioctl rc, so each guest keeps the defconfig 10.0.2.30 IP.
    const SIOCSIFADDR: core::ffi::c_ulong = 0x0700 | 0x0002;
    const SIOCSIFNETMASK: core::ffi::c_ulong = 0x0700 | 0x0008;
    const SIOCSIFDSTADDR: core::ffi::c_ulong = 0x0700 | 0x0004;

    #[repr(C)]
    struct sockaddr_in {
        sin_family: u16,
        sin_port: u16,
        sin_addr: u32,
        sin_zero: [u8; 8],
    }
    #[repr(C)]
    struct ifreq {
        ifr_name: [u8; 16],
        ifr_addr: sockaddr_in,
    }
    unsafe extern "C" {
        fn socket(
            domain: core::ffi::c_int,
            ty: core::ffi::c_int,
            proto: core::ffi::c_int,
        ) -> RawFd;
        fn ioctl(
            fd: RawFd,
            req: core::ffi::c_ulong,
            ...
        ) -> core::ffi::c_int;
        fn close(fd: RawFd) -> core::ffi::c_int;
    }
    const AF_INET: core::ffi::c_int = 2;
    const SOCK_DGRAM: core::ffi::c_int = 2;

    let fd = unsafe { socket(AF_INET, SOCK_DGRAM, 0) };
    if fd < 0 {
        return;
    }

    let pack = |a: [u8; 4]| -> u32 {
        // Network byte order: a[0] in low byte.
        (a[0] as u32) | ((a[1] as u32) << 8) | ((a[2] as u32) << 16) | ((a[3] as u32) << 24)
    };

    let mut name = [0u8; 16];
    for (i, b) in b"eth0".iter().enumerate() {
        name[i] = *b;
    }

    let mut req = ifreq {
        ifr_name: name,
        ifr_addr: sockaddr_in {
            sin_family: AF_INET as u16,
            sin_port: 0,
            sin_addr: pack(config.ip),
            sin_zero: [0; 8],
        },
    };
    unsafe {
        ioctl(fd, SIOCSIFADDR, &mut req);
    }

    let mask = {
        let bits = config.prefix.min(32);
        if bits == 0 {
            0
        } else if bits == 32 {
            !0u32
        } else {
            let host = !0u32 >> bits;
            (!0u32 ^ host).to_be()
        }
    };
    req.ifr_addr.sin_addr = mask;
    unsafe {
        ioctl(fd, SIOCSIFNETMASK, &mut req);
    }

    req.ifr_addr.sin_addr = pack(config.gateway);
    unsafe {
        ioctl(fd, SIOCSIFDSTADDR, &mut req);
    }

    unsafe {
        close(fd);
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
/// use nros_board_nuttx_qemu_arm::{Config, run};
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

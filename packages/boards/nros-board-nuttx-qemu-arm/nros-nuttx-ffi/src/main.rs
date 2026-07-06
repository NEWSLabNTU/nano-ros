//! NuttX kernel + FFI entry point for C/C++ examples.
//!
//! This binary provides the NuttX kernel (via -Z build-std=std) and calls
//! `app_main()` defined in C/C++ code (linked by CMake).

// Force-link crates so their symbols are available to C/C++ code.
// nros_board_nuttx_qemu_arm provides the NuttX kernel + board startup code.
extern crate nros_board_nuttx_qemu_arm;
extern crate nros_c;
extern crate nros_cpp;
extern crate nros_rmw_zenoh;

unsafe extern "C" {
    fn app_main();
}

/// Issue #130 — parse a compile-time-baked dotted IPv4 (`"10.0.2.30"`) into the
/// `[u8; 4]` the eth0 helper wants, falling back to `default` when the env var
/// is unset or malformed. Runtime parse (not `const`) — the value comes from
/// `option_env!`, which yields `Option<&'static str>`.
fn baked_ipv4(baked: Option<&str>, default: [u8; 4]) -> [u8; 4] {
    let Some(s) = baked else { return default };
    let mut out = [0u8; 4];
    let mut n = 0;
    for part in s.split('.') {
        match (n < 4, part.parse::<u8>()) {
            (true, Ok(b)) => {
                out[n] = b;
                n += 1;
            }
            _ => return default,
        }
    }
    if n == 4 { out } else { default }
}

fn main() {
    // Phase 104.A — bare-metal callers explicitly register the RMW
    // backend before `Executor::open`. POSIX hosts auto-register via
    // `.init_array`; this target doesn't walk that section.
    nros_rmw_zenoh::register().expect("Failed to register RMW backend");

    // Issue #130 — the C `nano_ros_entry LAUNCH` path reaches `app_main()`
    // WITHOUT going through the Rust `BoardEntry::run` wrappers, so nothing has
    // configured eth0 yet: the guest still holds its defconfig IP and cannot
    // reach slirp's `10.0.2.2`, and `app_main`'s `Executor::open` on the baked
    // locator would fail `Transport(ConnectionFailed)`. Push the guest IP into
    // eth0 via the SAME shared helper the Rust path uses. IP/netmask/gateway are
    // baked per-entry via `option_env!` (channel mirrors `NROS_ENTRY_LOCATOR`);
    // absent a bake, the slirp e2e defaults (`10.0.2.30/24` via `10.0.2.2`)
    // apply so an un-overridden C entry still connects.
    use nros_board_nuttx_qemu_arm::{
        SLIRP_DEFAULT_GATEWAY, SLIRP_DEFAULT_IP, SLIRP_DEFAULT_PREFIX, configure_entry_eth0,
    };
    let ip = baked_ipv4(option_env!("NROS_IP"), SLIRP_DEFAULT_IP);
    let gateway = baked_ipv4(option_env!("NROS_GATEWAY"), SLIRP_DEFAULT_GATEWAY);
    let prefix = option_env!("NROS_PREFIX")
        .and_then(|s| s.parse::<u8>().ok())
        .unwrap_or(SLIRP_DEFAULT_PREFIX);
    configure_entry_eth0(ip, prefix, gateway);

    unsafe { app_main() };
}

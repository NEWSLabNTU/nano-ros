//! NuttX kernel + FFI entry point for C/C++ examples.
//!
//! This binary provides the NuttX kernel (via -Z build-std=core,alloc) and calls
//! `nros_app_main()` defined in C/C++ code (linked by CMake).

// phase-359 W7 — `no_std` + `no_main`, like the other C-runtime families' bins.
// The image no longer compiles the standard library, so libstd's `lang_start`
// does not supply the `main` symbol NuttX's `nsh_main` calls; this bin defines
// it directly. `nros-c` (linked below) owns the image's `#[panic_handler]` and
// `#[global_allocator]`, which is why the board crate is taken here with
// `default-features = false`.
#![no_std]
#![no_main]

// Force-link crates so their symbols are available to C/C++ code.
// nros_board_nuttx_qemu provides the NuttX kernel + board startup code.
extern crate nros_board_nuttx_qemu;
extern crate nros_c;
extern crate nros_cpp;
extern crate nros_rmw_zenoh;

unsafe extern "C" {
    /// The app's entry, defined in C/C++ beside its `NROS_APP_MAIN_REGISTER()`.
    fn nros_app_main(argc: i32, argv: *mut *mut core::ffi::c_char) -> i32;
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

/// The image entry NuttX's `nsh_main` calls. Signature matches the
/// `extern "C" fn main(argc, argv)` that `nros-board-nuttx-qemu`'s `entry.rs`
/// declares and calls.
#[unsafe(no_mangle)]
pub extern "C" fn main(_argc: i32, _argv: *const *const core::ffi::c_char) -> i32 {
    // Phase 104.A — bare-metal callers explicitly register the RMW
    // backend before `Executor::open`. POSIX hosts auto-register via
    // `.init_array`; this target doesn't walk that section.
    nros_rmw_zenoh::register().expect("Failed to register RMW backend");

    // Issue #130 — the C `nano_ros_entry LAUNCH` path reaches `nros_app_main()`
    // WITHOUT going through the Rust `BoardEntry::run` wrappers, so nothing has
    // configured eth0 yet: the guest still holds its defconfig IP and cannot
    // reach slirp's `10.0.2.2`, and the app's `Executor::open` on the baked
    // locator would fail `Transport(ConnectionFailed)`. Push the guest IP into
    // eth0 via the SAME shared helper the Rust path uses. IP/netmask/gateway are
    // baked per-entry via `option_env!` (channel mirrors `NROS_ENTRY_LOCATOR`);
    // absent a bake, the board's SLIRP defaults (its kernel defconfig's
    // `10.0.2.15/24` via `10.0.2.2`, RFC-0109 D2) apply so an un-overridden C
    // entry still connects.
    use nros_board_nuttx_qemu::{
        SLIRP_DEFAULT_GATEWAY, SLIRP_DEFAULT_IP, SLIRP_DEFAULT_PREFIX, configure_entry_eth0,
    };
    let ip = baked_ipv4(option_env!("NROS_IP"), SLIRP_DEFAULT_IP);
    let gateway = baked_ipv4(option_env!("NROS_GATEWAY"), SLIRP_DEFAULT_GATEWAY);
    let prefix = option_env!("NROS_PREFIX")
        .and_then(|s| s.parse::<u8>().ok())
        .unwrap_or(SLIRP_DEFAULT_PREFIX);
    configure_entry_eth0(ip, prefix, gateway);

    // Issue 1770 — return the APP's status, the `int main` shim's contract
    // (`<rclc/detail/app_main.h>`). This called the VOID shim's `app_main` and
    // returned 0, so nsh saw success for an app that failed.
    // SAFETY: `nros_app_main` is the C entry every app defines; `argc = 0` with
    // a null `argv` is what the header's own VOID shim passes.
    unsafe { nros_app_main(0, core::ptr::null_mut()) }
}

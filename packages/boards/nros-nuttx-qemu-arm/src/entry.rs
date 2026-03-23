//! NuttX init entry point override.
//!
//! NuttX boots the kernel, then starts the init task by calling
//! `CONFIG_INIT_ENTRYPOINT` (default: `nsh_main`). The default `nsh_main`
//! from NuttX's apps library starts an interactive shell — our application
//! code is never called.
//!
//! This module provides a custom `nsh_main` that calls Rust's generated
//! `main` symbol (from `lang_start`), which in turn calls the user's
//! `fn main()`. Because this symbol is in the main binary, it takes
//! precedence over the archive definition in `libapps.a`.
//!
//! Call chain: NuttX init → `nsh_main` (ours) → `main` (Rust) → `fn main()`

use core::ffi::c_char;

unsafe extern "C" {
    fn main(argc: i32, argv: *const *const c_char) -> i32;
}

/// Override NuttX's default `nsh_main` to run the Rust application.
///
/// NuttX's scheduler calls this as the init task (PID 1). We redirect to
/// Rust's `main`, which initializes the Rust runtime and calls `fn main()`.
#[unsafe(no_mangle)]
pub extern "C" fn nsh_main(argc: i32, argv: *const *const c_char) -> i32 {
    unsafe { main(argc, argv) }
}

// Prevent linker from garbage-collecting nsh_main when --gc-sections is active.
// The NuttX kernel (libsched.a) references nsh_main, but the Rust linker may
// not see that reference early enough to keep the symbol.
#[used]
static _NSH_MAIN_REF: unsafe extern "C" fn(i32, *const *const c_char) -> i32 = nsh_main;

//! The C-ABI start of a C-rooted image (RFC-0130 D2, phase-553 W2).
//!
//! In a C-rooted image CMake owns the final link and the generated C/C++
//! entry owns `main`; cortex-m-rt (from this crate's staticlib) still owns
//! `Reset`, so `.data` and `.bss` are initialised before `main` runs. What
//! cortex-m-rt never does is run `.init_array`, so a C++ object at namespace
//! scope would stay unconstructed. So this start walks the preinit and init
//! arrays itself (C++ constructors, `__attribute__((constructor))`) before the
//! board brings up its clock and network and runs the application. The arrays
//! are kept and bounded by `c-runtime.x`, the C-rooted link's script (it
//! includes `link.x`). newlib's `__libc_init_array` is not used: it also calls `_init`
//! from `crti.o`, which a `-nostartfiles` link does not have (measured).

use crate::{Config, run_bare};

unsafe extern "C" {
    static __preinit_array_start: [unsafe extern "C" fn(); 0];
    static __preinit_array_end: [unsafe extern "C" fn(); 0];
    static __init_array_start: [unsafe extern "C" fn(); 0];
    static __init_array_end: [unsafe extern "C" fn(); 0];
}

/// Call every function pointer in `[start, end)`, in order.
///
/// # Safety
/// `start..end` must bound an array of valid `extern "C" fn()` pointers.
unsafe fn run_array(start: *const unsafe extern "C" fn(), end: *const unsafe extern "C" fn()) {
    let mut f = start;
    while f < end {
        // SAFETY: the linker fragment bounds the array; each entry is a
        // constructor the compiler emitted.
        unsafe { (*f)() };
        f = f.wrapping_add(1);
    }
}

/// Run the C/C++ static constructors: the preinit array, then the init array.
///
/// # Safety
/// Call once, after `.data`/`.bss` are initialised, in an image linked with
/// `c-runtime.x`.
pub unsafe fn run_static_constructors() {
    // SAFETY: the symbols come from `c-runtime.x`; see the module doc.
    unsafe {
        run_array(
            core::ptr::addr_of!(__preinit_array_start).cast(),
            core::ptr::addr_of!(__preinit_array_end).cast(),
        );
        run_array(
            core::ptr::addr_of!(__init_array_start).cast(),
            core::ptr::addr_of!(__init_array_end).cast(),
        );
    }
}

/// The application a C-rooted image runs once the board is up: returns 0 on a
/// clean exit, anything else is an error code.
pub type CApp = unsafe extern "C" fn() -> i32;

/// Run the C/C++ static constructors, bring the board up, then run `app`.
/// Never returns: the board reports the outcome and halts the image.
///
/// # Safety
/// Call once, from `main`, with `app` a valid function or null. A null `app`
/// is reported as an application error.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_board_mps2_an385_start(app: Option<CApp>) -> ! {
    // phase-553 W7 — guard the main stack before the constructors run on it.
    crate::stack_guard::install();
    // SAFETY: cortex-m-rt's `Reset` initialised `.data`/`.bss` before `main`.
    unsafe { run_static_constructors() };
    run_bare(Config::from_image(), |_cfg| {
        let Some(app) = app else {
            return Err(-22);
        };
        // SAFETY: the caller passes a valid C function.
        let status = unsafe { app() };
        if status == 0 { Ok(()) } else { Err(status) }
    })
}

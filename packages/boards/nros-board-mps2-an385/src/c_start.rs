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

/// The image identity the start takes (`<nros/board.h>`, issue 2555), named
/// here so the image crate's anchor (`NanoRosBareMetalPlatform.cmake`) can
/// spell the start's type without depending on the ABI crate itself.
pub type ImageConfig = nros_board_cffi::nros_board_image_config_t;

/// A NUL-terminated C string the generated entry passed, as `&'static str`.
/// `None` for a null pointer, an empty string (the entry's "not stated": its
/// locator macro is `""` when the image names none) or bytes that are not
/// UTF-8.
///
/// # Safety
/// `p` is null or a NUL-terminated string that lives for the whole image
/// (`<nros/board.h>` requires it; the generated entry passes literals).
unsafe fn static_c_str(p: *const core::ffi::c_char) -> Option<&'static str> {
    if p.is_null() {
        return None;
    }
    // SAFETY: the caller's contract above.
    unsafe { core::ffi::CStr::from_ptr(p) }
        .to_str()
        .ok()
        .filter(|s| !s.is_empty())
}

/// What [`nros_board_mps2_an385_start`] boots with: the image's identity from
/// `image` when the entry passed one, the board's own rungs otherwise
/// (issue 2555). `Err` names a struct version this board does not read.
///
/// # Safety
/// `image` is null or points at a valid `nros_board_image_config_t` whose
/// strings outlive the image.
unsafe fn boot_config(
    image: *const nros_board_cffi::nros_board_image_config_t,
) -> Result<Config, i32> {
    use nros_board_cffi::{NROS_BOARD_IMAGE_CONFIG_VERSION, NROS_BOARD_IMAGE_DOMAIN_DEFAULT};
    // SAFETY: the caller's contract above.
    let Some(image) = (unsafe { image.as_ref() }) else {
        return Ok(Config::from_image());
    };
    if image.version != NROS_BOARD_IMAGE_CONFIG_VERSION as u32 {
        // A newer struct may carry fields this board would ignore; refuse it
        // rather than boot an identity the image did not ask for.
        return Err(-71); // EPROTO
    }
    let domain = (image.domain_id != NROS_BOARD_IMAGE_DOMAIN_DEFAULT)
        .then(|| u32::try_from(image.domain_id).ok())
        .flatten();
    // SAFETY: the caller's contract above.
    let (net, locator) = unsafe { (static_c_str(image.net), static_c_str(image.locator)) };
    Ok(Config::from_image_parts(net, locator, domain))
}

/// Run the C/C++ static constructors, bring the board up, then run `app`.
/// Never returns: the board reports the outcome and halts the image.
///
/// `image` is the image's identity (`<nros/board.h>`'s
/// `nros_board_image_config_t`, issue 2555), filled per image by the
/// board's `c_main.c`; null boots the board's compile-time rungs.
///
/// # Safety
/// Call once, from `main`, with `app` a valid function or null, and `image`
/// null or valid as [`boot_config`] requires. A null `app` and an `image` of
/// an unknown version are reported as application errors.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_board_mps2_an385_start(
    image: *const nros_board_cffi::nros_board_image_config_t,
    app: Option<CApp>,
) -> ! {
    // phase-553 W7 — guard the main stack before the constructors run on it.
    crate::stack_guard::install();
    // SAFETY: cortex-m-rt's `Reset` initialised `.data`/`.bss` before `main`.
    unsafe { run_static_constructors() };
    // SAFETY: the caller's contract above.
    let boot = unsafe { boot_config(image) };
    let refused = boot.as_ref().err().copied();
    let cfg = boot.unwrap_or_else(|_| Config::from_image());
    run_bare(cfg, |_cfg| {
        if let Some(code) = refused {
            return Err(code);
        }
        let Some(app) = app else {
            return Err(-22);
        };
        // SAFETY: the caller passes a valid C function.
        let status = unsafe { app() };
        if status == 0 { Ok(()) } else { Err(status) }
    })
}

//! `run()` entry point for AGX Orin SPE applications.
//!
//! Unlike `nros-board-mps2-an385-freertos`, **the FSP boots the
//! FreeRTOS scheduler before user code**. The SPE firmware's `main()`
//! (linked separately by NVIDIA's Makefile) calls `app_init()` which
//! is itself executed inside a FreeRTOS task. So [`run()`] simply
//! spawns one application task and **returns** — the scheduler is
//! already up.

use core::ffi::c_void;

use crate::Config;

unsafe extern "C" {
    /// FSP-provided FreeRTOS task creator. Same shape as upstream's
    /// `xTaskCreate` — wrapping it in an `extern "C"` declaration here
    /// (instead of a generated FreeRTOS bindgen) keeps the board crate
    /// independent of the FSP's exact header layout, which varies
    /// across NVIDIA SDK Manager versions.
    fn xTaskCreate(
        entry: unsafe extern "C" fn(*mut c_void),
        name: *const core::ffi::c_char,
        stack_depth: u32,  // in WORDS, not bytes
        arg: *mut c_void,
        priority: u32,
        created_task: *mut *mut c_void,
    ) -> i32;

    /// FSP-provided heap allocator (FreeRTOS `heap_4`). Used for the
    /// app-task context box; lasts the program lifetime so we never
    /// `vPortFree` it.
    fn pvPortMalloc(size: u32) -> *mut c_void;

    /// Configure zenoh-pico's internal read+lease task priorities.
    /// Provided by `zpico-platform-shim`'s C glue, gated on
    /// `feature = "active"` (always selected by this crate).
    #[cfg(feature = "fsp")]
    fn zpico_set_task_config(
        read_priority: u32,
        read_stack_bytes: u32,
        lease_priority: u32,
        lease_stack_bytes: u32,
    );
}

/// Wrapper passed through `xTaskCreate`'s `void *` arg so we can
/// transport both the typed config and the user's `FnOnce` into the
/// task-entry trampoline.
struct AppContext<F> {
    config: Config,
    closure: F,
}

/// FreeRTOS task entry. Tunes zenoh-pico's task scheduling, then runs
/// the user's closure. Never returns — on closure exit (Ok or Err) it
/// blocks forever; the FSP is responsible for any orderly shutdown.
unsafe extern "C" fn app_task_entry<F, E>(arg: *mut c_void)
where
    F: FnOnce(&Config) -> core::result::Result<(), E>,
    E: core::fmt::Debug,
{
    // SAFETY: `arg` is the `Box::leak`-analogous `pvPortMalloc`
    // allocation written in `run()`; lives forever.
    let ctx = unsafe { &mut *(arg as *mut AppContext<F>) };

    #[cfg(feature = "fsp")]
    {
        let read_pri = Config::to_freertos_priority(ctx.config.zenoh_read_priority);
        let lease_pri = Config::to_freertos_priority(ctx.config.zenoh_lease_priority);
        unsafe {
            zpico_set_task_config(
                read_pri,
                ctx.config.zenoh_read_stack_bytes,
                lease_pri,
                ctx.config.zenoh_lease_stack_bytes,
            );
        }
    }

    // Take the closure out so we can invoke `FnOnce`. Single-call
    // guarantee comes from FreeRTOS — `xTaskCreate` calls this
    // function exactly once per task.
    let closure = unsafe { core::ptr::read(&ctx.closure) };

    match closure(&ctx.config) {
        Ok(()) => {
            crate::println!("");
            crate::println!("nros-board-orin-spe: application closure returned Ok.");
        }
        Err(e) => {
            crate::println!("nros-board-orin-spe: application error: {e:?}");
        }
    }

    // Block forever — the FSP doesn't expect application tasks to
    // exit. `vTaskDelete(NULL)` would also work but pulls another
    // FFI symbol; busy-yield via WFI is simpler and lets the FSP's
    // idle hook fire normally.
    loop {
        unsafe {
            core::arch::asm!("wfi", options(nomem, nostack, preserves_flags));
        }
    }
}

/// Pre-task hardware init.
///
/// On the SPE, FSP-managed hardware (TCU, HSP, IVC carveout setup) is
/// already initialised by the time `app_init()` runs. This function
/// is a no-op kept for API parity with other board crates.
pub fn init_hardware(_config: &Config) {}

/// Spawn the application closure on a fresh FreeRTOS task and return.
///
/// **Returns immediately** — the FSP's scheduler is already running
/// when this function is called. Contrast with
/// `nros-board-mps2-an385-freertos::run`, which is `-> !` because it
/// starts the scheduler.
///
/// # Safety / contract
///
/// - Must be called from within an existing FreeRTOS task (typically
///   the FSP's `app_init`-spawned task). Calling from an ISR will
///   panic — `pvPortMalloc` is not ISR-safe.
/// - The closure runs forever; on return the task blocks via `wfi`.
///
/// # Example
///
/// ```ignore
/// // app_init.c → calls nros_app_rust_entry()
/// #[unsafe(no_mangle)]
/// pub extern "C" fn nros_app_rust_entry() {
///     use nros_board_orin_spe::{run, Config};
///     run(Config::default(), |config| {
///         /* publish on /chatter etc. */
///         Ok::<(), &'static str>(())
///     });
/// }
/// ```
pub fn run<F, E>(config: Config, f: F)
where
    F: FnOnce(&Config) -> core::result::Result<(), E>,
    F: 'static,
    E: core::fmt::Debug + 'static,
{
    crate::println!("");
    crate::println!("========================================");
    crate::println!("  nros-board-orin-spe (Cortex-R5F)");
    crate::println!("========================================");
    crate::println!("  locator:  {}", config.zenoh_locator);
    crate::println!("  domain:   {}", config.domain_id);
    crate::println!("");

    // Stack the priority + stack size onto the local frame because the
    // config is moved into the heap context below.
    let app_pri = Config::to_freertos_priority(config.app_priority);
    let app_stack_words = config.app_stack_bytes / 4;

    let ctx_ptr = unsafe {
        let size = core::mem::size_of::<AppContext<F>>() as u32;
        let ptr = pvPortMalloc(size) as *mut AppContext<F>;
        assert!(
            !ptr.is_null(),
            "nros-board-orin-spe: pvPortMalloc returned null — heap exhausted?"
        );
        core::ptr::write(ptr, AppContext { config, closure: f });
        ptr
    };

    let mut handle: *mut c_void = core::ptr::null_mut();
    let ret = unsafe {
        xTaskCreate(
            app_task_entry::<F, E>,
            c"nros_app".as_ptr(),
            app_stack_words,
            ctx_ptr as *mut c_void,
            app_pri,
            &mut handle as *mut *mut c_void,
        )
    };

    if ret != 1 {
        // FreeRTOS xTaskCreate returns pdPASS = 1 on success.
        crate::println!(
            "nros-board-orin-spe: xTaskCreate failed (ret={ret}) — \
             check stack budget against BTCM"
        );
        // No way back; halt.
        loop {
            unsafe {
                core::arch::asm!("wfi", options(nomem, nostack, preserves_flags));
            }
        }
    }

    // Scheduler is already running — return into the FSP's `app_init`,
    // which usually loops or vTaskDeletes itself.
}

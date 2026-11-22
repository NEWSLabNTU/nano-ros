//! Issue 2732 -- a task stack the build never counted is refused at boot.
//!
//! The ThreadX byte pool's derived size counts the stacks of the sched tasks
//! the image's sizing descriptor states. A task stack the build could not see
//! (an owned entry, a bare `cargo build`, a task spawned by hand) is not in it,
//! and the pool's last line of defence is the refusal in
//! `nros_threadx_create_task`: a `FATAL` line naming the requested bytes, the
//! free bytes and the knob, then the image stops before the task runs.
//!
//! This image keeps its own `main`, opens no session and spawns one task whose
//! stack no pool this board derives can hold, so the refusal is reached
//! deterministically. The harness asserts the line, and that the task never
//! ran.

#![no_std]
#![no_main]

use nros_board_threadx_linux::ThreadxLinux;

/// The task's stack: 16 MiB, beyond any pool this board derives.
const REQUESTED_STACK_BYTES: core::ffi::c_ulong = 16 * 1024 * 1024;

unsafe extern "C" {
    fn nros_threadx_create_task(
        name: *const core::ffi::c_char,
        entry: unsafe extern "C" fn(*mut core::ffi::c_void),
        arg: *mut core::ffi::c_void,
        stack_bytes: core::ffi::c_ulong,
        priority: core::ffi::c_uint,
        preempt_threshold: core::ffi::c_int,
        time_slice_us: core::ffi::c_ulong,
    ) -> core::ffi::c_int;
    fn nros_board_log(msg: *const core::ffi::c_char);
}

unsafe extern "C" fn never_runs(_arg: *mut core::ffi::c_void) {
    // SAFETY: a NUL-terminated literal.
    unsafe { nros_board_log(c"sched-task-pool-refusal: the task ran\n".as_ptr()) };
}

// A `#![no_std]` + `#![no_main]` image: glibc's crt0 calls the C-ABI `main`
// below and this handler ends a panic through the board's
// `nros_platform_panic` (RFC-0077).
#[panic_handler]
fn panic(_info: &core::panic::PanicInfo) -> ! {
    unsafe extern "C" {
        fn nros_platform_panic(msg: *const u8, len: usize) -> !;
    }
    const MSG: &[u8] = b"sched-task-pool-refusal: panic";
    // SAFETY: a static byte string.
    unsafe { nros_platform_panic(MSG.as_ptr(), MSG.len()) }
}

#[unsafe(no_mangle)]
extern "C" fn main(_argc: i32, _argv: *const *const core::ffi::c_char) -> i32 {
    let _ = ThreadxLinux::run_bare(|| -> Result<(), ()> {
        // SAFETY: a NUL-terminated literal.
        unsafe { nros_board_log(c"sched-task-pool-refusal: spawning\n".as_ptr()) };
        // SAFETY: a NUL-terminated name and an entry that takes its argument
        // as an opaque pointer. The board refuses the stack and stops the
        // image, so this call does not return.
        let rc = unsafe {
            nros_threadx_create_task(
                c"unseen".as_ptr(),
                never_runs,
                core::ptr::null_mut(),
                REQUESTED_STACK_BYTES,
                20,
                -1,
                0,
            )
        };
        // Reached only if the refusal is gone: say so, for the harness.
        let _ = rc;
        // SAFETY: a NUL-terminated literal.
        unsafe {
            nros_board_log(c"sched-task-pool-refusal: the pool accepted the stack\n".as_ptr())
        };
        Ok(())
    });
    0
}

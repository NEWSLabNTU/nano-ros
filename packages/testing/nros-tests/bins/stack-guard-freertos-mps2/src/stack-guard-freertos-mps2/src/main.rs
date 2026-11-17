//! Issue 2507 — a FreeRTOS task stack overflow on MPS2-AN385 hits the board's
//! stack guard and names the task, instead of corrupting the heap block below
//! the stack and reporting `*** MALLOC FAILED ***`.
//!
//! Boots FreeRTOS with no session and, from the app task:
//!
//! 1. prints the app task's stack high-water mark. The guard is read-only, so
//!    `uxTaskGetStackHighWaterMark` can still scan it; the value is the same
//!    with the guard as without it;
//! 2. times a fixed number of context switches between two tasks of the same
//!    priority, which is what the guard's reprogramming adds to;
//! 3. starts a task with a small stack that recurses without bound. The guard
//!    must stop it: the board's fault handler prints the task's name and exits
//!    QEMU. If the app task wakes up again, nothing caught the overflow.

#![no_std]
#![no_main]

use core::{
    ffi::{c_char, c_void},
    ptr,
};

use cortex_m_semihosting::{
    debug::{EXIT_FAILURE, exit},
    hprintln,
};
use nros_board_mps2_an385_freertos::{Config, Mps2An385};
use panic_semihosting as _;

extern crate nros_platform as _;

const CONFIG: &str = include_str!("../config.toml");

/// Context switches timed in step 2: each of the two tasks yields this many
/// times.
const YIELDS: u32 = 200_000;
/// The overflowing task's stack, in words (2 KiB).
const OVERFLOW_STACK_WORDS: u32 = 512;
/// The peer task's stack, in words.
const PEER_STACK_WORDS: u32 = 256;

type RawTask = *mut c_void;

unsafe extern "C" {
    fn xTaskCreate(
        code: extern "C" fn(*mut c_void),
        name: *const c_char,
        stack_words: u32,
        param: *mut c_void,
        priority: u32,
        handle: *mut RawTask,
    ) -> i32;
    fn vTaskDelete(task: RawTask);
    fn vTaskDelay(ticks: u32);
    fn xTaskGetTickCount() -> u32;
    fn uxTaskPriorityGet(task: RawTask) -> u32;
    fn uxTaskGetStackHighWaterMark(task: RawTask) -> u32;
}

/// `pdPASS`.
const PD_PASS: i32 = 1;

extern "C" fn peer(_param: *mut c_void) {
    for _ in 0..YIELDS {
        // SAFETY: a FreeRTOS call from a task; a zero delay yields.
        unsafe { vTaskDelay(0) };
    }
    // SAFETY: a task deleting itself.
    unsafe { vTaskDelete(ptr::null_mut()) };
}

/// Recurse with a 128-byte frame that is written, so each level stores into
/// the stack and the compiler can neither drop the buffer nor turn the
/// recursion into a loop (the result is used after the call).
#[inline(never)]
fn deep(level: u32) -> u32 {
    let mut buf = [0u8; 128];
    for b in buf.iter_mut() {
        // SAFETY: a write to a local.
        unsafe { ptr::write_volatile(b, level as u8) };
    }
    let below = deep(core::hint::black_box(level + 1));
    // SAFETY: a read of a local.
    below.wrapping_add(unsafe { ptr::read_volatile(&buf[(level as usize) % 128]) } as u32)
}

extern "C" fn overflow(_param: *mut c_void) {
    let r = deep(0);
    hprintln!("stack-guard: FAIL: the recursion returned {}", r);
    exit(EXIT_FAILURE);
}

#[unsafe(no_mangle)]
extern "C" fn main() -> ! {
    let _ = Mps2An385::run_bare(Config::from_toml(CONFIG), |_config| {
        // SAFETY: FreeRTOS calls from the app task; NULL names the caller.
        let words = unsafe { uxTaskGetStackHighWaterMark(ptr::null_mut()) };
        hprintln!("stack-guard: app high-water {} bytes", words * 4);

        // SAFETY: as above.
        let prio = unsafe { uxTaskPriorityGet(ptr::null_mut()) };
        let mut handle: RawTask = ptr::null_mut();
        // SAFETY: a valid entry, a NUL-terminated name, and an out pointer.
        let created = unsafe {
            xTaskCreate(
                peer,
                c"peer".as_ptr(),
                PEER_STACK_WORDS,
                ptr::null_mut(),
                prio,
                &mut handle,
            )
        };
        if created != PD_PASS {
            hprintln!("stack-guard: FAIL: could not create the peer task");
            exit(EXIT_FAILURE);
        }
        // SAFETY: FreeRTOS calls from the app task.
        let start = unsafe { xTaskGetTickCount() };
        for _ in 0..YIELDS {
            // SAFETY: as above.
            unsafe { vTaskDelay(0) };
        }
        // SAFETY: as above.
        let ticks = unsafe { xTaskGetTickCount() }.wrapping_sub(start);
        hprintln!("stack-guard: {} switches took {} ticks", 2 * YIELDS, ticks);

        hprintln!("stack-guard: overflowing the task 'overflow'");
        // SAFETY: as above.
        let created = unsafe {
            xTaskCreate(
                overflow,
                c"overflow".as_ptr(),
                OVERFLOW_STACK_WORDS,
                ptr::null_mut(),
                prio,
                &mut handle,
            )
        };
        if created != PD_PASS {
            hprintln!("stack-guard: FAIL: could not create the overflow task");
            exit(EXIT_FAILURE);
        }
        // SAFETY: as above.
        unsafe { vTaskDelay(2000) };
        hprintln!("stack-guard: FAIL: the overflow was not caught");
        exit(EXIT_FAILURE);

        #[allow(unreachable_code)]
        Ok::<(), &'static str>(())
    });
    unreachable!()
}

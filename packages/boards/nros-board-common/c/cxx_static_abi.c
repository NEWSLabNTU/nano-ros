/*
 * cxx_static_abi.c — the C++ static-object ABI for an image that NEVER EXITS.
 *
 * A C++ object with static storage duration and a destructor registers that
 * destructor through `__cxa_atexit(dtor, obj, &__dso_handle)`. A hosted image
 * gets both from the C runtime (`crtbegin.o` defines `__dso_handle`; libc's
 * `__cxa_atexit` keeps a list `exit()` walks). A bare-metal `-nostartfiles`
 * image links no crt, so:
 *
 *   - `__dso_handle` is undefined (phase-370 W4: `undefined reference to
 *     __dso_handle` on the FreeRTOS C++ workspace entry);
 *   - libc's `__cxa_atexit` drags in newlib's exit machinery — `__call_atexit.o`
 *     (`register_fini`, a weak reference to `__libc_fini`) and `fini.o`
 *     (`__libc_fini_array`, which needs `__fini_array_start/end`). On RISC-V
 *     medany those references are PC-relative, and an undefined weak or an
 *     absent section resolves near address 0, ~2 GiB from `.text` at
 *     0x80000000: `relocation R_RISCV_PCREL_HI20 out of range` (issue 1753,
 *     the rv-virt-threadx TYPED carrier, whose generated entry holds
 *     `static ::rclcpp::Node`).
 *
 * Defining them as no-ops is CORRECT here rather than a papering-over: these
 * images never exit. The RTOS scheduler owns the CPU until power-off, so a
 * static destructor could only run at a point that does not exist, and a
 * registration list would be RAM nothing walks. `__dso_handle`'s value is never
 * dereferenced — it is an identity token for "which shared object registered
 * this", and there is exactly one.
 *
 * One definition for every never-exiting board family (issue 1753 moved it
 * here from `nros-board-freertos/c/freertos_hooks.c`, where phase-370 wrote it,
 * so the ThreadX family stops lacking it). NOT for a host-process board
 * (threadx-linux, freertos-posix): those link the real C runtime, which
 * defines all four.
 */

void *__dso_handle = 0;

int __cxa_atexit(void (*destructor)(void *), void *arg, void *dso) {
    (void) destructor;
    (void) arg;
    (void) dso;
    /* 0 = "registered". Reporting failure would make a conforming C++ runtime
     * call `std::terminate` at static-init time. */
    return 0;
}

void __cxa_finalize(void *dso) {
    (void) dso;
}

void _fini(void) {
}

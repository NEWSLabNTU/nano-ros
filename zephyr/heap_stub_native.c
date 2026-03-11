/*
 * Heap symbol stubs for native_sim.
 *
 * Rust staticlibs built with std pull picolibc's nano-malloc into the link,
 * and picolibc's sbrk references __heap_start/__heap_end. These symbols are
 * normally provided by embedded linker scripts but don't exist on native_sim.
 *
 * Provide weak stubs so the link succeeds. Actual allocation uses Zephyr's
 * COMMON_LIBC_MALLOC (linked first via --allow-multiple-definition), so
 * picolibc's sbrk is never called at runtime.
 */

__attribute__((weak)) char __heap_start;
__attribute__((weak)) char __heap_end;

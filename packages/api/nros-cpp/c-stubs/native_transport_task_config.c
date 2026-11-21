/*
 * The C/C++ native runners' way to zenoh-pico's `zpico_set_task_config`.
 *
 * nros-cpp is built against the CFFI RMW, so it does not know at compile time
 * which backend the image links (cmake's `nano_ros_link_rmw` decides that, at
 * the final link). A plain reference to `zpico_set_task_config` would be an
 * undefined symbol in every XRCE or Cyclone image; the Rust board records that
 * exact failure for its own unguarded call. So this is a WEAK reference (a
 * declaration, not a definition): it resolves when zpico.c is linked, which the
 * zenoh backend whole-archives, and is NULL otherwise, where an image has no
 * transport tasks to place.
 *
 * Linux only, like the runners that call it. A weak-undefined call cannot be
 * relocated on every target (RISC-V), and no other target reaches this.
 *
 * Returns 1 when the priority reached a transport, 0 when none is linked. The
 * decision of WHICH priority is `nros::transport_band`, shared with the Rust
 * board.
 */

#include <stdint.h>

int nros_cpp_native_set_transport_priority(uint32_t priority);

#if defined(__linux__) && (defined(__GNUC__) || defined(__clang__))
extern void zpico_set_task_config(uint32_t read_priority, uint32_t read_stack_bytes)
    __attribute__((weak));

int nros_cpp_native_set_transport_priority(uint32_t priority) {
    if (zpico_set_task_config == 0) {
        return 0;
    }
    /* 0 stack bytes = "leave the port's default alone": this states a
     * PRIORITY, not a stack size. */
    zpico_set_task_config(priority, 0u);
    return 1;
}
#else
int nros_cpp_native_set_transport_priority(uint32_t priority) {
    (void)priority;
    return 0;
}
#endif

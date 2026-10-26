/*
 * Issue 2078 -- `nros_platform_zephyr_wait_network` for an image with no IP
 * stack (`CONFIG_NET_SOCKETS` off: a serial-only zenoh image).
 *
 * `net_wait.c` is IP-stack code and is compiled only with `CONFIG_NET_SOCKETS`
 * (zephyr/CMakeLists.txt). The symbol is part of the Zephyr platform surface
 * all the same: the Rust entry that `nros::main!` emits calls it before opening
 * the session, whatever the transport, and the C/C++ examples call it under their
 * own guard. With no interface to come up there is nothing to wait for, so the
 * answer is "ready", at once.
 */

#include <nros/platform_zephyr.h>

int32_t nros_platform_zephyr_wait_network(int timeout_ms) {
    (void)timeout_ms;
    return 0;
}

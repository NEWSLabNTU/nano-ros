/* Size probe: encode struct sizes as symbol array lengths.
 * build.rs compiles this with the same flags as zenoh-pico, then reads
 * the symbol sizes from the .o file to determine _z_sys_net_socket_t
 * and _z_sys_net_endpoint_t sizes for the target platform.
 *
 * This avoids hardcoding platform-specific sizes in Rust — the C compiler
 * calculates them from the actual headers.
 */
#include "zenoh-pico/system/platform.h"
#include "zenoh-pico/system/link/tcp.h"

/* Array length == sizeof(type). llvm-nm reports the size. */
const unsigned char __nros_sizeof_net_socket[sizeof(_z_sys_net_socket_t)] = {0};
const unsigned char __nros_sizeof_net_endpoint[sizeof(_z_sys_net_endpoint_t)] = {0};

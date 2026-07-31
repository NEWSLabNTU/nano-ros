/* Wrapper header for bindgen — Zephyr POSIX socket types.
 *
 * Requires a Zephyr build tree (for autoconf.h, devicetree_generated.h).
 * Include paths extracted from compile_commands.json.
 *
 * autoconf.h must be included FIRST — Zephyr's toolchain/gcc.h checks
 * CONFIG_ARCH_POSIX which is defined there.
 */

/* Kconfig-generated defines (CONFIG_ARCH_POSIX, etc.) */
#include <zephyr/autoconf.h>

/* Zephyr POSIX socket API */
#include <zephyr/net/socket.h>
#include <zephyr/posix/netdb.h>
#include <zephyr/posix/fcntl.h>
#include <zephyr/posix/sys/socket.h>

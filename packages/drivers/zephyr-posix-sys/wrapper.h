/* Wrapper header for bindgen — Zephyr POSIX socket types.
 *
 * Requires a Zephyr build tree (for autoconf.h, devicetree_generated.h).
 * Include paths extracted from compile_commands.json.
 */

/* Zephyr POSIX socket API */
#include <zephyr/net/socket.h>
#include <zephyr/posix/netdb.h>
#include <zephyr/posix/fcntl.h>
#include <zephyr/posix/sys/socket.h>

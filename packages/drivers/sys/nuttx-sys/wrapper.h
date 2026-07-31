/* Wrapper header for bindgen — NuttX POSIX socket types and functions.
 *
 * NuttX provides standard POSIX headers. Requires:
 *   - NUTTX_DIR/include (NuttX kernel headers)
 */

#include <sys/socket.h>
#include <sys/time.h>
#include <netdb.h>
#include <netinet/in.h>
#include <netinet/tcp.h>
#include <arpa/inet.h>
#include <fcntl.h>
#include <unistd.h>

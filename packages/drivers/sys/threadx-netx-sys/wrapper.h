/* Wrapper header for bindgen — pulls in NetX Duo BSD socket types and functions.
 *
 * ThreadX + NetX Duo must be configured via include paths:
 *   - THREADX_DIR/common/inc        (ThreadX kernel headers)
 *   - THREADX_DIR/ports/<port>/inc  (port-specific: tx_port.h)
 *   - THREADX_CONFIG_DIR            (tx_user.h, nx_user.h)
 *   - NETX_DIR/common/inc           (NetX Duo headers)
 *   - NETX_DIR/ports/<port>/inc     (port-specific: nx_port.h)
 *   - NETX_DIR/addons/BSD           (nxd_bsd.h)
 */

#include "tx_api.h"
#include "nx_api.h"
#include "nxd_bsd.h"

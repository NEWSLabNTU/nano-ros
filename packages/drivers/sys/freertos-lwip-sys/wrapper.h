/* Wrapper header for bindgen — pulls in lwIP BSD socket types and functions.
 *
 * FreeRTOS + lwIP must be configured via include paths:
 *   - FREERTOS_DIR/include          (FreeRTOS kernel headers)
 *   - FREERTOS_PORT dir             (portmacro.h)
 *   - FREERTOS_CONFIG_DIR           (FreeRTOSConfig.h, lwipopts.h)
 *   - LWIP_DIR/src/include          (lwIP core headers)
 */

#include "FreeRTOS.h"
#include "lwip/sockets.h"
#include "lwip/netdb.h"

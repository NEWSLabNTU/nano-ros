/*
 * network_glue.c — lwIP + FreeRTOS network plumbing called from Rust
 *
 * Phase 152.1.B.1 — extracted from build.rs's `STARTUP_C` const.
 * Phase 152.1.B.2 — board-specific Ethernet init lifted to the
 * weak `nros_board_register_netif` / `nros_board_poll_netif`
 * hooks the overlay implements (see `board_mps2.c` for the
 * LAN9118 strong override). This TU is now board-agnostic and
 * can promote into the generic `nros-board-freertos` crate at
 * 152.1.B.4 without further changes.
 */

#include <stdint.h>
#include <stdlib.h>
#include <errno.h>

#include "FreeRTOS.h"
#include "task.h"

#include "lwip/init.h"
#include "lwip/tcpip.h"
#include "lwip/netif.h"
#include "lwip/ip4_addr.h"
#include "lwip/sockets.h"

/* ---- Weak board-init contract ----
 *
 * Overlays implement these to register their Ethernet driver with
 * lwIP + drive the poll loop. Default no-ops keep the generic
 * code linkable when an overlay is intentionally serial-only or
 * loopback-only.
 *
 * Signature contract:
 *   - nros_board_register_netif(mac, ip, netmask, gw): called
 *     once after tcpip_init completes. Set up the board's netif,
 *     call netifapi_netif_add + set_default + set_up + set_link_up.
 *     Return 0 on success, -1 on failure.
 *   - nros_board_poll_netif(): called periodically from the poll
 *     task. Drives the board's netif RX-FIFO drain (LAN9118,
 *     etc.). Default no-op for boards that use IRQ-driven RX.
 */
__attribute__((weak)) int nros_board_register_netif(
    const uint8_t mac[6],
    const uint8_t ip[4],
    const uint8_t netmask[4],
    const uint8_t gw[4])
{
    (void)mac; (void)ip; (void)netmask; (void)gw;
    return -1;  /* No board override → no Ethernet. */
}

__attribute__((weak)) void nros_board_poll_netif(void) {
    /* No board override → nothing to poll. */
}

/* ---- lwIP init bookkeeping ---- */
static volatile int lwip_init_done = 0;

static void tcpip_init_done_cb(void *arg) {
    (void)arg;
    lwip_init_done = 1;
}

/* ---- Public C API called from Rust ---- */

/*
 * Initialise the LAN9118 Ethernet + lwIP stack.
 *
 * Parameters are passed from Rust config:
 *   mac[6], ip[4], netmask[4], gateway[4]
 *
 * Returns 0 on success, -1 on failure.
 */
int nros_freertos_init_network(
    const uint8_t mac[6],
    const uint8_t ip[4],
    const uint8_t netmask[4],
    const uint8_t gw[4])
{
    /* Seed the C stdlib RNG with a value unique to this node.
     * Without this, rand() starts from seed 1 on every boot, causing
     * all QEMU instances to generate identical zenoh-pico session IDs
     * (16 bytes from LWIP_RAND → rand()). zenohd rejects duplicate
     * session IDs, so the second QEMU's z_open() always fails.
     *
     * Use IP octets directly — each node has a unique IP. Multiply to
     * spread bits and avoid XOR cancellation between MAC and IP. */
    {
        uint32_t seed = ((uint32_t)ip[0] << 24) | ((uint32_t)ip[1] << 16)
                      | ((uint32_t)ip[2] << 8)  | (uint32_t)ip[3];
        seed = seed * 2654435761u;  /* Knuth multiplicative hash */
        seed ^= ((uint32_t)mac[4] << 8) | (uint32_t)mac[5];
        if (seed == 0) seed = 1;
        srand(seed);
    }

    /* Initialize per-thread lwIP semaphore for the app task.
     * Required when LWIP_NETCONN_SEM_PER_THREAD=1 — each task that calls
     * lwIP socket/netifapi functions must have its own semaphore.
     * Must be called before any lwIP API (including netifapi_netif_add). */
    lwip_socket_thread_init();

    /* Start lwIP's tcpip_thread (scheduler must be running) */
    tcpip_init(tcpip_init_done_cb, NULL);
    while (!lwip_init_done) {
        vTaskDelay(1);
    }

    /* Delegate netif registration to the board overlay.
     * Default weak impl returns -1 (no Ethernet); LAN9118 / STM ETH
     * / NXP ENET / etc. overlays provide the strong version. */
    return nros_board_register_netif(mac, ip, netmask, gw);
}

/*
 * Drive the board's netif RX-FIFO drain. Called periodically from
 * the poll task. Default weak impl is a no-op (boards with
 * IRQ-driven RX or no Ethernet leave it alone).
 */
void nros_freertos_poll_network(void) {
    nros_board_poll_netif();
}

/*
 * Start the FreeRTOS scheduler.  Does not return.
 */
void nros_freertos_start_scheduler(void) {
    vTaskStartScheduler();
    /* Should never reach here */
    for (;;) {}
}

/*
 * Create a FreeRTOS task.
 * Returns 0 on success, -1 on failure.
 */
int nros_freertos_create_task(
    void (*entry)(void *),
    const char *name,
    uint32_t stack_words,
    void *arg,
    uint32_t priority)
{
    /* configSTACK_DEPTH_TYPE defaults to StackType_t (uint32_t on Cortex-M3
     * via portmacro.h). The previous (uint16_t) cast silently truncated
     * stack depths > 65535 words (>256 KB), leaving tasks with a 0-word
     * stack and a wild SP. Drop the cast — xTaskCreate accepts the full
     * uint32_t we already declared in this wrapper. */
    BaseType_t ret = xTaskCreate(entry, name, stack_words, arg,
                                 (UBaseType_t)priority, NULL);
    return (ret == pdPASS) ? 0 : -1;
}

/*
 * Test TCP connectivity to a given IPv4 address and port.
 * Returns 0 on success, or the positive errno value on failure.
 */
int nros_freertos_test_tcp_connect(const uint8_t ip[4], uint16_t port) {
    struct sockaddr_in addr;
    int sock;

    sock = lwip_socket(AF_INET, SOCK_STREAM, 0);
    if (sock < 0) {
        return errno ? errno : 1;
    }

    memset(&addr, 0, sizeof(addr));
    addr.sin_family = AF_INET;
    addr.sin_port = lwip_htons(port);
    addr.sin_addr.s_addr = lwip_htonl(
        ((uint32_t)ip[0] << 24) |
        ((uint32_t)ip[1] << 16) |
        ((uint32_t)ip[2] << 8)  |
        (uint32_t)ip[3]);

    /* Set a 10-second connect/receive timeout. */
    struct timeval tv;
    tv.tv_sec = 10;
    tv.tv_usec = 0;
    lwip_setsockopt(sock, SOL_SOCKET, SO_RCVTIMEO, &tv, sizeof(tv));

    int ret = lwip_connect(sock, (struct sockaddr *)&addr, sizeof(addr));
    if (ret < 0) {
        int err = errno;
        lwip_close(sock);
        return err ? err : 1;
    }
    lwip_close(sock);
    return 0;
}

/*
 * Query lwIP netif state for diagnostics.
 * Returns a bitmask:
 *   bit 0: netif_default is set
 *   bit 1: netif is UP
 *   bit 2: link is UP
 *   bit 3: has IP address (non-zero)
 */
int nros_freertos_get_netif_state(void) {
    int flags = 0;
    if (netif_default != NULL) {
        flags |= 1;
        if (netif_default->flags & NETIF_FLAG_UP) flags |= 2;
        if (netif_default->flags & NETIF_FLAG_LINK_UP) flags |= 4;
        if (netif_default->ip_addr.addr != 0) flags |= 8;
    }
    return flags;
}

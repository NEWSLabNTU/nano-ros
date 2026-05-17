/*
 * network_glue.c — lwIP + FreeRTOS network plumbing called from Rust
 *
 * Phase 149.1.B.1 — extracted from build.rs's `STARTUP_C` const.
 * Contains the FFI surface Rust calls (`nros_freertos_init_network`,
 * `_poll_network`, `_start_scheduler`, `_create_task`,
 * `_get_netif_state`, `_test_tcp_connect`) + the lwIP init wiring
 * (`tcpip_init` + `netifapi_netif_add`). The LAN9118 chunk inside
 * `nros_freertos_init_network` calls `lan9118_lwip_init` from the
 * driver crate; the LAN9118 *direct-register* poking lives in
 * `board_mps2.c`. Promotion to the generic `nros-board-freertos`
 * crate is 149.1.B.4, gated on 149.1.B.2 lifting the
 * `nros_board_init_eth` weak-hook contract.
 */

#include <stdint.h>
#include <string.h>
#include <stdlib.h>
#include <errno.h>

#include "FreeRTOS.h"
#include "task.h"

#include "lwip/init.h"
#include "lwip/tcpip.h"
#include "lwip/netif.h"
#include "lwip/netifapi.h"
#include "lwip/ip4_addr.h"
#include "lwip/sockets.h"

#include "lan9118_lwip.h"

/* ---- Network globals (also accessed from board_mps2.c for diag) ---- */
struct netif lan9118_netif;
struct lan9118_config lan9118_cfg;
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
    ip4_addr_t ipaddr, mask, gateway;

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

    IP4_ADDR(&ipaddr,  ip[0], ip[1], ip[2], ip[3]);
    IP4_ADDR(&mask,    netmask[0], netmask[1], netmask[2], netmask[3]);
    IP4_ADDR(&gateway, gw[0], gw[1], gw[2], gw[3]);

    lan9118_cfg.base_addr = LAN9118_BASE_DEFAULT;
    memcpy(lan9118_cfg.mac_addr, mac, 6);

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

    /* Register netif via netifapi (thread-safe: executes in tcpip_thread).
     * Note: netif_add() does NOT set netif_default, even with LWIP_SINGLE_NETIF.
     * We must call netif_set_default() explicitly. */
    if (netifapi_netif_add(&lan9118_netif, &ipaddr, &mask, &gateway,
                           &lan9118_cfg, lan9118_lwip_init, tcpip_input) != ERR_OK) {
        return -1;
    }

    netifapi_netif_set_default(&lan9118_netif);
    netifapi_netif_set_up(&lan9118_netif);
    netifapi_netif_set_link_up(&lan9118_netif);

    return 0;
}

/*
 * Poll the LAN9118 RX FIFO for received frames.
 * Call from a FreeRTOS task periodically.
 */
void nros_freertos_poll_network(void) {
    lan9118_lwip_poll(&lan9118_netif);
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

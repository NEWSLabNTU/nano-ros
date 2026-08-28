/**
 * LAN9118/SMSC911x lwIP netif driver for QEMU MPS2-AN385
 *
 * Polling-based Ethernet driver implementing the lwIP netif interface.
 * Register layout and init sequence match the lan9118-smoltcp Rust driver.
 *
 * Usage:
 *   struct netif nif;
 *   struct lan9118_config cfg = {
 *       .base_addr = 0x40200000,
 *       .mac_addr  = {0x02, 0x00, 0x00, 0x00, 0x00, 0x01},
 *   };
 *   netif_add(&nif, &ip, &mask, &gw, &cfg, lan9118_lwip_init, ethernet_input);
 *   netif_set_default(&nif);
 *   netif_set_up(&nif);
 *
 *   // In a polling loop or FreeRTOS task:
 *   lan9118_lwip_poll(&nif);
 */

#ifndef LAN9118_LWIP_H
#define LAN9118_LWIP_H

#include "lwip/err.h"
#include "lwip/netif.h"

#ifdef __cplusplus
extern "C" {
#endif

/** Default base address on MPS2-AN385 QEMU */
#define LAN9118_BASE_DEFAULT 0x40200000UL

/** Driver configuration — passed as netif_add() state argument */
struct lan9118_config {
    uint32_t base_addr;    /**< MMIO base address */
    uint8_t  mac_addr[6];  /**< Ethernet MAC address */
};

/**
 * RX drop counters (issue 0836).
 *
 * The receive path discards a frame for three reasons that are otherwise
 * invisible — a hardware error status, a length outside the sane range, and
 * pbuf-pool exhaustion — and each one looks exactly like "the FIFO was empty"
 * from outside. A burst that loses most of its frames cannot be attributed
 * without these.
 */
struct lan9118_rx_stats {
    uint32_t delivered;         /**< frames handed to lwIP and accepted */
    uint32_t err_status;        /**< RX_STAT_ES set — discarded */
    uint32_t bad_length;        /**< length < 4 or > MAX_FRAME_SIZE — discarded */
    uint32_t no_pbuf;           /**< pbuf_alloc failed — discarded */
    uint32_t input_err;         /**< netif->input() rejected it — freed */
    uint32_t budget_exhausted;  /**< polls that returned with frames still pending */
};

/**
 * Read the RX drop counters. Counters are cumulative and never reset.
 *
 * @param out  Filled with a snapshot. Ignored if NULL.
 */
void lan9118_lwip_rx_stats(struct lan9118_rx_stats *out);

/**
 * lwIP netif init callback.
 *
 * Pass as the `init` argument to netif_add(). The `state` argument must
 * point to a `struct lan9118_config` that remains valid for the lifetime
 * of the netif.
 *
 * @param netif  The network interface to initialise.
 * @return ERR_OK on success, ERR_IF on hardware failure.
 */
err_t lan9118_lwip_init(struct netif *netif);

/**
 * Poll for received packets and pass them to lwIP.
 *
 * Call this periodically from a FreeRTOS task. Each call drains all
 * packets currently waiting in the RX FIFO.
 *
 * @param netif  The network interface to poll.
 */
void lan9118_lwip_poll(struct netif *netif);

/**
 * Check if the Ethernet link is up.
 *
 * Reads the PHY BMSR register and returns nonzero if link status is set.
 *
 * @param netif  The network interface.
 * @return 1 if link is up, 0 otherwise.
 */
int lan9118_lwip_link_is_up(struct netif *netif);

#ifdef __cplusplus
}
#endif

#endif /* LAN9118_LWIP_H */

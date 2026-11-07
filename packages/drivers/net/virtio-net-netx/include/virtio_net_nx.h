/**
 * VirtIO-net NetX Duo Ethernet driver for QEMU RISC-V virt machine
 *
 * Implements the NX_IP_DRIVER interface over VirtIO MMIO transport (modern,
 * version 2 only). Designed for the QEMU virt machine's virtio-net device
 * (8 MMIO slots starting at 0x10001000, IRQs 1-8 on PLIC).
 *
 * Usage:
 *   static const struct virtio_net_nx_config cfg = {
 *       .mmio_base = 0x10001000,
 *       .irq_num   = 1,
 *   };
 *   virtio_net_nx_configure(&cfg);
 *
 *   // Then pass virtio_net_nx_driver as the driver entry to nx_ip_create():
 *   nx_ip_create(&ip, "ip", addr, mask, &pool,
 *                virtio_net_nx_driver, stack, stack_size, priority);
 */

#ifndef VIRTIO_NET_NX_H
#define VIRTIO_NET_NX_H

#include "nx_api.h"

#ifdef __cplusplus
extern "C" {
#endif

/** Driver configuration -- set before calling nx_ip_create() */
struct virtio_net_nx_config {
    ULONG64 mmio_base;   /**< VirtIO MMIO base (0x10001000 for QEMU virt slot 0) */
    int     irq_num;     /**< PLIC IRQ number (1 for slot 0) */
};

/**
 * Configure the virtio-net driver.
 *
 * Must be called before nx_ip_create(). The config struct is copied
 * internally; the caller's copy does not need to persist.
 *
 * @param config  Pointer to driver configuration.
 */
void virtio_net_nx_configure(const struct virtio_net_nx_config *config);

/**
 * NetX Duo driver entry point.
 *
 * Pass this function as the driver_entry argument to nx_ip_create().
 * Handles NX_LINK_INITIALIZE, NX_LINK_ENABLE, NX_LINK_DISABLE,
 * NX_LINK_PACKET_SEND, NX_LINK_DEFERRED_PROCESSING, and related commands.
 *
 * @param driver_req_ptr  NetX Duo driver request structure.
 */
VOID virtio_net_nx_driver(NX_IP_DRIVER *driver_req_ptr);

/**
 * Read the MAC address the virtio-net device was given, without touching the
 * device's state (no reset, no feature negotiation).
 *
 * The device MAC is the per-instance identity a QEMU launcher sets
 * (`-device virtio-net-device,mac=...`), so a board folds it into its RNG
 * seed: the image's configured address can be the same for every instance.
 * Safe to call before `nx_ip_create()`.
 *
 * @param mac  Output, 6 bytes.
 * @return 0 on success, -1 if no virtio-net device was found.
 */
int virtio_net_nx_read_mac(unsigned char mac[6]);

#ifdef __cplusplus
}
#endif

#endif /* VIRTIO_NET_NX_H */

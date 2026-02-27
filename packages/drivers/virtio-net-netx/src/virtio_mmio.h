/**
 * VirtIO MMIO transport (modern, version 2 only)
 *
 * Register definitions and transport functions per VirtIO 1.2 specification,
 * section 4.2 (Virtio Over MMIO).
 */

#ifndef VIRTIO_MMIO_H
#define VIRTIO_MMIO_H

#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

/* --------------------------------------------------------------------------
 * MMIO register offsets (VirtIO 1.2 spec, Table 4.1)
 * ----------------------------------------------------------------------- */
#define VIRTIO_MMIO_MAGIC_VALUE         0x000
#define VIRTIO_MMIO_VERSION             0x004
#define VIRTIO_MMIO_DEVICE_ID           0x008
#define VIRTIO_MMIO_VENDOR_ID           0x00c
#define VIRTIO_MMIO_DEVICE_FEATURES     0x010
#define VIRTIO_MMIO_DEVICE_FEATURES_SEL 0x014
#define VIRTIO_MMIO_DRIVER_FEATURES     0x020
#define VIRTIO_MMIO_DRIVER_FEATURES_SEL 0x024
#define VIRTIO_MMIO_QUEUE_SEL           0x030
#define VIRTIO_MMIO_QUEUE_NUM_MAX       0x034
#define VIRTIO_MMIO_QUEUE_NUM           0x038
#define VIRTIO_MMIO_QUEUE_READY         0x044
#define VIRTIO_MMIO_QUEUE_NOTIFY        0x050
#define VIRTIO_MMIO_INTERRUPT_STATUS    0x060
#define VIRTIO_MMIO_INTERRUPT_ACK       0x064
#define VIRTIO_MMIO_STATUS              0x070
#define VIRTIO_MMIO_QUEUE_DESC_LOW      0x080
#define VIRTIO_MMIO_QUEUE_DESC_HIGH     0x084
#define VIRTIO_MMIO_QUEUE_AVAIL_LOW     0x090
#define VIRTIO_MMIO_QUEUE_AVAIL_HIGH    0x094
#define VIRTIO_MMIO_QUEUE_USED_LOW      0x0a0
#define VIRTIO_MMIO_QUEUE_USED_HIGH     0x0a4
#define VIRTIO_MMIO_CONFIG_GENERATION   0x0fc
#define VIRTIO_MMIO_CONFIG              0x100

/* Magic value: "virt" in little-endian */
#define VIRTIO_MMIO_MAGIC               0x74726976

/* Device IDs */
#define VIRTIO_DEV_NET                  1

/* Device status bits (VirtIO 1.2 spec, section 2.1) */
#define VIRTIO_STATUS_ACKNOWLEDGE       1
#define VIRTIO_STATUS_DRIVER            2
#define VIRTIO_STATUS_FEATURES_OK       8
#define VIRTIO_STATUS_DRIVER_OK         4
#define VIRTIO_STATUS_FAILED            128

/* VirtIO net feature bits */
#define VIRTIO_NET_F_MAC                (1u << 5)
#define VIRTIO_NET_F_STATUS             (1u << 16)

/* Interrupt status bits */
#define VIRTIO_MMIO_INT_VRING           (1u << 0)
#define VIRTIO_MMIO_INT_CONFIG          (1u << 1)

/* --------------------------------------------------------------------------
 * Transport functions
 * ----------------------------------------------------------------------- */

/** Read a 32-bit MMIO register */
static inline uint32_t virtio_mmio_read32(uint64_t base, uint32_t offset)
{
    return *(volatile uint32_t *)(base + offset);
}

/** Write a 32-bit MMIO register */
static inline void virtio_mmio_write32(uint64_t base, uint32_t offset,
                                       uint32_t val)
{
    *(volatile uint32_t *)(base + offset) = val;
}

/**
 * Probe a VirtIO MMIO device.
 *
 * @param base  MMIO base address
 * @return 0 on success (magic, version, device_id all valid), -1 on failure
 */
int virtio_mmio_probe(uint64_t base);

/**
 * Negotiate device features.
 *
 * Reads device feature bits, ANDs with the driver's desired features,
 * writes the result, and checks FEATURES_OK.
 *
 * @param base             MMIO base address
 * @param driver_features  Feature bits the driver supports
 * @return negotiated features on success, 0 on failure
 */
uint32_t virtio_mmio_negotiate_features(uint64_t base,
                                        uint32_t driver_features);

/**
 * Set the device status register.
 *
 * @param base    MMIO base address
 * @param status  Status bits to OR into the register
 */
void virtio_mmio_set_status(uint64_t base, uint32_t status);

/**
 * Get the current device status.
 *
 * @param base  MMIO base address
 * @return current status register value
 */
uint32_t virtio_mmio_get_status(uint64_t base);

/**
 * Reset the device (write 0 to status).
 *
 * @param base  MMIO base address
 */
void virtio_mmio_reset(uint64_t base);

/**
 * Configure a virtqueue's addresses in the MMIO registers.
 *
 * @param base       MMIO base address
 * @param queue_idx  Queue index (0=RX, 1=TX for net device)
 * @param num        Queue size (number of descriptors)
 * @param desc_addr  Physical address of descriptor table
 * @param avail_addr Physical address of available ring
 * @param used_addr  Physical address of used ring
 * @return 0 on success, -1 if queue_num_max < num
 */
int virtio_mmio_setup_queue(uint64_t base, uint32_t queue_idx, uint32_t num,
                            uint64_t desc_addr, uint64_t avail_addr,
                            uint64_t used_addr);

#ifdef __cplusplus
}
#endif

#endif /* VIRTIO_MMIO_H */

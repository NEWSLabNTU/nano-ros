/**
 * VirtIO MMIO transport implementation (modern, version 2 only)
 *
 * Implements probe, feature negotiation, status transitions, and
 * virtqueue address configuration for VirtIO MMIO devices.
 */

#include "virtio_mmio.h"

int virtio_mmio_probe(uint64_t base)
{
    uint32_t magic, version, device_id;

    magic = virtio_mmio_read32(base, VIRTIO_MMIO_MAGIC_VALUE);
    if (magic != VIRTIO_MMIO_MAGIC) {
        return -1;
    }

    version = virtio_mmio_read32(base, VIRTIO_MMIO_VERSION);
    if (version != 2) {
        return -1;
    }

    device_id = virtio_mmio_read32(base, VIRTIO_MMIO_DEVICE_ID);
    if (device_id != VIRTIO_DEV_NET) {
        return -1;
    }

    return 0;
}

uint32_t virtio_mmio_negotiate_features(uint64_t base,
                                        uint32_t driver_features)
{
    uint32_t device_features, negotiated;

    /* Read device features (page 0 = bits 0-31) */
    virtio_mmio_write32(base, VIRTIO_MMIO_DEVICE_FEATURES_SEL, 0);
    device_features = virtio_mmio_read32(base, VIRTIO_MMIO_DEVICE_FEATURES);

    /* Intersect with driver features */
    negotiated = device_features & driver_features;

    /* Write driver features (page 0) */
    virtio_mmio_write32(base, VIRTIO_MMIO_DRIVER_FEATURES_SEL, 0);
    virtio_mmio_write32(base, VIRTIO_MMIO_DRIVER_FEATURES, negotiated);

    /* Page 1 (bits 32-63): negotiate VIRTIO_F_VERSION_1.
     * Required by VirtIO 1.x spec for modern (non-legacy) devices. */
    virtio_mmio_write32(base, VIRTIO_MMIO_DEVICE_FEATURES_SEL, 1);
    uint32_t dev_feat1 = virtio_mmio_read32(base, VIRTIO_MMIO_DEVICE_FEATURES);
    uint32_t drv_feat1 = dev_feat1 & VIRTIO_F_VERSION_1;
    virtio_mmio_write32(base, VIRTIO_MMIO_DRIVER_FEATURES_SEL, 1);
    virtio_mmio_write32(base, VIRTIO_MMIO_DRIVER_FEATURES, drv_feat1);

    /* Set FEATURES_OK */
    uint32_t status = virtio_mmio_read32(base, VIRTIO_MMIO_STATUS);
    status |= VIRTIO_STATUS_FEATURES_OK;
    virtio_mmio_write32(base, VIRTIO_MMIO_STATUS, status);

    /* Re-read to confirm device accepted our features */
    status = virtio_mmio_read32(base, VIRTIO_MMIO_STATUS);
    if (!(status & VIRTIO_STATUS_FEATURES_OK)) {
        return 0;
    }

    return negotiated;
}

void virtio_mmio_set_status(uint64_t base, uint32_t status)
{
    uint32_t current = virtio_mmio_read32(base, VIRTIO_MMIO_STATUS);
    virtio_mmio_write32(base, VIRTIO_MMIO_STATUS, current | status);
}

uint32_t virtio_mmio_get_status(uint64_t base)
{
    return virtio_mmio_read32(base, VIRTIO_MMIO_STATUS);
}

void virtio_mmio_reset(uint64_t base)
{
    virtio_mmio_write32(base, VIRTIO_MMIO_STATUS, 0);
}

int virtio_mmio_setup_queue(uint64_t base, uint32_t queue_idx, uint32_t num,
                            uint64_t desc_addr, uint64_t avail_addr,
                            uint64_t used_addr)
{
    /* Select queue */
    virtio_mmio_write32(base, VIRTIO_MMIO_QUEUE_SEL, queue_idx);

    /* Check max queue size */
    uint32_t max_num = virtio_mmio_read32(base, VIRTIO_MMIO_QUEUE_NUM_MAX);
    if (max_num == 0 || max_num < num) {
        return -1;
    }

    /* Set queue size */
    virtio_mmio_write32(base, VIRTIO_MMIO_QUEUE_NUM, num);

    /* Set descriptor table address */
    virtio_mmio_write32(base, VIRTIO_MMIO_QUEUE_DESC_LOW,
                        (uint32_t)(desc_addr & 0xFFFFFFFF));
    virtio_mmio_write32(base, VIRTIO_MMIO_QUEUE_DESC_HIGH,
                        (uint32_t)(desc_addr >> 32));

    /* Set available ring address */
    virtio_mmio_write32(base, VIRTIO_MMIO_QUEUE_AVAIL_LOW,
                        (uint32_t)(avail_addr & 0xFFFFFFFF));
    virtio_mmio_write32(base, VIRTIO_MMIO_QUEUE_AVAIL_HIGH,
                        (uint32_t)(avail_addr >> 32));

    /* Set used ring address */
    virtio_mmio_write32(base, VIRTIO_MMIO_QUEUE_USED_LOW,
                        (uint32_t)(used_addr & 0xFFFFFFFF));
    virtio_mmio_write32(base, VIRTIO_MMIO_QUEUE_USED_HIGH,
                        (uint32_t)(used_addr >> 32));

    /* Mark queue ready */
    virtio_mmio_write32(base, VIRTIO_MMIO_QUEUE_READY, 1);

    return 0;
}

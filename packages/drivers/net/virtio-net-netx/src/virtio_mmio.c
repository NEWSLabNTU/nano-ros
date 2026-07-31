/**
 * VirtIO MMIO transport implementation
 *
 * Supports both legacy (version 1) and modern (version 2) MMIO transport.
 * QEMU 6.x uses version 1, QEMU 7+ may use version 2.
 */

#include "virtio_mmio.h"

/* Global MMIO version detected at probe time (default v1 for safety) */
uint32_t virtio_mmio_version = 1;

/* MMIO v1 page size for PFN calculation */
#define VIRTIO_MMIO_PAGE_SIZE 4096

int virtio_mmio_probe(uint64_t base)
{
    uint32_t magic, version, device_id;

    magic = virtio_mmio_read32(base, VIRTIO_MMIO_MAGIC_VALUE);
    if (magic != VIRTIO_MMIO_MAGIC) {
        return -1;
    }

    version = virtio_mmio_read32(base, VIRTIO_MMIO_VERSION);
    if (version != 1 && version != 2) {
        return -1;
    }

    device_id = virtio_mmio_read32(base, VIRTIO_MMIO_DEVICE_ID);
    if (device_id != VIRTIO_DEV_NET) {
        return -1;
    }

    virtio_mmio_version = version;
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

    if (virtio_mmio_version == 2) {
        /* Page 1 (bits 32-63): negotiate VIRTIO_F_VERSION_1.
         * Required by VirtIO 1.x spec for modern (non-legacy) devices. */
        virtio_mmio_write32(base, VIRTIO_MMIO_DEVICE_FEATURES_SEL, 1);
        uint32_t dev_feat1 = virtio_mmio_read32(base, VIRTIO_MMIO_DEVICE_FEATURES);
        uint32_t drv_feat1 = dev_feat1 & VIRTIO_F_VERSION_1;
        virtio_mmio_write32(base, VIRTIO_MMIO_DRIVER_FEATURES_SEL, 1);
        virtio_mmio_write32(base, VIRTIO_MMIO_DRIVER_FEATURES, drv_feat1);
    }

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

    if (virtio_mmio_version == 1) {
        /* MMIO v1 (legacy): set guest page size, alignment, then PFN.
         * The device computes avail/used addresses from the PFN base.
         * desc_addr must be page-aligned. */
        extern void uart_puts(const char *s);
        extern int snprintf(char *, unsigned long, const char *, ...);
        {
            char buf[128];
            uint32_t pfn = (uint32_t)(desc_addr / VIRTIO_MMIO_PAGE_SIZE);
            snprintf(buf, sizeof(buf),
                     "[vq] q%u: desc=0x%lx avail=0x%lx used=0x%lx pfn=%u\n",
                     queue_idx, (unsigned long)desc_addr,
                     (unsigned long)avail_addr, (unsigned long)used_addr, pfn);
            uart_puts(buf);
        }
        virtio_mmio_write32(base, VIRTIO_MMIO_GUEST_PAGE_SIZE,
                            VIRTIO_MMIO_PAGE_SIZE);
        virtio_mmio_write32(base, VIRTIO_MMIO_QUEUE_ALIGN,
                            VIRTIO_MMIO_PAGE_SIZE);
        virtio_mmio_write32(base, VIRTIO_MMIO_QUEUE_PFN,
                            (uint32_t)(desc_addr / VIRTIO_MMIO_PAGE_SIZE));
    } else {
        /* MMIO v2 (modern): set each address separately */
        virtio_mmio_write32(base, VIRTIO_MMIO_QUEUE_DESC_LOW,
                            (uint32_t)(desc_addr & 0xFFFFFFFF));
        virtio_mmio_write32(base, VIRTIO_MMIO_QUEUE_DESC_HIGH,
                            (uint32_t)(desc_addr >> 32));

        virtio_mmio_write32(base, VIRTIO_MMIO_QUEUE_AVAIL_LOW,
                            (uint32_t)(avail_addr & 0xFFFFFFFF));
        virtio_mmio_write32(base, VIRTIO_MMIO_QUEUE_AVAIL_HIGH,
                            (uint32_t)(avail_addr >> 32));

        virtio_mmio_write32(base, VIRTIO_MMIO_QUEUE_USED_LOW,
                            (uint32_t)(used_addr & 0xFFFFFFFF));
        virtio_mmio_write32(base, VIRTIO_MMIO_QUEUE_USED_HIGH,
                            (uint32_t)(used_addr >> 32));

        /* Mark queue ready (v2 only) */
        virtio_mmio_write32(base, VIRTIO_MMIO_QUEUE_READY, 1);
    }

    return 0;
}

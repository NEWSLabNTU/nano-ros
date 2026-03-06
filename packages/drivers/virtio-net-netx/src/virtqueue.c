/**
 * Split virtqueue implementation
 *
 * Supports both MMIO v1 (legacy) and v2 (modern) layouts.
 * For v1, the desc/avail/used arrays must be contiguous with the used ring
 * aligned to a page boundary.  For v2, separate static arrays are fine.
 */

#include "virtqueue.h"
#include "virtio_mmio.h"
#include <string.h>

/* --------------------------------------------------------------------------
 * Static memory for two queues (RX=0, TX=1)
 *
 * For MMIO v1 (legacy), QEMU derives avail/used addresses from the PFN:
 *   desc_addr  = pfn * page_size
 *   avail_addr = desc_addr + num * sizeof(vring_desc)
 *   used_addr  = align_up(avail_addr + sizeof(vring_avail), page_size)
 *
 * For VIRTQUEUE_SIZE=64, page_size=4096:
 *   desc:  64 * 16 = 1024 bytes  (offset 0)
 *   avail: 6 + 64*2 = 134 bytes  (offset 1024)
 *   padding: 4096 - 1024 - 134 = 2938 bytes
 *   used:  6 + 64*8 = 518 bytes  (offset 4096)
 *   Total per queue: 4096 + 518 = 4614 bytes, rounded up to 8192.
 *
 * We allocate a single contiguous buffer per queue, page-aligned, that
 * works for both v1 and v2.
 * ----------------------------------------------------------------------- */

#define NUM_QUEUES   2
#define PAGE_SIZE    4096

/* Size of the contiguous vring region per queue.
 * Must be a multiple of PAGE_SIZE so that each queue element in the
 * vring_mem array is page-aligned (required for v1 PFN addressing).
 * Two pages: desc+avail in page 0, used in page 1. */
#define VRING_SIZE   (2 * PAGE_SIZE)

/* Contiguous vring memory, page-aligned */
static uint8_t vring_mem[NUM_QUEUES][VRING_SIZE]
    __attribute__((aligned(PAGE_SIZE)));

int virtqueue_init(struct virtqueue *vq, uint32_t queue_idx,
                   uint64_t mmio_base)
{
    if (queue_idx >= NUM_QUEUES) {
        return -1;
    }

    uint8_t *base = vring_mem[queue_idx];
    memset(base, 0, VRING_SIZE);

    /* Lay out arrays at standard vring offsets */
    vq->desc  = (struct virtq_desc *)base;
    /* avail ring starts right after the descriptor table */
    vq->avail = (struct virtq_avail *)(base + VIRTQUEUE_SIZE * sizeof(struct virtq_desc));
    /* used ring starts at next page boundary */
    vq->used  = (struct virtq_used *)(base + PAGE_SIZE);

    vq->num   = VIRTQUEUE_SIZE;
    vq->free_head = 0;
    vq->num_free  = VIRTQUEUE_SIZE;
    vq->last_used_idx = 0;
    vq->mmio_base  = mmio_base;
    vq->queue_idx  = queue_idx;

    /* Chain all descriptors into a free list */
    for (uint16_t i = 0; i < VIRTQUEUE_SIZE - 1; i++) {
        vq->desc[i].next = i + 1;
    }
    vq->desc[VIRTQUEUE_SIZE - 1].next = 0xFFFF;  /* End of free list */

    /* Configure MMIO queue registers */
    uint64_t desc_addr  = (uint64_t)(uintptr_t)vq->desc;
    uint64_t avail_addr = (uint64_t)(uintptr_t)vq->avail;
    uint64_t used_addr  = (uint64_t)(uintptr_t)vq->used;

    return virtio_mmio_setup_queue(mmio_base, queue_idx, VIRTQUEUE_SIZE,
                                   desc_addr, avail_addr, used_addr);
}

int virtqueue_add_buf(struct virtqueue *vq, uint64_t addr, uint32_t len,
                      uint16_t flags)
{
    if (vq->num_free == 0) {
        return -1;
    }

    /* Allocate a descriptor from the free list */
    uint16_t idx = vq->free_head;
    vq->free_head = vq->desc[idx].next;
    vq->num_free--;

    /* Fill in the descriptor */
    vq->desc[idx].addr  = addr;
    vq->desc[idx].len   = len;
    vq->desc[idx].flags = flags;
    vq->desc[idx].next  = 0;

    /* Add to the available ring */
    uint16_t avail_idx = vq->avail->idx % vq->num;
    vq->avail->ring[avail_idx] = idx;

    /* Memory barrier: ensure descriptor is visible before updating idx */
    __asm__ volatile("fence ow, ow" ::: "memory");

    vq->avail->idx++;

    return (int)idx;
}

void virtqueue_kick(struct virtqueue *vq)
{
    /* Memory barrier: ensure avail->idx is visible before notify */
    __asm__ volatile("fence ow, ow" ::: "memory");

    virtio_mmio_write32(vq->mmio_base, VIRTIO_MMIO_QUEUE_NOTIFY,
                        vq->queue_idx);
}

int virtqueue_get_used(struct virtqueue *vq, uint32_t *len)
{
    /* Memory barrier: ensure we see device's writes to used ring */
    __asm__ volatile("fence ir, ir" ::: "memory");

    if (vq->last_used_idx == vq->used->idx) {
        return -1;  /* No completed buffers */
    }

    uint16_t used_slot = vq->last_used_idx % vq->num;
    uint32_t desc_idx  = vq->used->ring[used_slot].id;

    if (len != NULL) {
        *len = vq->used->ring[used_slot].len;
    }

    vq->last_used_idx++;

    return (int)desc_idx;
}

void virtqueue_free_desc(struct virtqueue *vq, uint16_t idx)
{
    vq->desc[idx].addr  = 0;
    vq->desc[idx].len   = 0;
    vq->desc[idx].flags = 0;
    vq->desc[idx].next  = vq->free_head;
    vq->free_head = idx;
    vq->num_free++;
}

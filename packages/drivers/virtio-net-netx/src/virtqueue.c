/**
 * Split virtqueue implementation
 *
 * All memory is statically allocated from arrays sized for two queues
 * (RX + TX), each with VIRTQUEUE_SIZE descriptors.
 */

#include "virtqueue.h"
#include "virtio_mmio.h"
#include <string.h>

/* --------------------------------------------------------------------------
 * Static memory for two queues (RX=0, TX=1)
 *
 * Each queue needs:
 *   desc:  VIRTQUEUE_SIZE * 16 bytes = 1024 bytes
 *   avail: 6 + VIRTQUEUE_SIZE * 2    = 134 bytes
 *   used:  6 + VIRTQUEUE_SIZE * 8    = 518 bytes
 *
 * Total: ~1676 bytes per queue, ~3352 bytes for both.
 * Alignment: desc 16-byte, avail 2-byte, used 4-byte (spec 2.7.x).
 * ----------------------------------------------------------------------- */

#define NUM_QUEUES 2

static struct virtq_desc  queue_desc[NUM_QUEUES][VIRTQUEUE_SIZE]
    __attribute__((aligned(16)));
static struct virtq_avail queue_avail[NUM_QUEUES]
    __attribute__((aligned(2)));
static struct virtq_used  queue_used[NUM_QUEUES]
    __attribute__((aligned(4)));

int virtqueue_init(struct virtqueue *vq, uint32_t queue_idx,
                   uint64_t mmio_base)
{
    if (queue_idx >= NUM_QUEUES) {
        return -1;
    }

    /* Zero out all ring memory */
    memset(&queue_desc[queue_idx], 0, sizeof(queue_desc[queue_idx]));
    memset(&queue_avail[queue_idx], 0, sizeof(queue_avail[queue_idx]));
    memset(&queue_used[queue_idx], 0, sizeof(queue_used[queue_idx]));

    vq->desc  = queue_desc[queue_idx];
    vq->avail = &queue_avail[queue_idx];
    vq->used  = &queue_used[queue_idx];
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

/**
 * Split virtqueue management (VirtIO 1.2, section 2.7)
 *
 * Provides descriptor allocation, available/used ring management, and
 * device notification for split virtqueues. All memory is statically
 * allocated (no malloc).
 */

#ifndef VIRTQUEUE_H
#define VIRTQUEUE_H

#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

#define VIRTQUEUE_SIZE 64   /* Number of descriptors per queue */

/* Descriptor flags */
#define VIRTQ_DESC_F_NEXT     1   /* Buffer continues in next descriptor */
#define VIRTQ_DESC_F_WRITE    2   /* Device writes (vs reads) this buffer */

/* --------------------------------------------------------------------------
 * VirtIO data structures (VirtIO 1.2 spec, section 2.7)
 * ----------------------------------------------------------------------- */

struct virtq_desc {
    uint64_t addr;    /* Guest physical address */
    uint32_t len;     /* Buffer length */
    uint16_t flags;   /* NEXT, WRITE, INDIRECT */
    uint16_t next;    /* Next descriptor index (if NEXT flag set) */
};

struct virtq_avail {
    uint16_t flags;
    uint16_t idx;
    uint16_t ring[VIRTQUEUE_SIZE];
};

struct virtq_used_elem {
    uint32_t id;      /* Descriptor chain head index */
    uint32_t len;     /* Bytes written by device */
};

struct virtq_used {
    uint16_t flags;
    uint16_t idx;
    struct virtq_used_elem ring[VIRTQUEUE_SIZE];
};

/* --------------------------------------------------------------------------
 * Virtqueue state
 * ----------------------------------------------------------------------- */

struct virtqueue {
    struct virtq_desc  *desc;
    struct virtq_avail *avail;
    struct virtq_used  *used;

    uint16_t num;            /* Queue size (= VIRTQUEUE_SIZE) */
    uint16_t free_head;      /* Head of free descriptor chain */
    uint16_t num_free;       /* Number of free descriptors */
    uint16_t last_used_idx;  /* Last processed used ring index */

    uint64_t mmio_base;      /* For notify writes */
    uint32_t queue_idx;      /* 0=RX, 1=TX */
};

/* --------------------------------------------------------------------------
 * API
 * ----------------------------------------------------------------------- */

/**
 * Initialize a virtqueue and configure its MMIO registers.
 *
 * Allocates descriptor table, available ring, and used ring from static
 * buffers. Chains all descriptors into a free list.
 *
 * @param vq         Virtqueue to initialize
 * @param queue_idx  Queue index (0=RX, 1=TX)
 * @param mmio_base  VirtIO MMIO base address (for notify)
 * @return 0 on success, -1 on failure
 */
int virtqueue_init(struct virtqueue *vq, uint32_t queue_idx,
                   uint64_t mmio_base);

/**
 * Add a single buffer to the available ring.
 *
 * @param vq    Virtqueue
 * @param addr  Physical address of the buffer
 * @param len   Buffer length in bytes
 * @param flags Descriptor flags (e.g., VIRTQ_DESC_F_WRITE for RX buffers)
 * @return descriptor index on success, -1 if no free descriptors
 */
int virtqueue_add_buf(struct virtqueue *vq, uint64_t addr, uint32_t len,
                      uint16_t flags);

/**
 * Notify the device that new buffers are available.
 *
 * @param vq  Virtqueue
 */
void virtqueue_kick(struct virtqueue *vq);

/**
 * Get the next completed buffer from the used ring.
 *
 * @param vq   Virtqueue
 * @param len  Output: bytes written/consumed by device (may be NULL)
 * @return descriptor index on success, -1 if no completed buffers
 */
int virtqueue_get_used(struct virtqueue *vq, uint32_t *len);

/**
 * Return a descriptor to the free list.
 *
 * @param vq   Virtqueue
 * @param idx  Descriptor index to free
 */
void virtqueue_free_desc(struct virtqueue *vq, uint16_t idx);

#ifdef __cplusplus
}
#endif

#endif /* VIRTQUEUE_H */

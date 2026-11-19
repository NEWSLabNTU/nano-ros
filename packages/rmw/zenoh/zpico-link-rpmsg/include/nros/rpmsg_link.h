/*
 * zenoh-pico over RPMsg: the contract between nano-ros's link and a board's
 * RPMsg binding (OpenAMP `rpmsg_*`, TI IPC RPMessage, ...).
 *
 * nano-ros owns the endpoint table, the framing (one frame per RPMsg message,
 * at most NROS_RPMSG_FRAME_SIZE bytes) and the receive ring. The board owns
 * the vendor calls: it opens the endpoint the locator names and sends frames,
 * and it hands every received message to nros_rpmsg_deliver(), from its
 * receive callback or receive task.
 *
 * A locator `rpmsg/<name>` opens the endpoint `<name>`. What the name means
 * (an OpenAMP service name, a TI endpoint number written in decimal, ...) is
 * the board's to decide.
 */
#ifndef NROS_RPMSG_LINK_H
#define NROS_RPMSG_LINK_H

#include <stddef.h>
#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

/* The largest frame the link sends: the RPMsg payload with 512-byte buffers
 * (512 minus the 16-byte RPMsg header). A message longer than this is
 * refused by nros_rpmsg_deliver. */
#define NROS_RPMSG_FRAME_SIZE 496

/* The link's handle for one open endpoint. The board stores it at open and
 * passes it back to nros_rpmsg_deliver. */
typedef struct nros_rpmsg_link_ep nros_rpmsg_link_ep_t;

typedef struct {
    /* Open the endpoint named `name` (`name_len` bytes, not NUL-terminated).
     * Returns the board's endpoint handle, or NULL if it cannot be opened.
     * From here on the board delivers every message received on it with
     * nros_rpmsg_deliver(link_ep, ...). */
    void *(*open)(void *ctx, const char *name, size_t name_len, nros_rpmsg_link_ep_t *link_ep);
    /* Stop delivering for `ep` and release it. May be NULL. */
    void (*close)(void *ctx, void *ep);
    /* Send one message of `len` bytes (at most NROS_RPMSG_FRAME_SIZE). The
     * call may wait for a transmit buffer for a bounded time. Returns 0 once
     * the message is queued, non-zero if it was not sent. */
    int (*send)(void *ctx, void *ep, const uint8_t *data, size_t len);
    /* Wait until a message may have been delivered on `ep`, or `timeout_ms`
     * has passed; the return value is not read. Required: the link reads
     * from its ring, which answers at once when empty, and this is where the
     * reading thread sleeps instead of spinning. A semaphore the receive
     * callback gives after nros_rpmsg_deliver() is the usual shape; a wake
     * with nothing delivered is harmless. */
    int (*wait)(void *ctx, void *ep, uint32_t timeout_ms);
    void *ctx;
} nros_rpmsg_backend_t;

/* Install the board's binding. Call once, before the session opens; the
 * table must outlive the program. Returns 0, or non-zero if a backend is
 * already installed or `open`, `send` or `wait` is NULL. */
int nros_rpmsg_register_backend(const nros_rpmsg_backend_t *backend);

/* Hand one received message to the link. Safe from an interrupt handler or
 * any one task per endpoint (single producer). Returns 0, or non-zero if the
 * message was dropped: longer than NROS_RPMSG_FRAME_SIZE, or the receive ring
 * is full. A dropped frame loses the batch it belongs to; the link
 * resynchronises on the next one. */
int nros_rpmsg_deliver(nros_rpmsg_link_ep_t *link_ep, const uint8_t *data, size_t len);

#ifdef __cplusplus
}
#endif

#endif /* NROS_RPMSG_LINK_H */

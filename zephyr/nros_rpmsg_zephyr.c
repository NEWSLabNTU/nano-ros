/*
 * The RPMsg backend for a Zephyr image: zenoh-pico's RPMsg link over
 * Zephyr's RPMsg service (subsys/ipc/rpmsg_service, OpenAMP underneath).
 *
 * The link's board contract is include/nros/rpmsg_link.h in
 * zpico-link-rpmsg: the board opens the endpoint a locator `rpmsg/<name>`
 * names, sends frames, and hands every received message to
 * nros_rpmsg_deliver(). This file is that board half for every Zephyr board
 * whose devicetree gives the RPMsg service its IPM and shared memory.
 *
 * The RPMsg service registers its endpoints before it initialises, in a
 * POST_KERNEL SYS_INIT, so the endpoint cannot be created when the session
 * opens. It is registered here at boot under CONFIG_NROS_ZENOH_RPMSG_ENDPOINT,
 * and open() accepts that one name.
 *
 * A message can arrive before the session opens: the other core's handshake
 * if that core opened first. A socket would queue it, and zenoh-pico does not
 * resend a handshake its peer missed, so the first frames are held here and
 * handed to the link at open (NROS_RPMSG_PREOPEN_FRAMES of them; more is
 * more than one handshake, and is dropped).
 */

#include <errno.h>
#include <stdint.h>
#include <string.h>

#include <zephyr/init.h>
#include <zephyr/ipc/rpmsg_service.h>
#include <zephyr/kernel.h>

#include <nros/rpmsg_link.h>

static int nros_rpmsg_ep_id = -1;

/* The frames held until a session opens (see the top of this file). */
#define NROS_RPMSG_PREOPEN_FRAMES 4

/* The link's handle while a session has the endpoint open, else NULL, and
 * the frames that arrived while it was NULL. Both under `nros_rpmsg_lock`:
 * the receive callback (the RPMsg service's work queue) and open() each
 * deliver, and the link's ring takes one producer at a time. */
static K_MUTEX_DEFINE(nros_rpmsg_lock);
static nros_rpmsg_link_ep_t *nros_rpmsg_link_ep;
static uint8_t nros_rpmsg_held[NROS_RPMSG_PREOPEN_FRAMES][NROS_RPMSG_FRAME_SIZE];
static size_t nros_rpmsg_held_len[NROS_RPMSG_PREOPEN_FRAMES];
static size_t nros_rpmsg_held_count;
/* Given after each delivery, taken by `wait`: where zenoh's read sleeps. */
static K_SEM_DEFINE(nros_rpmsg_arrived, 0, K_SEM_MAX_LIMIT);

static int nros_rpmsg_rx(struct rpmsg_endpoint *ept, void *data, size_t len, uint32_t src,
                         void *priv)
{
    ARG_UNUSED(ept);
    ARG_UNUSED(src);
    ARG_UNUSED(priv);
    k_mutex_lock(&nros_rpmsg_lock, K_FOREVER);
    if (nros_rpmsg_link_ep != NULL) {
        /* A full ring drops the frame; the link loses that batch and
         * resynchronises on the next one. */
        (void)nros_rpmsg_deliver(nros_rpmsg_link_ep, data, len);
    } else if (nros_rpmsg_held_count < NROS_RPMSG_PREOPEN_FRAMES &&
               len <= NROS_RPMSG_FRAME_SIZE) {
        memcpy(nros_rpmsg_held[nros_rpmsg_held_count], data, len);
        nros_rpmsg_held_len[nros_rpmsg_held_count] = len;
        nros_rpmsg_held_count++;
    }
    k_mutex_unlock(&nros_rpmsg_lock);
    k_sem_give(&nros_rpmsg_arrived);
    return RPMSG_SUCCESS;
}

static void *nros_rpmsg_open(void *ctx, const char *name, size_t name_len,
                             nros_rpmsg_link_ep_t *link_ep)
{
    ARG_UNUSED(ctx);
    const char *mine = CONFIG_NROS_ZENOH_RPMSG_ENDPOINT;
    if (nros_rpmsg_ep_id < 0 || name_len != strlen(mine) || memcmp(name, mine, name_len) != 0) {
        return NULL;
    }
#if defined(CONFIG_RPMSG_SERVICE_MODE_MASTER)
    /* On the master the endpoint binds when the remote core announces it
     * through the name service. Wait for that a bounded time, so a locator
     * naming a core that never starts fails the open rather than hanging it.
     *
     * The remote side has no such signal: its endpoint learns the master's
     * address from the first message it receives, and cannot send before
     * then. So the remote core is the end that LISTENS, and the master the
     * end that connects, whose first frame is zenoh's handshake. */
    for (int ms = 0; !rpmsg_service_endpoint_is_bound(nros_rpmsg_ep_id); ms++) {
        if (ms >= CONFIG_NROS_ZENOH_RPMSG_BIND_TIMEOUT_MS) {
            return NULL;
        }
        k_msleep(1);
    }
#endif
    k_mutex_lock(&nros_rpmsg_lock, K_FOREVER);
    if (nros_rpmsg_link_ep != NULL) {
        k_mutex_unlock(&nros_rpmsg_lock);
        return NULL; /* already open */
    }
    nros_rpmsg_link_ep = link_ep;
    for (size_t i = 0; i < nros_rpmsg_held_count; i++) {
        (void)nros_rpmsg_deliver(link_ep, nros_rpmsg_held[i], nros_rpmsg_held_len[i]);
    }
    nros_rpmsg_held_count = 0;
    k_mutex_unlock(&nros_rpmsg_lock);
    /* Any non-NULL handle; there is one endpoint. */
    return &nros_rpmsg_ep_id;
}

static void nros_rpmsg_close(void *ctx, void *ep)
{
    ARG_UNUSED(ctx);
    ARG_UNUSED(ep);
    k_mutex_lock(&nros_rpmsg_lock, K_FOREVER);
    nros_rpmsg_link_ep = NULL;
    nros_rpmsg_held_count = 0;
    k_mutex_unlock(&nros_rpmsg_lock);
}

static int nros_rpmsg_send(void *ctx, void *ep, const uint8_t *data, size_t len)
{
    ARG_UNUSED(ctx);
    ARG_UNUSED(ep);
    /* rpmsg_send waits for a free transmit buffer, bounded by OpenAMP's own
     * timeout, and returns the bytes sent or a negative error. */
    int rc = rpmsg_service_send(nros_rpmsg_ep_id, data, len);
    return (rc >= 0 && (size_t)rc == len) ? 0 : -1;
}

static int nros_rpmsg_wait(void *ctx, void *ep, uint32_t timeout_ms)
{
    ARG_UNUSED(ctx);
    ARG_UNUSED(ep);
    return k_sem_take(&nros_rpmsg_arrived, K_MSEC(timeout_ms));
}

static const nros_rpmsg_backend_t nros_rpmsg_zephyr_backend = {
    .open = nros_rpmsg_open,
    .close = nros_rpmsg_close,
    .send = nros_rpmsg_send,
    .wait = nros_rpmsg_wait,
    .ctx = NULL,
};

static int nros_rpmsg_zephyr_init(void)
{
    int id = rpmsg_service_register_endpoint(CONFIG_NROS_ZENOH_RPMSG_ENDPOINT, nros_rpmsg_rx);
    if (id < 0) {
        return id;
    }
    nros_rpmsg_ep_id = id;
    return nros_rpmsg_register_backend(&nros_rpmsg_zephyr_backend) == 0 ? 0 : -EALREADY;
}

/* Before the RPMsg service initialises, which is when it creates the
 * endpoints registered so far. */
SYS_INIT(nros_rpmsg_zephyr_init, POST_KERNEL, CONFIG_RPMSG_SERVICE_EP_REG_PRIORITY);

#if defined(CONFIG_NROS_ZENOH_RPMSG_START_REMOTE)
/* The SoC's own call that releases the second core, which an application
 * on the first core otherwise makes from main(). After the RPMsg service has
 * set up the shared memory the remote attaches to. */
extern void wakeup_cpu1(void);

static int nros_rpmsg_start_remote(void)
{
    wakeup_cpu1();
    return 0;
}

SYS_INIT(nros_rpmsg_start_remote, APPLICATION, 0);
#endif

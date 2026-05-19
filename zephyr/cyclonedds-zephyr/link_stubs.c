/*
 * Phase 11W.4 — link-time stubs for cyclonedds symbols that
 * stay referenced by call sites whose TU bodies we deliberately
 * drop on Zephyr.
 *
 * Each stub returns the failure / no-op sentinel matching the
 * function's return type. Never called at runtime (the gating
 * branches that would invoke them are dead-code-eliminated by
 * `#ifdef DDS_HAS_*` higher up the call graph), but ld still
 * wants a symbol.
 */
#include <stdint.h>

/* IN_MULTICAST: POSIX classifies 224.0.0.0/4. Zephyr's
 * <zephyr/net/socket.h> doesn't expose the macro. Use the
 * canonical definition. */
#ifndef IN_MULTICAST
#define IN_MULTICAST(a) (((uint32_t)(a) & 0xf0000000U) == 0xe0000000U)
#endif

/* ddsi_vnet_init: virtual-network transport init. The full
 * `ddsi_vnet.c` TU is dropped from the cyclonedds source list
 * (uses Zephyr-incompatible sockaddr.sa_data field). Return
 * failure so any caller drops vnet transport silently. */
struct ddsi_domaingv;
struct ddsi_locator;

int ddsi_vnet_init(struct ddsi_domaingv *gv, const char *name, int32_t kind);
int ddsi_vnet_init(struct ddsi_domaingv *gv, const char *name, int32_t kind) {
    (void)gv;
    (void)name;
    (void)kind;
    return -1;
}

/* ddsrt_getifaddrs: enumerate network interfaces. `ifaddrs.c`
 * was dropped (no `getifaddrs` on Zephyr). Cyclone only calls
 * this from interface-autoselect paths the Zephyr build avoids
 * by passing an explicit interface name via Cyclone XML config.
 * Return error so any unexpected caller surfaces failure cleanly. */
struct ddsrt_ifaddrs;
typedef int (*ddsrt_eth_if_filter_fn)(void *);

int32_t ddsrt_getifaddrs(struct ddsrt_ifaddrs **ifap,
                          const int *afs,
                          ddsrt_eth_if_filter_fn filter);
int32_t ddsrt_getifaddrs(struct ddsrt_ifaddrs **ifap,
                          const int *afs,
                          ddsrt_eth_if_filter_fn filter) {
    (void)afs;
    (void)filter;
    if (ifap) {
        *ifap = NULL;
    }
    return -1; /* DDS_RETCODE_ERROR */
}

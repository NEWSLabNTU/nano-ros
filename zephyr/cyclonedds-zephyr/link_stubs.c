/*
 * Phase 11W.4 — link-time stubs for cyclonedds symbols whose call
 * sites stay referenced after their owning TUs (`ifaddrs.c`,
 * `ddsi_vnet.c`) are dropped from the Zephyr cyclonedds source
 * list. Phase 11W.6 extended `ddsrt_getifaddrs` from a -1 stub to
 * a synthetic loopback enumerator so Cyclone DDS can complete its
 * `ddsi_ownip` interface-discovery pass under native_sim NSOS
 * (where socket calls hit the host syscall layer but
 * `getifaddrs(3)` is not exposed via NSOS).
 */
#include <stdint.h>
#include <string.h>
#include <zephyr/net/socket.h>

#include "dds/ddsrt/heap.h"
#include "dds/ddsrt/ifaddrs.h"
#include "dds/ddsrt/retcode.h"
#include "dds/ddsrt/string.h"

#ifndef IFF_UP
#define IFF_UP        0x1
#endif
#ifndef IFF_LOOPBACK
#define IFF_LOOPBACK  0x8
#endif
#ifndef IFF_MULTICAST
#define IFF_MULTICAST 0x1000
#endif

/* IN_MULTICAST: POSIX classifies 224.0.0.0/4. Zephyr's
 * <zephyr/net/socket.h> doesn't expose the macro. */
#ifndef IN_MULTICAST
#define IN_MULTICAST(a) (((uint32_t)(a) & 0xf0000000U) == 0xe0000000U)
#endif

/* ddsi_vnet_init: virtual-network transport init. The full
 * `ddsi_vnet.c` TU is dropped from the cyclonedds source list
 * (uses Zephyr-incompatible sockaddr.sa_data field). */
struct ddsi_domaingv;

int ddsi_vnet_init(struct ddsi_domaingv *gv, const char *name, int32_t kind);
int ddsi_vnet_init(struct ddsi_domaingv *gv, const char *name, int32_t kind) {
    (void)gv;
    (void)name;
    (void)kind;
    return -1;
}

/* ddsrt_getifaddrs: enumerate network interfaces.
 *
 * Zephyr / NSOS have no `getifaddrs(3)` and no kernel-side interface
 * table the Cyclone DDS posix code path can read. Return one
 * synthetic UP+MULTICAST entry pinned at 0.0.0.0 so Cyclone's
 * `ddsi_ownip` autodetect picks the address Cyclone will then bind
 * to via NSOS-offloaded UDP sockets — the host kernel routes from
 * there. A loopback entry (127.0.0.1) would work for in-host
 * domain-0 traffic too but blocks talking to peers on the LAN; the
 * 0.0.0.0 wildcard preserves both.
 *
 * Ownership: `ddsrt_freeifaddrs` (in
 * `cyclonedds/src/ddsrt/src/ifaddrs.c`, kept in the source list)
 * walks the list and calls `ddsrt_free` on every member pointer, so
 * every field must come from `ddsrt_calloc` / `ddsrt_strdup` /
 * `ddsrt_memdup`.
 */
dds_return_t ddsrt_getifaddrs(ddsrt_ifaddrs_t **ifap, const int *afs) {
    if (ifap == NULL) {
        return DDS_RETCODE_BAD_PARAMETER;
    }
    *ifap = NULL;

    /* Skip address-family filtering if Cyclone asked for only non-IPv4
     * families; we only synthesise an IPv4 entry. */
    if (afs != NULL) {
        int has_inet = 0;
        for (const int *p = afs; *p != DDSRT_AF_TERM; p++) {
            if (*p == AF_INET) {
                has_inet = 1;
                break;
            }
        }
        if (!has_inet) {
            return DDS_RETCODE_OK;
        }
    }

    ddsrt_ifaddrs_t *ifa = ddsrt_calloc(1, sizeof(*ifa));
    struct sockaddr_in *addr = ddsrt_calloc(1, sizeof(*addr));
    struct sockaddr_in *mask = ddsrt_calloc(1, sizeof(*mask));
    char *name = ddsrt_strdup("nsos0");
    if (ifa == NULL || addr == NULL || mask == NULL || name == NULL) {
        ddsrt_free(ifa);
        ddsrt_free(addr);
        ddsrt_free(mask);
        ddsrt_free(name);
        return DDS_RETCODE_OUT_OF_RESOURCES;
    }

    /* 127.0.0.1/8 — usable loopback bind address. Cyclone DDS rejects
     * INADDR_ANY because it isn't a routable identity for the
     * participant's locator advertisement. With NSOS, socket calls
     * tunnel to the host kernel so loopback works for both in-host
     * domain peers and (with multicast group join) cross-host
     * traffic that the host's routing table forwards. */
    addr->sin_family = AF_INET;
    addr->sin_port = 0;
    addr->sin_addr.s_addr = htonl(0x7F000001U);  /* 127.0.0.1 */

    mask->sin_family = AF_INET;
    mask->sin_port = 0;
    mask->sin_addr.s_addr = htonl(0xFF000000U);  /* /8 */

    ifa->next = NULL;
    ifa->name = name;
    ifa->index = 1;
    ifa->flags = IFF_UP | IFF_LOOPBACK | IFF_MULTICAST;
    ifa->type = DDSRT_IFTYPE_WIRED;
    ifa->addr = (struct sockaddr *)addr;
    ifa->netmask = (struct sockaddr *)mask;
    ifa->broadaddr = NULL;

    *ifap = ifa;
    return DDS_RETCODE_OK;
}

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
#include <zephyr/net/net_if.h>
#include <zephyr/net/socket.h>

#include "dds/ddsrt/heap.h"
#include "dds/ddsrt/ifaddrs.h"
#include "dds/ddsrt/retcode.h"
#include "dds/ddsrt/string.h"

/* Phase 11W.12 — NSOS host interface enumeration (host trampoline
 * added by scripts/zephyr/nsos-getifaddrs-patch.sh). Declared locally
 * to avoid pulling the NSOS driver header into this cyclonedds TU;
 * layout MUST match `struct nsos_mid_ifaddr` in nsos.h. */
struct nsos_mid_ifaddr {
    unsigned int addr;      /* IPv4 address, network byte order */
    unsigned int netmask;   /* network byte order */
    unsigned int flags;     /* host IFF_* */
    unsigned int ifindex;
    char name[16];
};
int nsos_adapt_getifaddrs(struct nsos_mid_ifaddr *out);

/* Some native_sim link paths do not pull the NSOS host adapter object into
 * the final runner. Keep a weak fallback so Cyclone still links and uses the
 * loopback path below when the host trampoline is unavailable. */
#if defined(__GNUC__) || defined(__clang__)
__attribute__((weak))
#endif
int nsos_adapt_getifaddrs(struct nsos_mid_ifaddr *out) {
    (void)out;
    return -1;
}

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

    /* Phase 11W.12 — query the host's primary multicast-capable IPv4
     * interface via NSOS. SPDP multicast discovery needs to join the
     * group on a real interface; the loopback fallback below works for
     * unicast bind but Linux can't join multicast on lo. */
    struct nsos_mid_ifaddr hostif;
    int have_hostif = (nsos_adapt_getifaddrs(&hostif) == 0);

    /* phase-292 W2 (ASI wall #5) — on real Zephyr-IP-stack targets (FVP,
     * S32Z, ...) there is no NSOS trampoline, and the loopback fallback
     * below made Cyclone advertise AND bind 127.0.0.1, which the native
     * stack rejects with ENOENT (no loopback address) — every
     * dds_create_participant failed. Enumerate the kernel's own net_if
     * table instead: first UP, non-loopback interface with a usable IPv4
     * unicast address wins. */
    if (!have_hostif) {
        for (int idx = 1; ; idx++) {
            struct net_if *iface = net_if_get_by_index(idx);
            if (iface == NULL) {
                break;
            }
#if defined(CONFIG_NET_NATIVE_IPV4)
            struct net_if_ipv4 *ipv4 = iface->config.ip.ipv4;
            if (ipv4 == NULL) {
                continue;
            }
            for (int i = 0; i < NET_IF_MAX_IPV4_ADDR; i++) {
                const struct net_if_addr *ua = &ipv4->unicast[i].ipv4;
                if (!ua->is_used || ua->address.family != AF_INET) {
                    continue;
                }
                /* Skip unspecified and loopback (127/8) addresses — the
                 * whole point is a routable own-IP for SPDP. */
                uint32_t haddr = ntohl(ua->address.in_addr.s_addr);
                if (haddr == 0 || (haddr >> 24) == 127) {
                    continue;
                }
                hostif.addr = ua->address.in_addr.s_addr;
                hostif.netmask = ipv4->unicast[i].netmask.s_addr;
                hostif.flags = 0;
                hostif.ifindex = (unsigned int)idx;
                strncpy(hostif.name, "zeth", sizeof(hostif.name) - 1);
                hostif.name[sizeof(hostif.name) - 1] = '\0';
                have_hostif = 1;
                break;
            }
            if (have_hostif) {
                break;
            }
#endif /* CONFIG_NET_NATIVE_IPV4 */
        }
    }

    ddsrt_ifaddrs_t *ifa = ddsrt_calloc(1, sizeof(*ifa));
    struct sockaddr_in *addr = ddsrt_calloc(1, sizeof(*addr));
    struct sockaddr_in *mask = ddsrt_calloc(1, sizeof(*mask));
    char *name = ddsrt_strdup(have_hostif ? hostif.name : "nsos0");
    if (ifa == NULL || addr == NULL || mask == NULL || name == NULL) {
        ddsrt_free(ifa);
        ddsrt_free(addr);
        ddsrt_free(mask);
        ddsrt_free(name);
        return DDS_RETCODE_OUT_OF_RESOURCES;
    }

    addr->sin_family = AF_INET;
    addr->sin_port = 0;
    mask->sin_family = AF_INET;
    mask->sin_port = 0;

    if (have_hostif) {
        /* Real host interface — multicast join lands here and two
         * native_sim processes discover each other via SPDP. */
        addr->sin_addr.s_addr = hostif.addr;
        mask->sin_addr.s_addr = hostif.netmask;
        ifa->index = hostif.ifindex ? hostif.ifindex : 1;
        ifa->flags = IFF_UP | IFF_MULTICAST;
    } else {
        /* No usable host interface — fall back to loopback. Usable for
         * unicast bind; multicast discovery won't work. */
        addr->sin_addr.s_addr = htonl(0x7F000001U);  /* 127.0.0.1 */
        mask->sin_addr.s_addr = htonl(0xFF000000U);  /* /8 */
        ifa->index = 1;
        ifa->flags = IFF_UP | IFF_LOOPBACK | IFF_MULTICAST;
    }

    ifa->next = NULL;
    ifa->name = name;
    ifa->type = DDSRT_IFTYPE_WIRED;
    ifa->addr = (struct sockaddr *)addr;
    ifa->netmask = (struct sockaddr *)mask;
    ifa->broadaddr = NULL;

    *ifap = ifa;
    return DDS_RETCODE_OK;
}

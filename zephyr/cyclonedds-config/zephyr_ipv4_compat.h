/*
 * Copyright (c) 2026, NEWSLab NTU.
 * SPDX-License-Identifier: EPL-2.0 OR BSD-3-Clause
 *
 * Force-included via `-include` on every Cyclone DDS TU. Provides
 * the POSIX IPv4 multicast structs that Zephyr ≤3.5's BSD socket
 * layer doesn't expose. Cyclone's `ddsi_udp.c` references
 * `struct ip_mreq` directly; the actual `setsockopt(IP_ADD_MEMBERSHIP)`
 * calls will return -ENOPROTOOPT under Zephyr, but the struct must
 * exist for the TU to compile.
 *
 * Real multicast join uses `net_ipv4_igmp_join()` in
 * nros-platform-zephyr's net.c — Cyclone's setsockopt path is a
 * best-effort no-op on this target.
 */
#ifndef NROS_ZEPHYR_IPV4_COMPAT_H
#define NROS_ZEPHYR_IPV4_COMPAT_H

#ifdef __ZEPHYR__

#include <zephyr/net/net_ip.h>  /* struct in_addr */
/* Zephyr ≥3.7 defines `struct ip_mreqn` in <zephyr/net/socket.h>.
 * Pull it in so our compat shim doesn't redefine it (Phase 117 +
 * 168.X.fvp). `struct ip_mreq` (no trailing n) is still missing —
 * Zephyr only ships the newer form — so we keep that one below. */
#include <zephyr/net/socket.h>  /* struct ip_mreqn (Zephyr ≥3.7) */

#ifndef NROS_HAVE_STRUCT_IP_MREQ
#define NROS_HAVE_STRUCT_IP_MREQ 1
struct ip_mreq {
    struct in_addr imr_multiaddr;
    struct in_addr imr_interface;
};
#endif  /* NROS_HAVE_STRUCT_IP_MREQ */

#endif  /* __ZEPHYR__ */

#endif  /* NROS_ZEPHYR_IPV4_COMPAT_H */

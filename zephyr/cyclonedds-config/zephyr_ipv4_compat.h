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
/* Zephyr ≥3.7's own BSD layer defines `struct ip_mreqn` in
 * <zephyr/net/socket.h>. Pull it in so our compat shim doesn't redefine it
 * (Phase 117 + 168.X.fvp). `struct ip_mreq` (no trailing n) is still missing
 * from Zephyr's own headers — it ships only the newer form — so we keep that
 * one below. NOTE (Phase 190.B): on CONFIG_NEWLIB_LIBC targets newlib's
 * <netinet/in.h> ships the reverse — `struct ip_mreq` but not `ip_mreqn`; the
 * net.c multicast path picks the right struct by libc. This Cyclone-TU header
 * is independent (Cyclone's ddsi_udp.c uses `ip_mreq`, defined below). */
#include <zephyr/net/socket.h>  /* struct ip_mreqn (Zephyr's own BSD layer) */

/* Phase 180.A — Zephyr 4.x's <zephyr/net/net_compat.h> provides
 * `#define ip_mreq net_ip_mreq` (struct net_ip_mreq lives in net_ip.h).
 * Defining our own `struct ip_mreq` there macro-expands to a redefinition
 * of struct net_ip_mreq. So only define it when Zephyr does NOT already
 * provide ip_mreq (3.7 / pre-net_compat); the `ip_mreq` macro is the
 * version-agnostic feature detector. */
#if !defined(NROS_HAVE_STRUCT_IP_MREQ) && !defined(ip_mreq)
#define NROS_HAVE_STRUCT_IP_MREQ 1
struct ip_mreq {
    struct in_addr imr_multiaddr;
    struct in_addr imr_interface;
};
#endif  /* NROS_HAVE_STRUCT_IP_MREQ */

/* Phase 11W.4 — POSIX IN_MULTICAST macro. Zephyr's net/socket.h
 * doesn't expose it; Cyclone's ddsi_udp.c uses it to classify
 * 224.0.0.0/4 multicast addresses. */
#ifndef IN_MULTICAST
#include <stdint.h>
#define IN_MULTICAST(a) (((uint32_t)(a) & 0xf0000000U) == 0xe0000000U)
#endif

/* Phase 11W.3 — declare nothrow `operator new` overloads for
 * Cyclone DDS C++ TUs. Zephyr's `lib/cpp/minimal/include/new`
 * declares `std::nothrow_t` + `std::nothrow` but not the
 * matching ops; cyclonedds source uses `new (std::nothrow) T{}`
 * heavily. Definitions live in
 * `zephyr/cyclonedds-zephyr/nothrow_new.cpp`. C++ TUs only —
 * guarded so the C-side TUs don't see C++ syntax. */
#ifdef __cplusplus
#include <new>
#include <stddef.h>
void* operator new  (size_t, const std::nothrow_t&) noexcept;
void* operator new[](size_t, const std::nothrow_t&) noexcept;
void  operator delete  (void*, const std::nothrow_t&) noexcept;
void  operator delete[](void*, const std::nothrow_t&) noexcept;
#endif

#endif  /* __ZEPHYR__ */

#endif  /* NROS_ZEPHYR_IPV4_COMPAT_H */

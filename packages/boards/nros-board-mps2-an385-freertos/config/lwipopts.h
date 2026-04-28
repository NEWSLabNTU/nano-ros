/*
 * lwIP options for QEMU MPS2-AN385 with FreeRTOS
 *
 * Threaded mode (NO_SYS=0) with BSD socket API for zenoh-pico.
 * Tuned for moderate RAM usage (~32 KB lwIP heap) on 4 MB QEMU SRAM.
 */

#ifndef LWIPOPTS_H
#define LWIPOPTS_H

/* ---- Compatibility with newlib ---- */
/* Use newlib's struct timeval / fd_set instead of lwIP's private copies.
 * Without this, zenoh-pico (which includes <stdio.h> via newlib) gets a
 * redefinition error for struct timeval. */
#define LWIP_TIMEVAL_PRIVATE            0
#define LWIP_FD_SET_PRIVATE             0

/* ---- OS integration ---- */
#define NO_SYS                          0
#define LWIP_SOCKET                     1
#define LWIP_NETCONN                    1
#define LWIP_COMPAT_SOCKETS             1
#define LWIP_POSIX_SOCKETS_IO_NAMES     1

/* Disable core locking — netif setup runs in the app task after tcpip_init
 * completes, and zenoh-pico uses the socket API (which is thread-safe).
 * Without this, netif_add/netif_set_up assert on an uninitialized mutex
 * because they're called outside the tcpip_thread. */
#define LWIP_TCPIP_CORE_LOCKING         0

/* ---- Core protocols ---- */
#define LWIP_TCP                        1
#define LWIP_UDP                        1
#define LWIP_ICMP                       1
#define LWIP_ARP                        1
#define LWIP_ETHERNET                   1
#define LWIP_IPV4                       1
#define LWIP_IPV6                       0
#define LWIP_DHCP                       0
#define LWIP_DNS                        1
/* Phase 97.1.kconfig.freertos — IGMP for RTPS SPDP multicast
 * (239.255.0.1:7400+). Always-on cost is ~600 bytes of code +
 * ~64 bytes of state on a unicast-only system; cheap enough to
 * leave on for every RMW backend. */
#define LWIP_IGMP                       1
#define LWIP_RAW                        0
#define LWIP_BROADCAST                  1
/* RTPS DATA_FRAG submessages can fragment large samples; without
 * IP_REASSEMBLY the receiver drops every fragment past the first. */
#define IP_REASSEMBLY                   1

/* ---- Memory ----
 * Phase 97.1.kconfig.freertos bumped MEMP_NUM_NETBUF from 8 to 32 +
 * enabled IGMP / BROADCAST / IP_REASSEMBLY for DDS multicast. The
 * combined pool footprint exceeded the original 16 KiB lwIP heap on
 * the Zenoh path — `Executor::open` failed `Transport(ConnectionFailed)`
 * during the connect handshake because lwIP couldn't allocate a
 * netbuf for the outbound TCP SYN. Double the heap to 32 KiB; QEMU
 * MPS2-AN385 has 4 MiB of SRAM so the cost is irrelevant. */
#define MEM_SIZE                        (32 * 1024)
#define MEM_ALIGNMENT                   4
#define MEMP_NUM_PBUF                   32
#define MEMP_NUM_UDP_PCB                8
#define MEMP_NUM_TCP_PCB                8
#define MEMP_NUM_TCP_PCB_LISTEN         4
#define MEMP_NUM_TCP_SEG                32
/* Phase 97.1.kconfig.freertos — bumped from 8 to 32 so dust-dds's
 * SPDP / SEDP discovery burst doesn't exhaust the pool on
 * participant open. Same rationale as the Cortex-A9 net_pkt bump
 * for Zephyr (Phase 71.29). */
#define MEMP_NUM_NETBUF                 32
#define MEMP_NUM_NETCONN                8
#define MEMP_NUM_SYS_TIMEOUT            16

/* ---- Pbuf pool ---- */
#define PBUF_POOL_SIZE                  24
#define PBUF_POOL_BUFSIZE               LWIP_MEM_ALIGN_SIZE(TCP_MSS + 40 + PBUF_LINK_ENCAPSULATION_HLEN + PBUF_LINK_HLEN)

/* ---- TCP tuning ---- */
#define TCP_MSS                         1460
#define TCP_SND_BUF                     (4 * TCP_MSS)
#define TCP_SND_QUEUELEN                ((4 * TCP_SND_BUF) / TCP_MSS)
#define TCP_WND                         (4 * TCP_MSS)
#define LWIP_TCP_KEEPALIVE              1
#define LWIP_SO_RCVTIMEO                1
#define LWIP_SO_SNDTIMEO                1
#define LWIP_SO_LINGER                  1
#define SO_REUSE                        1

/* ---- Netif ---- */
#define LWIP_NETIF_STATUS_CALLBACK      1
#define LWIP_NETIF_LINK_CALLBACK        1
#define LWIP_SINGLE_NETIF              1
#define LWIP_NETIF_API                  1

/* ---- Threading (FreeRTOS) ---- */
#define LWIP_NETCONN_SEM_PER_THREAD     1
#define TCPIP_THREAD_STACKSIZE          (4 * 1024)
#define TCPIP_THREAD_PRIO               4
#define TCPIP_MBOX_SIZE                 16
#define DEFAULT_THREAD_STACKSIZE        (2 * 1024)
#define DEFAULT_RAW_RECVMBOX_SIZE       8
#define DEFAULT_UDP_RECVMBOX_SIZE       8
#define DEFAULT_TCP_RECVMBOX_SIZE       8
#define DEFAULT_ACCEPTMBOX_SIZE         4

/* ---- Checksum ---- */
#define CHECKSUM_GEN_IP                 1
#define CHECKSUM_GEN_UDP                1
#define CHECKSUM_GEN_TCP                1
#define CHECKSUM_CHECK_IP               1
#define CHECKSUM_CHECK_UDP              1
#define CHECKSUM_CHECK_TCP              1

/* ---- Debug (off by default, enable selectively) ---- */
#define LWIP_DEBUG                      0

/* ---- Stats ---- */
#define LWIP_STATS                      0

#endif /* LWIPOPTS_H */

/*
 * lwIP options for QEMU MPS2-AN385 with FreeRTOS
 *
 * Threaded mode (NO_SYS=0) with BSD socket API for zenoh-pico.
 * Tuned for moderate RAM usage (~32 KB lwIP heap) on 4 MB QEMU SRAM.
 */

#ifndef LWIPOPTS_H
#define LWIPOPTS_H

/* ---- OS integration ---- */
#define NO_SYS                          0
#define LWIP_SOCKET                     1
#define LWIP_NETCONN                    1
#define LWIP_COMPAT_SOCKETS             1
#define LWIP_POSIX_SOCKETS_IO_NAMES     1

/* ---- Core protocols ---- */
#define LWIP_TCP                        1
#define LWIP_UDP                        1
#define LWIP_ICMP                       1
#define LWIP_ARP                        1
#define LWIP_ETHERNET                   1
#define LWIP_IPV4                       1
#define LWIP_IPV6                       0
#define LWIP_DHCP                       0
#define LWIP_DNS                        0
#define LWIP_IGMP                       0
#define LWIP_RAW                        0

/* ---- Memory ---- */
#define MEM_SIZE                        (16 * 1024)
#define MEM_ALIGNMENT                   4
#define MEMP_NUM_PBUF                   32
#define MEMP_NUM_UDP_PCB                8
#define MEMP_NUM_TCP_PCB                8
#define MEMP_NUM_TCP_PCB_LISTEN         4
#define MEMP_NUM_TCP_SEG                32
#define MEMP_NUM_NETBUF                 8
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
#define SO_REUSE                        1

/* ---- Netif ---- */
#define LWIP_NETIF_STATUS_CALLBACK      1
#define LWIP_NETIF_LINK_CALLBACK        1
#define LWIP_SINGLE_NETIF              1

/* ---- Threading (FreeRTOS) ---- */
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

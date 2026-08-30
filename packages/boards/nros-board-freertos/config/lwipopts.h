/*
 * Shared lwIP options for nano-ros FreeRTOS boards.
 *
 * phase-337 W5.a — measured ZERO board-specific settings in this file, so it
 * moved here from `nros-board-mps2-an385-freertos/config/`. A board that needs
 * a different tuning `#define`s its overrides BEFORE including this file (every
 * knob below is `#ifndef`-free by lwIP convention, so an override needs an
 * `#undef`) or, per RFC-0064 ladder rung 3, owns its own copy.
 *
 * Threaded mode (NO_SYS=0) with BSD socket API for zenoh-pico.
 * Tuned for moderate RAM usage (~32 KB lwIP heap); the reference board
 * (MPS2-AN385) has 4 MiB of SRAM.
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
#ifndef MEM_SIZE
#define MEM_SIZE                        (32 * 1024)
#endif
#define MEM_ALIGNMENT                   4
#ifndef MEMP_NUM_PBUF
#define MEMP_NUM_PBUF                   32
#endif
#define MEMP_NUM_UDP_PCB                8
#define MEMP_NUM_TCP_PCB                8
#define MEMP_NUM_TCP_PCB_LISTEN         4
#define MEMP_NUM_TCP_SEG                32
/* Phase 97.1.kconfig.freertos — bumped from 8 to 32 so DDS's
 * SPDP / SEDP discovery burst doesn't exhaust the pool on
 * participant open. Same rationale as the Cortex-A9 net_pkt bump
 * for Zephyr (Phase 71.29). */
#ifndef MEMP_NUM_NETBUF
#define MEMP_NUM_NETBUF                 32
#endif
#define MEMP_NUM_NETCONN                8
#define MEMP_NUM_SYS_TIMEOUT            16
/* Phase 97.4.freertos — DDS create_endpoint() allocates an addrinfo
 * per RTPS port (3 binds + send sockets per participant + every
 * write_message() destination). Default MEMP_NUM_NETDB=1 exhausts on
 * the first SPDP send. */
#define MEMP_NUM_NETDB                  16

/* ---- Pbuf pool ---- */
#ifndef PBUF_POOL_SIZE
#define PBUF_POOL_SIZE                  24
#endif
#define PBUF_POOL_BUFSIZE               LWIP_MEM_ALIGN_SIZE(TCP_MSS + 40 + PBUF_LINK_ENCAPSULATION_HLEN + PBUF_LINK_HLEN)

/* ---- TCP tuning ---- */
#define TCP_MSS                         1460
/* nano-ros issue 0269: 4*MSS starved many-entity DECLARE bursts —
 * TCP_SND_QUEUELEN (derived below) capped in-flight small segments at
 * ~16 and forced the send path into its retry loop during session
 * open. 8*MSS doubles both byte- and segment-headroom; MEM_SIZE (32 KB)
 * comfortably covers the extra unacked segments. */
#define TCP_SND_BUF                     (8 * TCP_MSS)
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
/* Cyclone DDS's lwIP ddsrt port enumerates interfaces through netif_list.
 * Keep the linked-list netif model even though the QEMU board has one NIC. */
#define LWIP_SINGLE_NETIF              0
#define LWIP_NETIF_API                  1

/* ---- Threading (FreeRTOS) ---- */
/* nano-ros issue 0906 — these two are ONE setting; lwIP says so.
 *
 *   "LWIP_NETCONN_FULLDUPLEX==1: Enable code that allows reading from one
 *    thread, writing from a 2nd thread and closing from a 3rd thread at the
 *    same time. LWIP_NETCONN_SEM_PER_THREAD==1 is required to use one
 *    socket/netconn from multiple threads at once!"   (lwip/opt.h)
 *
 * That is precisely our shape: zenoh-pico's read task calls recv, the app task
 * publishes, and the lease task sends keepalives and CLOSES the socket during a
 * reconnect. The board previously set SEM_PER_THREAD alone and left FULLDUPLEX
 * at its default of 0 — half a requirement, which is not a working
 * configuration for this usage.
 *
 * The failure was not subtle once it was measured: a lease teardown entered
 * `_z_link_free`, lwIP's close waited for an operation that could never
 * complete, and the lease task parked there forever. The session stayed closed
 * (`_tp._type == _Z_TRANSPORT_NONE`), so every later publish returned
 * `NROS_RET_PUBLISH_FAILED` and the image went quiet after ~19 messages while
 * still printing "Publishing" for each one.
 *
 * SEM_PER_THREAD also has a requirement of its own that is easy to miss: the
 * FreeRTOS port's `sys_arch_netconn_sem_get()` only READS the thread-local
 * slot, never allocates. A task that has not called `lwip_socket_thread_init()`
 * hands lwIP a NULL semaphore. Only the app task called it (in
 * `nros_freertos_init_network`); zenoh-pico's tasks now do too, from
 * `z_task_wrapper` in the vendored fork. Measured before that fix:
 * `sys_arch_netconn_sem_get()` returned NULL for `zpico_read` and for
 * `zpico_lease`.
 *
 * So all three move together — FULLDUPLEX, SEM_PER_THREAD, and a
 * `lwip_socket_thread_init()` in every socket-using task. Changing one alone
 * gets you a different hang, not a fix. */
#define LWIP_NETCONN_FULLDUPLEX         1
#define LWIP_NETCONN_SEM_PER_THREAD     1
#define TCPIP_THREAD_STACKSIZE          (4 * 1024)
#define TCPIP_THREAD_PRIO               4
#ifndef TCPIP_MBOX_SIZE
#define TCPIP_MBOX_SIZE                 16
#endif
/* The tcpip mailbox holds POINTERS to `tcpip_msg` structures drawn from
 * MEMP_TCPIP_MSG_INPKT, so that pool — not the mailbox — is the real
 * driver->stack queue depth. A board that raises TCPIP_MBOX_SIZE and leaves
 * the pool at lwIP's default of 8 gets a queue of 8 that looks like its
 * mailbox size: past 8, `tcpip_input()` returns ERR_MEM and the frame is
 * dropped before it reaches IP. Tying them here makes raising one raise both.
 *
 * Issue 0836: the an536 board raised TCPIP_MBOX_SIZE to 64 and this stayed at
 * 8, so a real ROS 2 peer's discovery burst (which peaks at 14-16 here) lost
 * frames carrying SEDP — and a reliable builtin reader that loses a sample
 * waits for it forever, so every topic announced after the gap never matched. */
#ifndef MEMP_NUM_TCPIP_MSG_INPKT
#define MEMP_NUM_TCPIP_MSG_INPKT        TCPIP_MBOX_SIZE
#endif
#define DEFAULT_THREAD_STACKSIZE        (2 * 1024)
#ifndef DEFAULT_RAW_RECVMBOX_SIZE
#define DEFAULT_RAW_RECVMBOX_SIZE       8
#endif
#ifndef DEFAULT_UDP_RECVMBOX_SIZE
#define DEFAULT_UDP_RECVMBOX_SIZE       8
#endif
#ifndef DEFAULT_TCP_RECVMBOX_SIZE
#define DEFAULT_TCP_RECVMBOX_SIZE       8
#endif
#define DEFAULT_ACCEPTMBOX_SIZE         4

/* ---- Per-board sizing overrides ----------------------------------------
 * The values above are sized for the smallest board in the family. A board
 * carrying real ROS traffic needs more, and can say so with -D rather than
 * by editing this file: an Autoware trajectory is ~13 KiB, which RTPS sends
 * as ~10 back-to-back datagrams, and a receive mbox of 8 silently drops the
 * tail of every one of them — the sample then never reassembles, so the
 * subscriber reads nothing at all while a host peer on the same topic reads
 * a clean 10 Hz. The knobs that matter for that are the mbox depths, the
 * pbuf pool and MEM_SIZE, so those are the ones left overridable. */

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

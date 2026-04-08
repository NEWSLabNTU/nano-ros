/**
 * nx_tap_network_driver.c — TAP-based network driver for NetX Duo on Linux
 *
 * Uses /dev/net/tun with IFF_TAP | IFF_NO_PI for clean ethernet frame I/O.
 *
 * Architecture:
 *   - Init: open /dev/net/tun, ioctl(TUNSETIFF) to attach to named TAP
 *   - TX: write() ethernet frames to the TAP fd (from ThreadX thread context)
 *   - RX: pthread reads TAP fd → pipe → ThreadX thread processes packets
 *
 * The two-thread RX design is needed because:
 *   - NetX deferred_receive requires ThreadX scheduler context to wake IP thread
 *   - The ThreadX Linux port uses SIGUSR signals for scheduling
 *   - A raw pthread calling deferred_receive doesn't reliably wake the IP thread
 *   - Solution: pthread does blocking I/O on TAP fd, ThreadX thread does NetX calls
 *
 * SPDX-License-Identifier: Apache-2.0
 */

#include "nx_tap_network_driver.h"

#include <errno.h>
#include <fcntl.h>
#include <net/if.h>
#include <pthread.h>
#include <sched.h>
#include <stdio.h>
#include <string.h>
#include <unistd.h>
#include <linux/if_tun.h>
#include <sys/ioctl.h>
#include <sys/select.h>
#include <sys/time.h>

#include "nx_api.h"
#include "tx_api.h"

/* ---- Constants ---- */

#define NX_TAP_ETHERNET_SIZE    14
#define NX_TAP_MTU              1514
#define NX_TAP_ETHERNET_IP      0x0800
#define NX_TAP_ETHERNET_ARP     0x0806
#define NX_TAP_ETHERNET_RARP    0x8035
#define NX_TAP_ETHERNET_IPV6    0x86DD

#define NX_TAP_RX_STACK_SIZE    4096
#define NX_TAP_RX_PRIORITY      2

/* ---- Module state ---- */

static int          tap_fd = -1;
static int          tap_link_enabled = 0;
static const char  *tap_interface_name = "tap-tx0";
static ULONG        tap_mac_msw = 0x0002;
static ULONG        tap_mac_lsw = 0x00000000;
static NX_IP       *tap_ip_ptr = NULL;

/* Pipe for pthread → ThreadX thread communication */
static int          tap_pipe_fd[2] = {-1, -1};  /* [0]=read, [1]=write */

/* pthread for TAP fd reading */
static pthread_t    tap_reader_pthread;

/* ThreadX thread for packet processing */
static TX_THREAD    tap_rx_tx_thread;
static UCHAR        tap_rx_stack[NX_TAP_RX_STACK_SIZE];

/* Buffers */
static UCHAR        tap_rx_buffer[NX_TAP_MTU + 16];
static UCHAR        tap_tx_buffer[NX_TAP_MTU + 16];

/* ---- Public API ---- */

void nx_tap_set_interface_name(const char *name)
{
    tap_interface_name = name;
}

void nx_tap_set_mac_address(ULONG msw, ULONG lsw)
{
    tap_mac_msw = msw;
    tap_mac_lsw = lsw;
}

/* ---- Internal: open TAP device ---- */

static int tap_open(const char *dev_name)
{
    int fd = open("/dev/net/tun", O_RDWR);
    if (fd < 0)
    {
        fprintf(stderr, "[tap-netx] open(/dev/net/tun) failed: %s\n",
                strerror(errno));
        return -1;
    }

    struct ifreq ifr;
    memset(&ifr, 0, sizeof(ifr));
    ifr.ifr_flags = IFF_TAP | IFF_NO_PI;
    strncpy(ifr.ifr_name, dev_name, IFNAMSIZ - 1);

    if (ioctl(fd, TUNSETIFF, &ifr) < 0)
    {
        fprintf(stderr, "[tap-netx] ioctl(TUNSETIFF, %s) failed: %s\n",
                dev_name, strerror(errno));
        close(fd);
        return -1;
    }

    return fd;
}

/* ---- Internal: send ethernet frame ---- */

static UINT tap_send(NX_PACKET *packet_ptr)
{
    ULONG size = packet_ptr->nx_packet_length;

    if (size > NX_TAP_MTU)
        return NX_NOT_SUCCESSFUL;

    UCHAR *data;

#ifndef NX_DISABLE_PACKET_CHAIN
    if (packet_ptr->nx_packet_next)
    {
        ULONG copied = 0;
        if (nx_packet_data_retrieve(packet_ptr, tap_tx_buffer, &copied))
            return NX_NOT_SUCCESSFUL;
        data = tap_tx_buffer;
        size = copied;
    }
    else
#endif
    {
        data = packet_ptr->nx_packet_prepend_ptr;
    }

    ssize_t sent = write(tap_fd, data, size);
    if (sent != (ssize_t)size)
        return NX_NOT_SUCCESSFUL;

    return NX_SUCCESS;
}

/* ---- pthread: reads TAP fd, writes frames to pipe ---- */

static void *tap_reader_entry(void *arg)
{
    UCHAR buf[NX_TAP_MTU + 16];
    (void)arg;

    while (tap_link_enabled)
    {
        ssize_t bytes = read(tap_fd, buf, sizeof(buf));
        if (bytes < NX_TAP_ETHERNET_SIZE)
            continue;

        {
            static int rdr_count = 0;
            if (rdr_count < 5) { fprintf(stderr, "[tap-reader] %zd bytes\n", bytes); rdr_count++; }
        }

        /* Write length header + frame data to pipe.
           The ThreadX thread reads this on the other end. */
        uint16_t len = (uint16_t)bytes;
        /* Atomic-ish write: length + data in one write if possible */
        UCHAR msg[2 + NX_TAP_MTU + 16];
        msg[0] = (UCHAR)(len >> 8);
        msg[1] = (UCHAR)(len & 0xFF);
        memcpy(&msg[2], buf, (size_t)bytes);
        write(tap_pipe_fd[1], msg, 2 + (size_t)bytes);
    }

    return NULL;
}

/* ---- ThreadX thread: reads pipe, processes packets ---- */

static void tap_rx_thread_entry(ULONG input)
{
    NX_PACKET  *packet_ptr;
    UINT        status;
    UINT        packet_type;
    UCHAR       frame[NX_TAP_MTU + 16];

    (void)input;

    while (tap_link_enabled)
    {
        /* Read length header from pipe */
        uint8_t hdr[2];
        ssize_t n = read(tap_pipe_fd[0], hdr, 2);
        if (n != 2)
        {
            tx_thread_sleep(1);
            continue;
        }

        uint16_t len = ((uint16_t)hdr[0] << 8) | hdr[1];
        if (len > sizeof(frame) || len < NX_TAP_ETHERNET_SIZE)
        {
            /* Drain bad data */
            UCHAR drain[64];
            while (len > 0) {
                ssize_t r = read(tap_pipe_fd[0], drain,
                                 len < sizeof(drain) ? len : sizeof(drain));
                if (r <= 0) break;
                len -= (uint16_t)r;
            }
            continue;
        }

        /* Read frame data */
        size_t total = 0;
        while (total < len)
        {
            n = read(tap_pipe_fd[0], frame + total, len - total);
            if (n <= 0) break;
            total += (size_t)n;
        }

        if (total != len)
            continue;

        /* Allocate NetX packet */
        status = nx_packet_allocate(
            tap_ip_ptr->nx_ip_default_packet_pool,
            &packet_ptr, NX_RECEIVE_PACKET, TX_WAIT_FOREVER);

        if (status != NX_SUCCESS)
            continue;

        /* Copy frame into packet */
        status = nx_packet_data_append(
            packet_ptr, frame, (ULONG)len,
            tap_ip_ptr->nx_ip_default_packet_pool, TX_WAIT_FOREVER);

        if (status != NX_SUCCESS)
        {
            nx_packet_release(packet_ptr);
            continue;
        }

        /* Parse ethernet type */
        packet_type = (((UINT)frame[12]) << 8) | ((UINT)frame[13]);

        /* Strip ethernet header */
        packet_ptr->nx_packet_prepend_ptr += NX_TAP_ETHERNET_SIZE;
        packet_ptr->nx_packet_length      -= NX_TAP_ETHERNET_SIZE;

        {
            static int rx_count = 0;
            if (rx_count < 5) { fprintf(stderr, "[tap-rx] %u bytes type=0x%04x\n", (unsigned)len, packet_type); rx_count++; }
        }

        /* Route to protocol handler */
        if (packet_type == NX_TAP_ETHERNET_IP ||
            packet_type == NX_TAP_ETHERNET_IPV6)
        {
            /* Process directly in this ThreadX thread — avoids the deferred
               queue which requires the IP helper thread to be scheduled. */
            _nx_ip_packet_receive(tap_ip_ptr, packet_ptr);
        }
        else if (packet_type == NX_TAP_ETHERNET_ARP)
        {
            /* ARP deferred works because it's simpler and doesn't need
               TCP state machine synchronization. */
            _nx_arp_packet_deferred_receive(tap_ip_ptr, packet_ptr);
        }
        else if (packet_type == NX_TAP_ETHERNET_RARP)
        {
            _nx_rarp_packet_deferred_receive(tap_ip_ptr, packet_ptr);
        }
        else
        {
            nx_packet_release(packet_ptr);
        }
    }
}

/* ---- Internal: driver output ---- */

static void tap_driver_output(NX_PACKET *packet_ptr)
{
    UINT old_threshold = 0;
    tx_thread_preemption_change(tx_thread_identify(), 0, &old_threshold);

    tap_send(packet_ptr);

    packet_ptr->nx_packet_prepend_ptr += NX_TAP_ETHERNET_SIZE;
    packet_ptr->nx_packet_length      -= NX_TAP_ETHERNET_SIZE;
    nx_packet_transmit_release(packet_ptr);

    tx_thread_preemption_change(tx_thread_identify(), old_threshold,
                                &old_threshold);
}

/* ---- Public: NetX Duo driver entry point ---- */

void nx_tap_network_driver(NX_IP_DRIVER *driver_req_ptr)
{
    NX_IP        *ip_ptr         = driver_req_ptr->nx_ip_driver_ptr;
    NX_INTERFACE *interface_ptr  = driver_req_ptr->nx_ip_driver_interface;
    NX_PACKET    *packet_ptr;
    ULONG        *ethernet_frame_ptr;

    driver_req_ptr->nx_ip_driver_status = NX_SUCCESS;

    fprintf(stderr, "[tap] cmd=%u\n", driver_req_ptr->nx_ip_driver_command);

    switch (driver_req_ptr->nx_ip_driver_command)
    {

    case NX_LINK_INTERFACE_ATTACH:
        break;

    case NX_LINK_INITIALIZE:
    {
        tap_fd = tap_open(tap_interface_name);
        if (tap_fd < 0)
        {
            driver_req_ptr->nx_ip_driver_status = NX_NOT_CREATED;
            return;
        }

        /* Create pipe for pthread → ThreadX communication */
        if (pipe(tap_pipe_fd) < 0)
        {
            close(tap_fd);
            tap_fd = -1;
            driver_req_ptr->nx_ip_driver_status = NX_NOT_CREATED;
            return;
        }

        tap_ip_ptr = ip_ptr;

        interface_ptr->nx_interface_ip_mtu_size = NX_TAP_MTU - NX_TAP_ETHERNET_SIZE;

        /* Set MAC address */
        nx_ip_interface_physical_address_set(ip_ptr,
            interface_ptr->nx_interface_index,
            tap_mac_msw, tap_mac_lsw, NX_FALSE);

        interface_ptr->nx_interface_address_mapping_needed = NX_TRUE;
        break;
    }

    case NX_LINK_ENABLE:
    {
        tap_link_enabled = 1;

        /* Create ThreadX RX processing thread */
        tx_thread_create(&tap_rx_tx_thread, "tap_rx",
                         tap_rx_thread_entry, 0,
                         tap_rx_stack, NX_TAP_RX_STACK_SIZE,
                         NX_TAP_RX_PRIORITY, NX_TAP_RX_PRIORITY,
                         TX_NO_TIME_SLICE, TX_AUTO_START);

        /* Start pthread for TAP fd reading */
        pthread_create(&tap_reader_pthread, NULL, tap_reader_entry, NULL);

        interface_ptr->nx_interface_link_up = NX_TRUE;
        break;
    }

    case NX_LINK_DISABLE:
        tap_link_enabled = 0;
        interface_ptr->nx_interface_link_up = NX_FALSE;
        break;

    case NX_LINK_PACKET_SEND:
    case NX_LINK_PACKET_BROADCAST:
    case NX_LINK_ARP_SEND:
    case NX_LINK_ARP_RESPONSE_SEND:
    case NX_LINK_RARP_SEND:
    {
        packet_ptr = driver_req_ptr->nx_ip_driver_packet;

        packet_ptr->nx_packet_prepend_ptr -= NX_TAP_ETHERNET_SIZE;
        packet_ptr->nx_packet_length      += NX_TAP_ETHERNET_SIZE;

        ethernet_frame_ptr = (ULONG *)(packet_ptr->nx_packet_prepend_ptr - 2);

        /* Destination MAC */
        *ethernet_frame_ptr       = driver_req_ptr->nx_ip_driver_physical_address_msw;
        *(ethernet_frame_ptr + 1) = driver_req_ptr->nx_ip_driver_physical_address_lsw;

        /* Source MAC */
        *(ethernet_frame_ptr + 2) =
            (interface_ptr->nx_interface_physical_address_msw << 16) |
            (interface_ptr->nx_interface_physical_address_lsw >> 16);
        *(ethernet_frame_ptr + 3) =
            (interface_ptr->nx_interface_physical_address_lsw << 16);

        /* Ethernet type */
        if (driver_req_ptr->nx_ip_driver_command == NX_LINK_ARP_SEND ||
            driver_req_ptr->nx_ip_driver_command == NX_LINK_ARP_RESPONSE_SEND)
        {
            *(ethernet_frame_ptr + 3) |= NX_TAP_ETHERNET_ARP;
        }
        else if (driver_req_ptr->nx_ip_driver_command == NX_LINK_RARP_SEND)
        {
            *(ethernet_frame_ptr + 3) |= NX_TAP_ETHERNET_RARP;
        }
        else if (packet_ptr->nx_packet_ip_version == 4)
        {
            *(ethernet_frame_ptr + 3) |= NX_TAP_ETHERNET_IP;
        }
        else
        {
            *(ethernet_frame_ptr + 3) |= NX_TAP_ETHERNET_IPV6;
        }

        NX_CHANGE_ULONG_ENDIAN(*(ethernet_frame_ptr));
        NX_CHANGE_ULONG_ENDIAN(*(ethernet_frame_ptr + 1));
        NX_CHANGE_ULONG_ENDIAN(*(ethernet_frame_ptr + 2));
        NX_CHANGE_ULONG_ENDIAN(*(ethernet_frame_ptr + 3));

        tap_driver_output(packet_ptr);
        break;
    }

    case NX_LINK_MULTICAST_JOIN:
    case NX_LINK_MULTICAST_LEAVE:
        break;

    case NX_LINK_GET_STATUS:
        *(driver_req_ptr->nx_ip_driver_return_ptr) =
            interface_ptr->nx_interface_link_up;
        break;

    case NX_LINK_DEFERRED_PROCESSING:
        break;

    default:
        driver_req_ptr->nx_ip_driver_status = NX_UNHANDLED_COMMAND;
        break;
    }
}

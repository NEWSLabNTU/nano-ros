/**
 * nx_tap_network_driver.c — TAP-based network driver for NetX Duo on Linux
 *
 * Uses /dev/net/tun with IFF_TAP | IFF_NO_PI to send/receive raw ethernet
 * frames through a Linux TAP device. The TAP device is typically bridged
 * to a host bridge (e.g., qemu-br) for connectivity to zenohd.
 *
 * Architecture:
 *   - Init: open /dev/net/tun, ioctl(TUNSETIFF) to attach to named TAP
 *   - TX: write() ethernet frames to the TAP fd
 *   - RX: dedicated pthread reads ethernet frames, routes to NetX IP/ARP
 *   - No raw sockets, no CAP_NET_RAW needed
 *
 * SPDX-License-Identifier: Apache-2.0
 */

#include "nx_tap_network_driver.h"

#include <errno.h>
#include <fcntl.h>
#include <net/if.h>
#include <pthread.h>
#include <stdio.h>
#include <string.h>
#include <unistd.h>
#include <linux/if_tun.h>
#include <sys/ioctl.h>

#include "nx_api.h"
#include "tx_api.h"

/* ---- Constants ---- */

#define NX_TAP_ETHERNET_SIZE    14
#define NX_TAP_MTU              1514    /* Ethernet MTU including header */
#define NX_TAP_ETHERNET_IP      0x0800
#define NX_TAP_ETHERNET_ARP     0x0806
#define NX_TAP_ETHERNET_RARP    0x8035
#define NX_TAP_ETHERNET_IPV6    0x86DD

#define NX_TAP_MAC_OFFSET_MSW   0       /* Bytes 0-1 of MAC */
#define NX_TAP_MAC_OFFSET_LSW   2       /* Bytes 2-5 of MAC */

/* ---- Module state ---- */

static int          tap_fd = -1;
static int          tap_link_enabled = 0;
static const char  *tap_interface_name = "tap-tx0";
static NX_IP       *tap_ip_ptr = NULL;
static pthread_t    tap_rx_thread;
static UCHAR        tap_rx_buffer[NX_TAP_MTU + 16];    /* +16 for safety */
static UCHAR        tap_tx_buffer[NX_TAP_MTU + 16];

/* ---- Public: set interface name ---- */

void nx_tap_set_interface_name(const char *name)
{
    tap_interface_name = name;
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
    ifr.ifr_flags = IFF_TAP | IFF_NO_PI;   /* Ethernet frames, no packet info */
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
    {
        return NX_NOT_SUCCESSFUL;
    }

    UCHAR *data;

#ifndef NX_DISABLE_PACKET_CHAIN
    if (packet_ptr->nx_packet_next)
    {
        /* Chained packet — gather into contiguous buffer */
        ULONG copied = 0;
        if (nx_packet_data_retrieve(packet_ptr, tap_tx_buffer, &copied))
        {
            return NX_NOT_SUCCESSFUL;
        }
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
    {
        return NX_NOT_SUCCESSFUL;
    }

    return NX_SUCCESS;
}

/* ---- Internal: receive thread ---- */

static void *tap_rx_thread_entry(void *arg)
{
    NX_PACKET  *packet_ptr;
    UINT        status;
    UINT        packet_type;

    (void)arg;

    while (tap_link_enabled)
    {
        /* Blocking read — returns one ethernet frame */
        ssize_t bytes = read(tap_fd, tap_rx_buffer, sizeof(tap_rx_buffer));

        if (bytes < NX_TAP_ETHERNET_SIZE)
        {
            continue;   /* Too short or error */
        }

        _tx_thread_context_save();

        /* Allocate a NetX packet */
        status = nx_packet_allocate(
            tap_ip_ptr->nx_ip_default_packet_pool,
            &packet_ptr, NX_RECEIVE_PACKET, NX_NO_WAIT);

        if (status != NX_SUCCESS)
        {
            _tx_thread_context_restore();
            continue;   /* No packet available — drop */
        }

        /* Copy frame data into the packet */
        status = nx_packet_data_append(
            packet_ptr, tap_rx_buffer, (ULONG)bytes,
            tap_ip_ptr->nx_ip_default_packet_pool, NX_NO_WAIT);

        if (status != NX_SUCCESS)
        {
            nx_packet_release(packet_ptr);
            _tx_thread_context_restore();
            continue;
        }

        /* Parse ethernet type */
        packet_type = (((UINT)tap_rx_buffer[12]) << 8) |
                       ((UINT)tap_rx_buffer[13]);

        /* Strip ethernet header */
        packet_ptr->nx_packet_prepend_ptr += NX_TAP_ETHERNET_SIZE;
        packet_ptr->nx_packet_length      -= NX_TAP_ETHERNET_SIZE;

        /* Route to appropriate protocol handler */
        if (packet_type == NX_TAP_ETHERNET_IP ||
            packet_type == NX_TAP_ETHERNET_IPV6)
        {
            _nx_ip_packet_deferred_receive(tap_ip_ptr, packet_ptr);
        }
        else if (packet_type == NX_TAP_ETHERNET_ARP)
        {
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

        _tx_thread_context_restore();

        /* Yield to let ThreadX IP thread process deferred packets */
        usleep(100);
    }

    return NULL;
}

/* ---- Internal: driver output (prepends ethernet header, sends) ---- */

static void tap_driver_output(NX_PACKET *packet_ptr)
{
    UINT old_threshold = 0;
    tx_thread_preemption_change(tx_thread_identify(), 0, &old_threshold);

    tap_send(packet_ptr);

    /* Strip ethernet header (mirrors what a real NIC does after TX complete) */
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

    /* Default to success */
    driver_req_ptr->nx_ip_driver_status = NX_SUCCESS;

    switch (driver_req_ptr->nx_ip_driver_command)
    {

    case NX_LINK_INTERFACE_ATTACH:
        /* Nothing to do — interface is attached automatically */
        break;

    case NX_LINK_INITIALIZE:
    {
        /* Open the TAP device */
        tap_fd = tap_open(tap_interface_name);
        if (tap_fd < 0)
        {
            driver_req_ptr->nx_ip_driver_status = NX_NOT_CREATED;
            return;
        }

        tap_ip_ptr = ip_ptr;

        /* Set interface capabilities */
        interface_ptr->nx_interface_ip_mtu_size = NX_TAP_MTU - NX_TAP_ETHERNET_SIZE;

        /* Set MAC address on the interface */
        interface_ptr->nx_interface_physical_address_msw =
            (ULONG)((driver_req_ptr->nx_ip_driver_physical_address_msw) & 0xFFFF);
        interface_ptr->nx_interface_physical_address_lsw =
            driver_req_ptr->nx_ip_driver_physical_address_lsw;

        interface_ptr->nx_interface_address_mapping_needed = NX_TRUE;
        break;
    }

    case NX_LINK_ENABLE:
    {
        tap_link_enabled = 1;

        /* Start receive thread */
        struct sched_param sp;
        memset(&sp, 0, sizeof(sp));
        sp.sched_priority = 2;

        pthread_attr_t attr;
        pthread_attr_init(&attr);
        pthread_attr_setschedparam(&attr, &sp);

        pthread_create(&tap_rx_thread, &attr, tap_rx_thread_entry, NULL);
        pthread_attr_destroy(&attr);

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
        /* Build ethernet frame header */
        packet_ptr = driver_req_ptr->nx_ip_driver_packet;

        packet_ptr->nx_packet_prepend_ptr -= NX_TAP_ETHERNET_SIZE;
        packet_ptr->nx_packet_length      += NX_TAP_ETHERNET_SIZE;

        /* Build the ethernet header (word-aligned at prepend_ptr - 2) */
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

        /* Endian swap */
        NX_CHANGE_ULONG_ENDIAN(*(ethernet_frame_ptr));
        NX_CHANGE_ULONG_ENDIAN(*(ethernet_frame_ptr + 1));
        NX_CHANGE_ULONG_ENDIAN(*(ethernet_frame_ptr + 2));
        NX_CHANGE_ULONG_ENDIAN(*(ethernet_frame_ptr + 3));

        /* Send the frame */
        tap_driver_output(packet_ptr);
        break;
    }

    case NX_LINK_MULTICAST_JOIN:
    case NX_LINK_MULTICAST_LEAVE:
        /* TAP handles multicast at the host level — nothing to do */
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

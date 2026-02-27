/**
 * VirtIO-net NetX Duo driver
 *
 * Implements the NX_IP_DRIVER interface for the VirtIO MMIO network device.
 * Handles packet TX/RX through split virtqueues with static buffer allocation.
 * Uses PLIC interrupts for RX notification with deferred thread-context
 * processing.
 */

#include "virtio_net_nx.h"
#include "virtio_mmio.h"
#include "virtqueue.h"
#include "plic.h"
#include <string.h>

/* --------------------------------------------------------------------------
 * Constants
 * ----------------------------------------------------------------------- */

#define NX_LINK_MTU             1514
#define NX_ETHERNET_SIZE        14
#define NX_ETHERNET_IP          0x0800
#define NX_ETHERNET_ARP         0x0806
#define NX_ETHERNET_RARP        0x8035
#define NX_ETHERNET_IPV6        0x86DD

/** VirtIO net header size (without VIRTIO_NET_F_MRG_RXBUF) */
#define VIRTIO_NET_HDR_SIZE     12

#define NUM_RX_BUFFERS          32
#define BUFFER_SIZE             2048  /* virtio-net hdr (12) + MTU (1514) + padding */

/* --------------------------------------------------------------------------
 * Static state
 * ----------------------------------------------------------------------- */

static struct virtio_net_nx_config driver_config;
static NX_IP *driver_ip_ptr;
static struct virtqueue rxq, txq;
static volatile int rx_pending;

/* Static RX/TX packet buffers -- no dynamic allocation */
static uint8_t rx_buffers[NUM_RX_BUFFERS][BUFFER_SIZE]
    __attribute__((aligned(16)));
static uint8_t tx_buffer[BUFFER_SIZE]
    __attribute__((aligned(16)));

/* MAC address read from device */
static uint8_t device_mac[6];

/* --------------------------------------------------------------------------
 * Forward declarations
 * ----------------------------------------------------------------------- */

static void driver_initialize(NX_IP_DRIVER *driver_req);
static void driver_enable(NX_IP_DRIVER *driver_req);
static void driver_disable(NX_IP_DRIVER *driver_req);
static void driver_packet_send(NX_IP_DRIVER *driver_req);
static void driver_deferred_processing(NX_IP_DRIVER *driver_req);
static void driver_get_status(NX_IP_DRIVER *driver_req);
static int  virtio_net_isr(int irqno);
static void rx_fill_initial(void);

/* --------------------------------------------------------------------------
 * Public API
 * ----------------------------------------------------------------------- */

void virtio_net_nx_configure(const struct virtio_net_nx_config *config)
{
    driver_config = *config;
}

VOID virtio_net_nx_driver(NX_IP_DRIVER *driver_req_ptr)
{
    /* Default to success */
    driver_req_ptr->nx_ip_driver_status = NX_SUCCESS;

    switch (driver_req_ptr->nx_ip_driver_command) {

    case NX_LINK_INTERFACE_ATTACH:
        break;

    case NX_LINK_INITIALIZE:
        driver_initialize(driver_req_ptr);
        break;

    case NX_LINK_ENABLE:
        driver_enable(driver_req_ptr);
        break;

    case NX_LINK_DISABLE:
        driver_disable(driver_req_ptr);
        break;

    case NX_LINK_PACKET_SEND:
    case NX_LINK_PACKET_BROADCAST:
    case NX_LINK_ARP_SEND:
    case NX_LINK_ARP_RESPONSE_SEND:
    case NX_LINK_RARP_SEND:
        driver_packet_send(driver_req_ptr);
        break;

    case NX_LINK_DEFERRED_PROCESSING:
        driver_deferred_processing(driver_req_ptr);
        break;

    case NX_LINK_GET_STATUS:
        driver_get_status(driver_req_ptr);
        break;

    case NX_LINK_MULTICAST_JOIN:
    case NX_LINK_MULTICAST_LEAVE:
        /* Accept silently -- virtio-net receives all multicast by default */
        break;

    default:
        driver_req_ptr->nx_ip_driver_status = NX_UNHANDLED_COMMAND;
        break;
    }
}

/* --------------------------------------------------------------------------
 * NX_LINK_INITIALIZE
 * ----------------------------------------------------------------------- */

static void driver_initialize(NX_IP_DRIVER *driver_req)
{
    NX_IP        *ip_ptr     = driver_req->nx_ip_driver_ptr;
    NX_INTERFACE *iface      = driver_req->nx_ip_driver_interface;
    UINT          iface_idx  = iface->nx_interface_index;
    uint64_t      base       = driver_config.mmio_base;

    /* 1. Probe VirtIO MMIO device */
    if (virtio_mmio_probe(base) != 0) {
        driver_req->nx_ip_driver_status = NX_NOT_SUCCESSFUL;
        return;
    }

    /* 2. Reset and set ACKNOWLEDGE + DRIVER */
    virtio_mmio_reset(base);
    virtio_mmio_set_status(base, VIRTIO_STATUS_ACKNOWLEDGE);
    virtio_mmio_set_status(base, VIRTIO_STATUS_DRIVER);

    /* 3. Negotiate features */
    uint32_t features = virtio_mmio_negotiate_features(
        base, VIRTIO_NET_F_MAC | VIRTIO_NET_F_STATUS);
    if (features == 0) {
        virtio_mmio_set_status(base, VIRTIO_STATUS_FAILED);
        driver_req->nx_ip_driver_status = NX_NOT_SUCCESSFUL;
        return;
    }

    /* 4. Initialize virtqueues */
    if (virtqueue_init(&rxq, 0, base) != 0 ||
        virtqueue_init(&txq, 1, base) != 0) {
        virtio_mmio_set_status(base, VIRTIO_STATUS_FAILED);
        driver_req->nx_ip_driver_status = NX_NOT_SUCCESSFUL;
        return;
    }

    /* 5. DRIVER_OK -- device is live */
    virtio_mmio_set_status(base, VIRTIO_STATUS_DRIVER_OK);

    /* 6. Read MAC address from device config space (offset 0x100) */
    if (features & VIRTIO_NET_F_MAC) {
        for (int i = 0; i < 6; i++) {
            /* Each byte at config + i; read 32-bit and mask */
            uint32_t val = virtio_mmio_read32(base,
                VIRTIO_MMIO_CONFIG + (i & ~3));
            device_mac[i] = (uint8_t)(val >> (8 * (i & 3)));
        }
    } else {
        /* Fallback MAC: locally administered */
        device_mac[0] = 0x52;
        device_mac[1] = 0x54;
        device_mac[2] = 0x00;
        device_mac[3] = 0x12;
        device_mac[4] = 0x34;
        device_mac[5] = 0x56;
    }

    /* 7. Configure NetX interface */
    ULONG mac_msw = ((ULONG)device_mac[0] << 8) | (ULONG)device_mac[1];
    ULONG mac_lsw = ((ULONG)device_mac[2] << 24) | ((ULONG)device_mac[3] << 16) |
                    ((ULONG)device_mac[4] << 8)  | (ULONG)device_mac[5];

    nx_ip_interface_mtu_set(ip_ptr, iface_idx,
                            NX_LINK_MTU - NX_ETHERNET_SIZE);
    nx_ip_interface_physical_address_set(ip_ptr, iface_idx,
                                        mac_msw, mac_lsw, NX_FALSE);
    nx_ip_interface_address_mapping_configure(ip_ptr, iface_idx, NX_TRUE);

    /* Save IP pointer for ISR use */
    driver_ip_ptr = ip_ptr;
    rx_pending = 0;
}

/* --------------------------------------------------------------------------
 * NX_LINK_ENABLE
 * ----------------------------------------------------------------------- */

static void driver_enable(NX_IP_DRIVER *driver_req)
{
    NX_INTERFACE *iface = driver_req->nx_ip_driver_interface;

    /* Pre-fill RX virtqueue with buffers (device-writable) */
    rx_fill_initial();
    virtqueue_kick(&rxq);

    /* Register PLIC interrupt */
    plic_register_callback(driver_config.irq_num, virtio_net_isr);
    plic_prio_set(driver_config.irq_num, 1);
    plic_irq_enable(driver_config.irq_num);

    iface->nx_interface_link_up = NX_TRUE;
}

/* --------------------------------------------------------------------------
 * NX_LINK_DISABLE
 * ----------------------------------------------------------------------- */

static void driver_disable(NX_IP_DRIVER *driver_req)
{
    NX_INTERFACE *iface = driver_req->nx_ip_driver_interface;

    plic_irq_disable(driver_config.irq_num);

    iface->nx_interface_link_up = NX_FALSE;
}

/* --------------------------------------------------------------------------
 * NX_LINK_PACKET_SEND (+ BROADCAST, ARP_SEND, ARP_RESPONSE_SEND, RARP_SEND)
 * ----------------------------------------------------------------------- */

static void driver_packet_send(NX_IP_DRIVER *driver_req)
{
    NX_PACKET    *packet_ptr = driver_req->nx_ip_driver_packet;
    NX_INTERFACE *iface      = driver_req->nx_ip_driver_interface;
    uint32_t      offset     = 0;

    /* 1. Build virtio-net header (12 bytes, all zeros for simple TX) */
    memset(tx_buffer, 0, VIRTIO_NET_HDR_SIZE);
    offset = VIRTIO_NET_HDR_SIZE;

    /* 2. Build Ethernet header (14 bytes) */
    /* Destination MAC from driver request */
    ULONG dst_msw = driver_req->nx_ip_driver_physical_address_msw;
    ULONG dst_lsw = driver_req->nx_ip_driver_physical_address_lsw;
    tx_buffer[offset + 0] = (uint8_t)(dst_msw >> 8);
    tx_buffer[offset + 1] = (uint8_t)(dst_msw);
    tx_buffer[offset + 2] = (uint8_t)(dst_lsw >> 24);
    tx_buffer[offset + 3] = (uint8_t)(dst_lsw >> 16);
    tx_buffer[offset + 4] = (uint8_t)(dst_lsw >> 8);
    tx_buffer[offset + 5] = (uint8_t)(dst_lsw);

    /* Source MAC from interface */
    ULONG src_msw = iface->nx_interface_physical_address_msw;
    ULONG src_lsw = iface->nx_interface_physical_address_lsw;
    tx_buffer[offset + 6]  = (uint8_t)(src_msw >> 8);
    tx_buffer[offset + 7]  = (uint8_t)(src_msw);
    tx_buffer[offset + 8]  = (uint8_t)(src_lsw >> 24);
    tx_buffer[offset + 9]  = (uint8_t)(src_lsw >> 16);
    tx_buffer[offset + 10] = (uint8_t)(src_lsw >> 8);
    tx_buffer[offset + 11] = (uint8_t)(src_lsw);

    /* EtherType */
    uint16_t ethertype;
    switch (driver_req->nx_ip_driver_command) {
    case NX_LINK_ARP_SEND:
    case NX_LINK_ARP_RESPONSE_SEND:
        ethertype = NX_ETHERNET_ARP;
        break;
    case NX_LINK_RARP_SEND:
        ethertype = NX_ETHERNET_RARP;
        break;
    default:
        ethertype = (packet_ptr->nx_packet_ip_version == 4)
                  ? NX_ETHERNET_IP : NX_ETHERNET_IPV6;
        break;
    }
    tx_buffer[offset + 12] = (uint8_t)(ethertype >> 8);
    tx_buffer[offset + 13] = (uint8_t)(ethertype);
    offset += NX_ETHERNET_SIZE;

    /* 3. Copy IP payload from NX_PACKET chain */
    NX_PACKET *cur = packet_ptr;
    while (cur != NULL) {
        uint32_t chunk_len = (uint32_t)(cur->nx_packet_append_ptr -
                                        cur->nx_packet_prepend_ptr);
        if (offset + chunk_len > BUFFER_SIZE) {
            chunk_len = BUFFER_SIZE - offset;
        }
        memcpy(&tx_buffer[offset], cur->nx_packet_prepend_ptr, chunk_len);
        offset += chunk_len;
#ifndef NX_DISABLE_PACKET_CHAIN
        cur = cur->nx_packet_next;
#else
        cur = NULL;
#endif
    }

    /* 4. Enqueue TX buffer (device-readable) */
    int desc_idx = virtqueue_add_buf(&txq, (uint64_t)(uintptr_t)tx_buffer,
                                     offset, 0);
    if (desc_idx < 0) {
        driver_req->nx_ip_driver_status = NX_NOT_SUCCESSFUL;
        nx_packet_transmit_release(packet_ptr);
        return;
    }

    /* 5. Notify device */
    virtqueue_kick(&txq);

    /* 6. Busy-wait for TX completion (single packet in flight) */
    uint32_t _unused_len;
    int completed;
    for (int spin = 0; spin < 100000; spin++) {
        completed = virtqueue_get_used(&txq, &_unused_len);
        if (completed >= 0) {
            virtqueue_free_desc(&txq, (uint16_t)completed);
            break;
        }
    }

    /* 7. Release the NX_PACKET */
    nx_packet_transmit_release(packet_ptr);
}

/* --------------------------------------------------------------------------
 * NX_LINK_DEFERRED_PROCESSING -- called in thread context after ISR
 * ----------------------------------------------------------------------- */

static void driver_deferred_processing(NX_IP_DRIVER *driver_req)
{
    NX_IP *ip_ptr = driver_req->nx_ip_driver_ptr;
    uint32_t len;
    int desc_idx;
    int reposted = 0;

    (void)ip_ptr;  /* Used via driver_ip_ptr */

    rx_pending = 0;

    /* Process all completed RX buffers */
    while ((desc_idx = virtqueue_get_used(&rxq, &len)) >= 0) {
        /* Read buffer address from descriptor before freeing it */
        uint8_t *buf = (uint8_t *)(uintptr_t)rxq.desc[desc_idx].addr;

        /* Free descriptor back to the free list */
        virtqueue_free_desc(&rxq, (uint16_t)desc_idx);

        /* Skip virtio-net header */
        uint8_t *frame = buf + VIRTIO_NET_HDR_SIZE;
        uint32_t frame_len = len - VIRTIO_NET_HDR_SIZE;

        if (frame_len < NX_ETHERNET_SIZE) {
            /* Runt frame -- re-post buffer and continue */
            virtqueue_add_buf(&rxq, (uint64_t)(uintptr_t)buf,
                              BUFFER_SIZE, VIRTQ_DESC_F_WRITE);
            reposted++;
            continue;
        }

        /* Allocate NX_PACKET */
        NX_PACKET *packet_ptr = NULL;
        UINT status = nx_packet_allocate(
            driver_ip_ptr->nx_ip_default_packet_pool,
            &packet_ptr, NX_RECEIVE_PACKET, NX_NO_WAIT);

        if (status != NX_SUCCESS || packet_ptr == NULL) {
            /* No packet available -- re-post buffer and drop frame */
            virtqueue_add_buf(&rxq, (uint64_t)(uintptr_t)buf,
                              BUFFER_SIZE, VIRTQ_DESC_F_WRITE);
            reposted++;
            continue;
        }

        /* Copy frame data into NX_PACKET */
        /* Adjust prepend_ptr for 4-byte alignment of IP header */
        packet_ptr->nx_packet_prepend_ptr += 2;
        packet_ptr->nx_packet_append_ptr  += 2;

        status = nx_packet_data_append(packet_ptr, frame, frame_len,
                                       driver_ip_ptr->nx_ip_default_packet_pool,
                                       NX_NO_WAIT);
        if (status != NX_SUCCESS) {
            nx_packet_release(packet_ptr);
            virtqueue_add_buf(&rxq, (uint64_t)(uintptr_t)buf,
                              BUFFER_SIZE, VIRTQ_DESC_F_WRITE);
            reposted++;
            continue;
        }

        /* Extract EtherType from Ethernet header (bytes 12-13) */
        uint16_t ethertype = ((uint16_t)frame[12] << 8) | (uint16_t)frame[13];

        /* Strip Ethernet header */
        packet_ptr->nx_packet_prepend_ptr += NX_ETHERNET_SIZE;
        packet_ptr->nx_packet_length      -= NX_ETHERNET_SIZE;

        /* Route to appropriate NetX handler */
        if (ethertype == NX_ETHERNET_IP || ethertype == NX_ETHERNET_IPV6) {
            _nx_ip_packet_deferred_receive(driver_ip_ptr, packet_ptr);
        } else if (ethertype == NX_ETHERNET_ARP) {
            _nx_arp_packet_deferred_receive(driver_ip_ptr, packet_ptr);
        } else if (ethertype == NX_ETHERNET_RARP) {
            _nx_rarp_packet_deferred_receive(driver_ip_ptr, packet_ptr);
        } else {
            nx_packet_release(packet_ptr);
        }

        /* Re-post this buffer to the RX virtqueue */
        virtqueue_add_buf(&rxq, (uint64_t)(uintptr_t)buf,
                          BUFFER_SIZE, VIRTQ_DESC_F_WRITE);
        reposted++;
    }

    /* Notify device of newly available RX buffers */
    if (reposted > 0) {
        virtqueue_kick(&rxq);
    }
}

/* --------------------------------------------------------------------------
 * NX_LINK_GET_STATUS
 * ----------------------------------------------------------------------- */

static void driver_get_status(NX_IP_DRIVER *driver_req)
{
    NX_INTERFACE *iface = driver_req->nx_ip_driver_interface;
    *(driver_req->nx_ip_driver_return_ptr) = (ULONG)iface->nx_interface_link_up;
}

/* --------------------------------------------------------------------------
 * PLIC ISR
 * ----------------------------------------------------------------------- */

static int virtio_net_isr(int irqno)
{
    uint64_t base = driver_config.mmio_base;

    /* Acknowledge interrupt */
    uint32_t isr_status = virtio_mmio_read32(base,
                                             VIRTIO_MMIO_INTERRUPT_STATUS);
    virtio_mmio_write32(base, VIRTIO_MMIO_INTERRUPT_ACK, isr_status);

    (void)irqno;

    if (isr_status & VIRTIO_MMIO_INT_VRING) {
        if (!rx_pending) {
            rx_pending = 1;
            /* Tell NetX Duo to call us back with DEFERRED_PROCESSING */
            _nx_ip_driver_deferred_processing(driver_ip_ptr);
        }
    }

    return 0;
}

/* --------------------------------------------------------------------------
 * RX buffer initial fill (called once from driver_enable)
 * ----------------------------------------------------------------------- */

static void rx_fill_initial(void)
{
    for (int i = 0; i < NUM_RX_BUFFERS; i++) {
        int idx = virtqueue_add_buf(&rxq,
                                    (uint64_t)(uintptr_t)rx_buffers[i],
                                    BUFFER_SIZE,
                                    VIRTQ_DESC_F_WRITE);
        if (idx < 0) {
            break;
        }
    }
}

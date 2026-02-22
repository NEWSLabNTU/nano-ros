/**
 * LAN9118/SMSC911x lwIP netif driver for QEMU MPS2-AN385
 *
 * Register map and init sequence from the lan9118-smoltcp Rust driver.
 * Polling-based RX, lwIP pbuf-based TX.
 */

#include "lan9118_lwip.h"

#include "lwip/etharp.h"
#include "lwip/pbuf.h"
#include "netif/ethernet.h"

#include <string.h>

/* ========================================================================
 * Register offsets
 * ======================================================================== */

#define REG_RX_DATA_PORT  0x00
#define REG_TX_DATA_PORT  0x20
#define REG_RX_STAT_PORT  0x40
#define REG_ID_REV        0x50
#define REG_INT_STS       0x58
#define REG_INT_EN        0x5C
#define REG_FIFO_INT      0x68
#define REG_RX_CFG        0x6C
#define REG_TX_CFG        0x70
#define REG_HW_CFG        0x74
#define REG_RX_DP_CTRL    0x78
#define REG_RX_FIFO_INF   0x7C
#define REG_TX_FIFO_INF   0x80
#define REG_GPIO_CFG      0x88
#define REG_MAC_CSR_CMD   0xA4
#define REG_MAC_CSR_DATA  0xA8
#define REG_AFC_CFG       0xAC

/* HW_CFG bits */
#define HW_CFG_SRST           (1u << 0)
#define HW_CFG_TX_FIF_SZ_SHIFT 16
#define HW_CFG_TX_FIF_SZ_MASK (0xFu << HW_CFG_TX_FIF_SZ_SHIFT)
#define HW_CFG_MBO            (1u << 20)

/* TX_CFG bits */
#define TX_CFG_TX_ON  (1u << 1)

/* RX_DP_CTRL bits */
#define RX_DP_CTRL_RX_FFWD  (1u << 31)

/* IRQ_CFG default */
#define IRQ_CFG_DEFAULT  0x22000111u

/* RX_FIFO_INF fields */
#define RX_FIFO_INF_RXSUSED_SHIFT 16
#define RX_FIFO_INF_RXSUSED_MASK  (0xFFu << RX_FIFO_INF_RXSUSED_SHIFT)

/* TX_FIFO_INF fields */
#define TX_FIFO_INF_TXDFREE_MASK  0xFFFFu

/* RX status fields */
#define RX_STAT_PKT_LEN_SHIFT 16
#define RX_STAT_PKT_LEN_MASK  (0x3FFFu << RX_STAT_PKT_LEN_SHIFT)
#define RX_STAT_ES             (1u << 15)

/* TX command A */
#define TX_CMD_A_FIRST_SEG  (1u << 13)
#define TX_CMD_A_LAST_SEG   (1u << 12)

/* TX command B */
#define TX_CMD_B_PKT_TAG_SHIFT 16

/* MAC CSR command bits */
#define MAC_CSR_BUSY   (1u << 31)
#define MAC_CSR_READ   (1u << 30)

/* MAC CSR register indices */
#define MAC_CSR_MAC_CR   1
#define MAC_CSR_ADDRH    2
#define MAC_CSR_ADDRL    3
#define MAC_CSR_MII_ACC  6
#define MAC_CSR_MII_DATA 7

/* MAC CR bits */
#define MAC_CR_TXEN  (1u << 3)
#define MAC_CR_RXEN  (1u << 2)

/* MII access bits */
#define MII_ACC_BUSY           (1u << 0)
#define MII_ACC_WRITE          (1u << 1)
#define MII_ACC_PHY_ADDR_SHIFT 11
#define MII_ACC_REG_ADDR_SHIFT 6

/* PHY */
#define PHY_ADDR    1
#define PHY_BMCR    0
#define PHY_BMSR    1
#define PHY_PHYID1  2
#define PHY_ANAR    4

#define BMCR_RESET     (1u << 15)
#define BMCR_ANENABLE  (1u << 12)
#define BMCR_ANRESTART (1u << 9)

#define BMSR_LSTATUS   (1u << 2)

#define ANAR_ALL_CAPS  0x0DE1u

/* Device IDs */
#define DEV_ID_LAN9220  0x9220u
#define DEV_ID_LAN9118  0x0118u

/* Frame limits */
#define MAX_FRAME_SIZE 1536

/* ========================================================================
 * MMIO helpers
 * ======================================================================== */

static inline uint32_t reg_read(uint32_t base, uint32_t offset) {
    return *(volatile uint32_t *)(uintptr_t)(base + offset);
}

static inline void reg_write(uint32_t base, uint32_t offset, uint32_t val) {
    *(volatile uint32_t *)(uintptr_t)(base + offset) = val;
}

static inline void delay_us(uint32_t us) {
    /* ~25 iterations per us at 25 MHz (matching the Rust driver) */
    for (volatile uint32_t i = 0; i < us * 25; i++) {}
}

/* ========================================================================
 * MAC CSR access (indirect register read/write via synchroniser)
 * ======================================================================== */

static int wait_mac_not_busy(uint32_t base) {
    for (int i = 0; i < 1000; i++) {
        if ((reg_read(base, REG_MAC_CSR_CMD) & MAC_CSR_BUSY) == 0)
            return 0;
        delay_us(1);
    }
    return -1;
}

static int mac_read(uint32_t base, uint8_t reg, uint32_t *out) {
    if (wait_mac_not_busy(base) != 0) return -1;
    reg_write(base, REG_MAC_CSR_CMD, MAC_CSR_BUSY | MAC_CSR_READ | reg);
    if (wait_mac_not_busy(base) != 0) return -1;
    *out = reg_read(base, REG_MAC_CSR_DATA);
    return 0;
}

static int mac_write(uint32_t base, uint8_t reg, uint32_t val) {
    if (wait_mac_not_busy(base) != 0) return -1;
    reg_write(base, REG_MAC_CSR_DATA, val);
    reg_write(base, REG_MAC_CSR_CMD, MAC_CSR_BUSY | reg);
    if (wait_mac_not_busy(base) != 0) return -1;
    return 0;
}

/* ========================================================================
 * PHY access (via MAC CSR MII registers)
 * ======================================================================== */

static int phy_read(uint32_t base, uint8_t reg, uint16_t *out) {
    uint32_t mii_acc;
    if (mac_read(base, MAC_CSR_MII_ACC, &mii_acc) != 0) return -1;
    if (mii_acc & MII_ACC_BUSY) return -1;

    uint32_t cmd = ((uint32_t)PHY_ADDR << MII_ACC_PHY_ADDR_SHIFT)
                 | ((uint32_t)reg << MII_ACC_REG_ADDR_SHIFT)
                 | MII_ACC_BUSY;
    if (mac_write(base, MAC_CSR_MII_ACC, cmd) != 0) return -1;

    for (int i = 0; i < 1000; i++) {
        delay_us(10);
        if (mac_read(base, MAC_CSR_MII_ACC, &mii_acc) != 0) return -1;
        if ((mii_acc & MII_ACC_BUSY) == 0) {
            uint32_t data;
            if (mac_read(base, MAC_CSR_MII_DATA, &data) != 0) return -1;
            *out = (uint16_t)data;
            return 0;
        }
    }
    return -1;
}

static int phy_write(uint32_t base, uint8_t reg, uint16_t val) {
    uint32_t mii_acc;
    if (mac_read(base, MAC_CSR_MII_ACC, &mii_acc) != 0) return -1;
    if (mii_acc & MII_ACC_BUSY) return -1;

    if (mac_write(base, MAC_CSR_MII_DATA, (uint32_t)val) != 0) return -1;

    uint32_t cmd = ((uint32_t)PHY_ADDR << MII_ACC_PHY_ADDR_SHIFT)
                 | ((uint32_t)reg << MII_ACC_REG_ADDR_SHIFT)
                 | MII_ACC_WRITE
                 | MII_ACC_BUSY;
    if (mac_write(base, MAC_CSR_MII_ACC, cmd) != 0) return -1;

    for (int i = 0; i < 1000; i++) {
        delay_us(10);
        if (mac_read(base, MAC_CSR_MII_ACC, &mii_acc) != 0) return -1;
        if ((mii_acc & MII_ACC_BUSY) == 0)
            return 0;
    }
    return -1;
}

/* ========================================================================
 * Hardware init (mirrors lan9118-smoltcp init sequence)
 * ======================================================================== */

static int hw_init(uint32_t base, const uint8_t mac[6]) {
    /* 1. Check device presence */
    uint32_t id_rev = reg_read(base, REG_ID_REV);
    uint16_t upper = (uint16_t)(id_rev >> 16);
    uint16_t lower = (uint16_t)(id_rev & 0xFFFF);
    if (upper == lower)
        return -1;
    if (upper != DEV_ID_LAN9220 && upper != DEV_ID_LAN9118)
        return -1;

    /* 2. Software reset */
    uint32_t hw_cfg = reg_read(base, REG_HW_CFG);
    reg_write(base, REG_HW_CFG, hw_cfg | HW_CFG_SRST);
    for (int i = 0; i < 1000; i++) {
        if ((reg_read(base, REG_HW_CFG) & HW_CFG_SRST) == 0)
            goto reset_done;
        delay_us(10);
    }
    return -1;  /* reset timeout */
reset_done:

    /* 3. Set TX FIFO size to 5 KB */
    hw_cfg = reg_read(base, REG_HW_CFG);
    hw_cfg = (hw_cfg & ~HW_CFG_TX_FIF_SZ_MASK)
           | (5u << HW_CFG_TX_FIF_SZ_SHIFT)
           | HW_CFG_MBO;
    reg_write(base, REG_HW_CFG, hw_cfg);

    /* 4. Auto flow control */
    reg_write(base, REG_AFC_CFG, 0x006E3740u);

    /* 5. GPIO / LEDs */
    reg_write(base, REG_GPIO_CFG, 0x70070000u);

    /* 6. Interrupts — disable all, clear pending */
    reg_write(base, REG_INT_EN, 0);
    reg_write(base, REG_INT_STS, 0xFFFFFFFFu);

    /* 7. PHY init */
    uint16_t phy_id1;
    if (phy_read(base, PHY_PHYID1, &phy_id1) != 0) return -1;
    if (phy_id1 == 0xFFFF || phy_id1 == 0) return -1;

    if (phy_write(base, PHY_BMCR, (uint16_t)BMCR_RESET) != 0) return -1;
    for (int i = 0; i < 100; i++) {
        delay_us(1000);
        uint16_t bmcr;
        if (phy_read(base, PHY_BMCR, &bmcr) != 0) return -1;
        if ((bmcr & BMCR_RESET) == 0) break;
    }

    uint16_t anar;
    if (phy_read(base, PHY_ANAR, &anar) != 0) return -1;
    if (phy_write(base, PHY_ANAR, anar | ANAR_ALL_CAPS) != 0) return -1;
    if (phy_write(base, PHY_BMCR, (uint16_t)(BMCR_ANENABLE | BMCR_ANRESTART)) != 0)
        return -1;

    /* 8. FIFO interrupt threshold */
    reg_write(base, REG_FIFO_INT, 0xFF000000u);

    /* 9. Enable MAC TX + RX */
    uint32_t mac_cr;
    if (mac_read(base, MAC_CSR_MAC_CR, &mac_cr) != 0) return -1;
    if (mac_write(base, MAC_CSR_MAC_CR, mac_cr | MAC_CR_TXEN) != 0) return -1;

    reg_write(base, REG_TX_CFG, TX_CFG_TX_ON);
    reg_write(base, REG_RX_CFG, 0);

    if (mac_read(base, MAC_CSR_MAC_CR, &mac_cr) != 0) return -1;
    if (mac_write(base, MAC_CSR_MAC_CR, mac_cr | MAC_CR_RXEN) != 0) return -1;

    /* 10. Clear RX threshold */
    uint32_t fifo_int = reg_read(base, REG_FIFO_INT);
    reg_write(base, REG_FIFO_INT, fifo_int & ~0xFFu);

    /* 11. Write MAC address */
    uint32_t addrl = (uint32_t)mac[0]
                   | ((uint32_t)mac[1] << 8)
                   | ((uint32_t)mac[2] << 16)
                   | ((uint32_t)mac[3] << 24);
    uint32_t addrh = (uint32_t)mac[4]
                   | ((uint32_t)mac[5] << 8);
    if (mac_write(base, MAC_CSR_ADDRL, addrl) != 0) return -1;
    if (mac_write(base, MAC_CSR_ADDRH, addrh) != 0) return -1;

    return 0;
}

/* ========================================================================
 * RX helpers
 * ======================================================================== */

static inline uint32_t rx_packets_pending(uint32_t base) {
    uint32_t inf = reg_read(base, REG_RX_FIFO_INF);
    return (inf & RX_FIFO_INF_RXSUSED_MASK) >> RX_FIFO_INF_RXSUSED_SHIFT;
}

static void rx_discard(uint32_t base) {
    reg_write(base, REG_RX_DP_CTRL, RX_DP_CTRL_RX_FFWD);
    for (int i = 0; i < 100; i++) {
        if ((reg_read(base, REG_RX_DP_CTRL) & RX_DP_CTRL_RX_FFWD) == 0)
            return;
        delay_us(1);
    }
}

/**
 * Receive one packet into a pbuf chain.
 * Returns NULL if no packet or on error.
 */
static struct pbuf *rx_receive(uint32_t base) {
    if (rx_packets_pending(base) == 0)
        return NULL;

    uint32_t rx_stat = reg_read(base, REG_RX_STAT_PORT);
    uint32_t pkt_len = (rx_stat & RX_STAT_PKT_LEN_MASK) >> RX_STAT_PKT_LEN_SHIFT;

    /* Error check */
    if (rx_stat & RX_STAT_ES) {
        rx_discard(base);
        return NULL;
    }

    /* Length sanity (includes 4-byte FCS) */
    if (pkt_len < 4 || pkt_len > MAX_FRAME_SIZE) {
        rx_discard(base);
        return NULL;
    }

    uint32_t data_len = pkt_len - 4;  /* strip FCS */

    /* Allocate pbuf for the Ethernet frame */
    struct pbuf *p = pbuf_alloc(PBUF_RAW, (u16_t)data_len, PBUF_POOL);
    if (p == NULL) {
        rx_discard(base);
        return NULL;
    }

    /* Read packet data from RX FIFO (DWORD-aligned reads) */
    uint32_t read_words = (pkt_len + 3) / 4;
    uint32_t fifo_offset = 0;
    struct pbuf *q = p;
    uint32_t pbuf_offset = 0;

    for (uint32_t i = 0; i < read_words; i++) {
        uint32_t word = reg_read(base, REG_RX_DATA_PORT);

        /* Copy up to 4 bytes from this word, skipping FCS bytes at end */
        for (int b = 0; b < 4 && fifo_offset < data_len; b++, fifo_offset++) {
            uint8_t byte = (uint8_t)(word >> (b * 8));

            /* Walk pbuf chain */
            while (q != NULL && pbuf_offset >= q->len) {
                pbuf_offset -= q->len;
                q = q->next;
            }
            if (q != NULL) {
                ((uint8_t *)q->payload)[pbuf_offset] = byte;
                pbuf_offset++;
            }
        }

        /* Consume remaining FCS bytes from this word without storing */
        fifo_offset += (fifo_offset < data_len) ? 0 : 0;
    }

    return p;
}

/* ========================================================================
 * TX (lwIP linkoutput callback)
 * ======================================================================== */

static err_t low_level_output(struct netif *netif, struct pbuf *p) {
    struct lan9118_config *cfg = (struct lan9118_config *)netif->state;
    uint32_t base = cfg->base_addr;

    uint16_t total = p->tot_len;
    if (total > MAX_FRAME_SIZE)
        return ERR_BUF;

    /* Check TX FIFO has enough space (frame + 8 bytes for commands) */
    uint32_t tx_free = reg_read(base, REG_TX_FIFO_INF) & TX_FIFO_INF_TXDFREE_MASK;
    if (tx_free < (uint32_t)(total + 8))
        return ERR_MEM;

    /* TX Command A */
    uint32_t cmd_a = TX_CMD_A_FIRST_SEG | TX_CMD_A_LAST_SEG | (uint32_t)total;
    reg_write(base, REG_TX_DATA_PORT, cmd_a);

    /* TX Command B */
    uint32_t cmd_b = ((uint32_t)total << TX_CMD_B_PKT_TAG_SHIFT) | (uint32_t)total;
    reg_write(base, REG_TX_DATA_PORT, cmd_b);

    /* Write frame data in DWORD-aligned chunks, walking the pbuf chain */
    uint32_t word = 0;
    uint32_t byte_idx = 0;  /* position within current 32-bit word (0-3) */

    for (struct pbuf *q = p; q != NULL; q = q->next) {
        const uint8_t *src = (const uint8_t *)q->payload;
        for (uint16_t i = 0; i < q->len; i++) {
            word |= (uint32_t)src[i] << (byte_idx * 8);
            byte_idx++;
            if (byte_idx == 4) {
                reg_write(base, REG_TX_DATA_PORT, word);
                word = 0;
                byte_idx = 0;
            }
        }
    }

    /* Flush partial last word (zero-padded) */
    if (byte_idx > 0) {
        reg_write(base, REG_TX_DATA_PORT, word);
    }

    return ERR_OK;
}

/* ========================================================================
 * Public API
 * ======================================================================== */

err_t lan9118_lwip_init(struct netif *netif) {
    struct lan9118_config *cfg = (struct lan9118_config *)netif->state;
    if (cfg == NULL)
        return ERR_ARG;

    /* Hardware init */
    if (hw_init(cfg->base_addr, cfg->mac_addr) != 0)
        return ERR_IF;

    /* Configure netif */
    netif->name[0] = 'e';
    netif->name[1] = 'n';

    netif->hwaddr_len = ETHARP_HWADDR_LEN;
    memcpy(netif->hwaddr, cfg->mac_addr, ETHARP_HWADDR_LEN);

    netif->mtu = 1500;
    netif->flags = NETIF_FLAG_BROADCAST
                 | NETIF_FLAG_ETHARP
                 | NETIF_FLAG_LINK_UP
                 | NETIF_FLAG_ETHERNET;

#if LWIP_IPV4
    netif->output = etharp_output;
#endif
#if LWIP_IPV6
    netif->output_ip6 = ethip6_output;
#endif
    netif->linkoutput = low_level_output;

    return ERR_OK;
}

void lan9118_lwip_poll(struct netif *netif) {
    struct lan9118_config *cfg = (struct lan9118_config *)netif->state;
    uint32_t base = cfg->base_addr;

    /* Drain all pending packets */
    while (rx_packets_pending(base) > 0) {
        struct pbuf *p = rx_receive(base);
        if (p == NULL)
            break;

        if (netif->input(p, netif) != ERR_OK) {
            pbuf_free(p);
        }
    }
}

int lan9118_lwip_link_is_up(struct netif *netif) {
    struct lan9118_config *cfg = (struct lan9118_config *)netif->state;
    uint16_t bmsr;
    if (phy_read(cfg->base_addr, PHY_BMSR, &bmsr) != 0)
        return 0;
    return (bmsr & BMSR_LSTATUS) ? 1 : 0;
}

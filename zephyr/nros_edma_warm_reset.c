/*
 * Issue 2387 -- quiesce the eDMA channels left running by the image before a
 * warm reset.
 *
 * On the S32K344 a functional reset (SYSRESETREQ: `pyocd reset`, a Zephyr
 * `sys_reboot`, a watchdog's functional path) does not reset the eDMA
 * channels. A channel the previous image used keeps its CH_INT flag and its
 * enabled request (CH_CSR.ERQ) across the reset. Zephyr's eDMA v3 driver
 * (`dma_mcux_edma.c` over the MCUX `dma3` HAL) does not clear them:
 * `EDMA_Init` for eDMA v3 programs MP_CSR only, unlike the older eDMA HAL,
 * which zeroes ERQ, INT and ERR. The driver then enables the channel
 * interrupts, and at the first interrupt unlock (`arch_switch_to_main_thread`)
 * the stale channel interrupt enters `EDMA_HandleIRQ` on a handle that
 * `EDMA_CreateHandle` has not filled yet: a MemManage fault at 0x4008, an
 * eDMA channel register offset off a NULL base. Every later warm boot faulted
 * the same way, of any image; only the reset pin (a destructive reset) cleared
 * it.
 *
 * This runs at PRE_KERNEL_2, after the driver's init (PRE_KERNEL_1) and with
 * interrupts still locked, and for every channel of every okay eDMA v3
 * controller disables the request and clears the DONE, ERR and INT flags,
 * then clears the controller's NVIC pending bits. No channel is in use yet:
 * a channel is configured only by `dma_config`, from thread context.
 */

#include <zephyr/kernel.h>
#include <zephyr/device.h>
#include <zephyr/devicetree.h>
#include <zephyr/init.h>
#include <zephyr/irq.h>
#include <zephyr/sys/util.h>

#include <fsl_edma.h>

#define NROS_EDMA_COMPAT nxp_mcux_edma

/* hw index of logical `ch`, the same rule as the driver's
 * `dma_mcux_edma_add_channel_gap`. */
#define NROS_EDMA_HW_CHANNEL(node, ch)                                                             \
    COND_CODE_1(DT_NODE_HAS_PROP(node, channel_gap),                                               \
                (((ch) < DT_PROP_BY_IDX(node, channel_gap, 0))                                     \
                     ? (ch)                                                                        \
                     : ((ch) + 1 + DT_PROP_BY_IDX(node, channel_gap, 1) -                          \
                        DT_PROP_BY_IDX(node, channel_gap, 0))),                                    \
                (ch))

#define NROS_EDMA_CLEAR_IRQ(idx, node) NVIC_ClearPendingIRQ((IRQn_Type)DT_IRQN_BY_IDX(node, idx));

#define NROS_EDMA_QUIESCE(node)                                                                    \
    COND_CODE_1(                                                                                   \
        DT_NODE_HAS_PROP(node, nxp_version),                                                       \
        (COND_CODE_1(IS_EQ(DT_PROP(node, nxp_version), 3), (nros_edma_quiesce_v3(node);), ())),    \
        ())

#define nros_edma_quiesce_v3(node)                                                                 \
    do {                                                                                           \
        DMA_Type* base = (DMA_Type*)DT_REG_ADDR_BY_IDX(node, 0);                                   \
        for (uint32_t ch = 0; ch < DT_PROP(node, dma_channels); ch++) {                            \
            uint32_t hw = NROS_EDMA_HW_CHANNEL(node, ch);                                          \
            EDMA_DisableChannelRequest(base, hw);                                                  \
            EDMA_ClearChannelStatusFlags(base, hw, 0xFFFFFFFFU);                                   \
        }                                                                                          \
        LISTIFY(DT_NUM_IRQS(node), NROS_EDMA_CLEAR_IRQ, (), node)                                  \
    } while (0)

static int nros_edma_warm_reset_init(void) {
    DT_FOREACH_STATUS_OKAY(NROS_EDMA_COMPAT, NROS_EDMA_QUIESCE)
    return 0;
}

SYS_INIT(nros_edma_warm_reset_init, PRE_KERNEL_2, 0);

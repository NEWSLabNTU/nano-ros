/*
 * board_an536.c — QEMU MPS3-AN536 (dual Cortex-R52) board support.
 *
 * phase-385 W1/W2. Sibling to board_s32z270.c (same CPU, same kernel port)
 * and to board_mps2.c (same NIC, same QEMU conventions). Unlike either, this
 * board is COMPLETE: it implements the tick seam and the netif so a clean
 * checkout produces an image that actually schedules and talks.
 *
 * What this TU owns:
 *   - the ARM vector table + Reset_Handler (EL2->EL1 drop, per-mode stacks,
 *     .data copy, .bss zero, jump to main)
 *   - GICv3 bring-up and the IRQ dispatch the FreeRTOS port delegates
 *   - the tick, from the ARM generic timer (PPI 30)
 *   - the LAN9118 netif strong overrides (same driver the MPS2 board uses)
 *
 * The console is NOT here: the family already provides `_write` and
 * `nros_board_freertos_console_write` over ARM semihosting
 * (nros-board-freertos/c/{freertos_c_entry,freertos_hooks}.c), and QEMU
 * enables it from the runner line. A second `_write` in this TU would be a
 * duplicate strong symbol. The per-CPU UART base is recorded below anyway,
 * because poking it directly is the first thing worth trying when an image
 * goes silent before the scheduler.
 *
 * BOARD FACTS, all measured from the model (`info qtree` / `info mtree`), not
 * assumed — see phase-385 W0:
 *   console UART   0xe7c00000   (the PER-CPU uart; QEMU's serial0. The four
 *                                shared CMSDK uarts at 0xe0205000+ are
 *                                serial1..4 and print nowhere by default,
 *                                which looks exactly like a dead image)
 *   LAN9118        0xe0300000
 *   GICv3 dist     0xf0000000
 *   GICv3 redist   0xf0100000
 *   DDR            0x20000000
 *
 * The CPU resets into HYP (EL2), not PL1 — the single most important find of
 * the W0 spike. `ARM_CRx_No_GIC` is a PL1 port (it does `CPS #SVC_MODE` and
 * uses banked IRQ/SVC stacks), so Reset_Handler drops to EL1 before anything
 * else. Booting a PL1 RTOS at EL2 fails in confusing ways much later.
 */

#include <stdint.h>
#include <string.h>

#include "FreeRTOS.h"
#include "task.h"

#include "lwip/netif.h"
#include "lwip/netifapi.h"
#include "lwip/ip4_addr.h"
#include "lwip/tcpip.h"

#include "lan9118_lwip.h"

/* ---- Board facts ---- */
#define AN536_UART0_BASE   0xe7c00000UL /* per-CPU CMSDK uart == QEMU serial0 */
#define AN536_LAN9118_BASE 0xe0300000UL
#define AN536_GICD_BASE    0xf0000000UL
#define AN536_GICR_BASE    0xf0100000UL

/* Generic timer PPI: EL1 physical timer is INTID 30 on every ARM core. */
#define TIMER_PPI_INTID 30

/* CMSDK APB UART registers. */
#define UART_DATA     0x00
#define UART_STATE    0x04
#define UART_CTRL     0x08
#define UART_BAUDDIV  0x10
#define UART_STATE_TXFULL 0x1u

/* GICv3 distributor. */
#define GICD_CTLR       0x0000
#define GICD_IGROUPR    0x0080
/* GICv3 redistributor: RD_base then SGI_base one 64 KiB frame later. */
#define GICR_WAKER      0x0014
#define GICR_SGI_OFFSET 0x10000
#define GICR_IGROUPR0   (GICR_SGI_OFFSET + 0x0080)
#define GICR_ISENABLER0 (GICR_SGI_OFFSET + 0x0100)
#define GICR_IPRIORITYR (GICR_SGI_OFFSET + 0x0400)

#define GICD_CTLR_ARE_NS  (1u << 4)
#define GICD_CTLR_ENABLE_G1NS (1u << 1)
#define GICR_WAKER_PROCESSOR_SLEEP (1u << 1)
#define GICR_WAKER_CHILDREN_ASLEEP (1u << 2)

static inline void mmio_w32(uintptr_t base, uint32_t off, uint32_t v) {
    *(volatile uint32_t *)(base + off) = v;
}
static inline uint32_t mmio_r32(uintptr_t base, uint32_t off) {
    return *(volatile uint32_t *)(base + off);
}

/* ---- Linker symbols (shared section layout) ---- */
extern uint32_t _estack;

/* ---- Forward declarations ---- */
void Reset_Handler(void);
void Default_Handler(void);
void nros_board_c_startup(void);

extern int main(void);

/* FreeRTOS ARM_CRx_No_GIC port handlers. */
extern void FreeRTOS_IRQ_Handler(void);
extern void FreeRTOS_SVC_Handler(void);

/*
 * The port's end-of-interrupt store lands here. See FreeRTOSConfig.h: this
 * machine has no memory-mapped GIC CPU interface, and the real EOI happens in
 * vApplicationIRQHandler() below, where the matching acknowledge already is.
 */
uint32_t nros_board_eoi_scratch;

/* ---- Vector table ----
 * Classic ARM layout: eight instruction slots, PC-relative so the table is
 * position-independent. Reset_Handler points VBAR at it.
 */
__attribute__((section(".isr_vector"), naked, used)) void vector_table(void) {
    __asm volatile("ldr pc, =Reset_Handler        \n" /* reset */
                   "ldr pc, =Default_Handler      \n" /* undefined instruction */
                   "ldr pc, =FreeRTOS_SVC_Handler \n" /* SVC */
                   "ldr pc, =Default_Handler      \n" /* prefetch abort */
                   "ldr pc, =Default_Handler      \n" /* data abort */
                   "ldr pc, =Default_Handler      \n" /* reserved */
                   "ldr pc, =FreeRTOS_IRQ_Handler \n" /* IRQ */
                   "ldr pc, =Default_Handler      \n" /* FIQ */
    );
}

void Default_Handler(void) {
    for (;;) {
    }
}

/* ---- Reset ----
 * QEMU enters here at the ELF entry point, in HYP mode (EL2) with interrupts
 * masked. Order matters:
 *   1. HYP stack, so the ERET below has somewhere to spill if it faults;
 *   2. drop EL2 -> EL1 (SVC mode) via ERET with SPSR = SVC/IRQ-masked;
 *   3. per-mode stacks for the modes the port actually uses;
 *   4. VBAR -> our table;
 *   5. C startup.
 */
__attribute__((naked, used)) void Reset_Handler(void) {
    __asm volatile(
        /* --- 1. HYP-mode stack (temporary, only for the drop) --- */
        "ldr r0, =_estack               \n"
        "mov sp, r0                     \n"

        /* --- 2. EL2 -> EL1 ---
         * HACTLR: let EL1 touch the implementation-defined registers the
         * kernel/port need (generic timer, IMP_* control) — without this the
         * first EL1 CP15 access traps to EL2 and looks like a corrupt image.
         */
        "mvn r1, #0                     \n"
        "mcr p15, 4, r1, c1, c0, 1      \n" /* HACTLR = 0xffffffff */
        /* Route nothing to EL2: HCR.{AMO,IMO,FMO} clear, so IRQs land at EL1. */
        "mov r1, #0                     \n"
        "mcr p15, 4, r1, c1, c1, 0      \n" /* HCR */
        /* HCPTR: do not trap EL1's CP10/CP11 (the FPU/NEON) to EL2. Must be
         * done HERE, while still at EL2 — after the drop it is unreachable. */
        "mov r1, #0                     \n"
        "mcr p15, 4, r1, c1, c1, 2      \n" /* HCPTR */
        /* Return to SVC mode (0x13) with IRQ+FIQ masked; the scheduler
         * unmasks when it starts the first task. */
        "mov r1, #0x1d3                 \n" /* SPSR_hyp = AIF masked, SVC */
        "msr spsr_hyp, r1               \n"
        "adr r1, 1f                     \n"
        "msr elr_hyp, r1                \n"
        "eret                           \n"

        /* --- 3. Per-mode stacks (now at EL1) --- */
        "1:                             \n"
        "ldr r0, =_estack               \n"
        "cps #0x12                      \n" /* IRQ mode */
        "mov sp, r0                     \n"
        "sub r0, r0, #0x4000            \n" /* 16 KiB IRQ stack */
        "cps #0x17                      \n" /* Abort mode */
        "mov sp, r0                     \n"
        "sub r0, r0, #0x1000            \n"
        "cps #0x1b                      \n" /* Undefined mode */
        "mov sp, r0                     \n"
        "sub r0, r0, #0x1000            \n"
        "cps #0x13                      \n" /* back to SVC: the boot stack */
        "mov sp, r0                     \n"

        /* --- 4. FPU ---
         * The kernel touches NEON before any task runs: `pxPortInitialiseStack`
         * zeroes the FP context with `vmov.i32 d16, #0`. With the FPU off that
         * is an UNDEFINED INSTRUCTION, and the image lands in und32 mode
         * spinning in Default_Handler — which looks like a hang, not a missing
         * enable. (The S32Z270 bundle leaves this to "consumer early-init";
         * this is what that consumer has to write.)
         */
        "mrc p15, 0, r0, c1, c0, 2      \n" /* CPACR */
        "orr r0, r0, #(0xf << 20)       \n" /* full access, CP10 + CP11 */
        "mcr p15, 0, r0, c1, c0, 2      \n"
        "isb                            \n"
        "mov r0, #0x40000000            \n" /* FPEXC.EN */
        "vmsr fpexc, r0                 \n"

        /* --- 5. VBAR -> our vector table --- */
        "ldr r0, =vector_table          \n"
        "mcr p15, 0, r0, c12, c0, 0     \n" /* VBAR */

        /* --- 6. C startup --- */
        "bl  nros_board_c_startup       \n"
        "b   .                          \n");
}

__attribute__((used)) void nros_board_c_startup(void) {
    extern uint32_t _sdata, _edata, _etext, _sbss, _ebss;
    memcpy(&_sdata, &_etext, (size_t)((uintptr_t)&_edata - (uintptr_t)&_sdata));
    memset(&_sbss, 0, (size_t)((uintptr_t)&_ebss - (uintptr_t)&_sbss));
    (void)main();
    for (;;) {
    }
}

/* ---- GICv3 ----
 * Only what a tick needs: enable the distributor's non-secure group 1, wake
 * this core's redistributor, put the timer PPI in group 1 at a middle
 * priority, and enable the CPU interface through the ICC_* system registers.
 *
 * The A32 encodings for the ICC_* registers are CP15 accesses; they are what
 * makes this GICv3 usable at all here, since the machine exposes no
 * memory-mapped CPU interface.
 */
#define ICC_SRE   "p15, 0, %0, c12, c12, 5"
#define ICC_PMR   "p15, 0, %0, c4, c6, 0"
#define ICC_IGRPEN1 "p15, 0, %0, c12, c12, 7"
#define ICC_IAR1  "p15, 0, %0, c12, c12, 0"
#define ICC_EOIR1 "p15, 0, %0, c12, c12, 1"

static inline void icc_write_sre(uint32_t v) { __asm volatile("mcr " ICC_SRE :: "r"(v)); }
static inline uint32_t icc_read_sre(void) {
    uint32_t v;
    __asm volatile("mrc " ICC_SRE : "=r"(v));
    return v;
}
static inline void icc_write_pmr(uint32_t v) { __asm volatile("mcr " ICC_PMR :: "r"(v)); }
static inline void icc_write_igrpen1(uint32_t v) { __asm volatile("mcr " ICC_IGRPEN1 :: "r"(v)); }
static inline uint32_t icc_read_iar1(void) {
    uint32_t v;
    __asm volatile("mrc " ICC_IAR1 : "=r"(v));
    return v;
}
static inline void icc_write_eoir1(uint32_t v) { __asm volatile("mcr " ICC_EOIR1 :: "r"(v)); }

static void gicv3_init(void) {
    /* Distributor: affinity routing + non-secure group 1. */
    mmio_w32(AN536_GICD_BASE, GICD_CTLR, GICD_CTLR_ARE_NS | GICD_CTLR_ENABLE_G1NS);

    /* Redistributor: clear ProcessorSleep and wait for ChildrenAsleep to
     * follow, or the core never sees an interrupt. */
    uint32_t waker = mmio_r32(AN536_GICR_BASE, GICR_WAKER);
    mmio_w32(AN536_GICR_BASE, GICR_WAKER, waker & ~GICR_WAKER_PROCESSOR_SLEEP);
    while (mmio_r32(AN536_GICR_BASE, GICR_WAKER) & GICR_WAKER_CHILDREN_ASLEEP) {
    }

    /* The timer PPI: group 1, mid priority, enabled. SGI/PPI registers live
     * in the redistributor's SGI frame, not the distributor. */
    mmio_w32(AN536_GICR_BASE, GICR_IGROUPR0, 0xffffffffu);
    *(volatile uint8_t *)(AN536_GICR_BASE + GICR_IPRIORITYR + TIMER_PPI_INTID) = 0xa0;
    mmio_w32(AN536_GICR_BASE, GICR_ISENABLER0, 1u << TIMER_PPI_INTID);

    /* CPU interface: system-register access, then let every priority through
     * and enable group 1. */
    icc_write_sre(icc_read_sre() | 1u);
    __asm volatile("isb");
    icc_write_pmr(0xffu);
    icc_write_igrpen1(1u);
    __asm volatile("isb");
}

/* ---- Generic timer tick ----
 * CNTP (EL1 physical timer): read CNTFRQ rather than trusting a constant,
 * program CNTP_TVAL for one tick period, enable. Each expiry re-arms in
 * nros_board_clear_tick_interrupt(), which the port calls from the tick ISR.
 */
static uint32_t tick_interval;

static inline uint32_t read_cntfrq(void) {
    uint32_t v;
    __asm volatile("mrc p15, 0, %0, c14, c0, 0" : "=r"(v));
    return v;
}
static inline void write_cntp_tval(uint32_t v) {
    __asm volatile("mcr p15, 0, %0, c14, c2, 0" :: "r"(v));
}
static inline void write_cntp_ctl(uint32_t v) {
    __asm volatile("mcr p15, 0, %0, c14, c2, 1" :: "r"(v));
}

void nros_board_setup_tick_interrupt(void) {
    uint32_t freq = read_cntfrq();
    if (freq == 0) {
        /* QEMU always reports one; a zero would silently mean "no ticks". */
        freq = NROS_BOARD_CPU_CLOCK_HZ;
    }
    tick_interval = freq / configTICK_RATE_HZ;

    gicv3_init();

    write_cntp_tval(tick_interval);
    write_cntp_ctl(1u); /* enable, unmasked */
    __asm volatile("isb");
}

void nros_board_clear_tick_interrupt(void) {
    /* Re-arm for the next period. TVAL is a down-counter, so writing the
     * interval again both clears the pending condition and schedules. */
    write_cntp_tval(tick_interval);
    __asm volatile("isb");
}

/* ---- IRQ dispatch ----
 * The port calls this with no argument and does not acknowledge, so the
 * acknowledge/EOI pair lives here (see FreeRTOSConfig.h). Only the tick is
 * wired; anything else is drained so a stray interrupt cannot wedge the CPU.
 */
extern void FreeRTOS_Tick_Handler(void);

__attribute__((used)) void vApplicationIRQHandler(void) {
    uint32_t iar = icc_read_iar1();
    uint32_t intid = iar & 0xffffffu;

    if (intid == TIMER_PPI_INTID) {
        FreeRTOS_Tick_Handler();
    }

    /* 1020-1023 are the spurious/special IDs: no EOI is owed for them. */
    if (intid < 1020u) {
        icc_write_eoir1(iar);
    }
}

/* ---- Network ----
 * Strong overrides of the weak hooks in nros-board-freertos's network_glue.c,
 * exactly as the MPS2 board does — the same LAN9118 driver, a different base
 * address. Poll mode: no NIC interrupt is wired, so the GIC is needed for the
 * tick alone.
 */
struct netif lan9118_netif;
struct lan9118_config lan9118_cfg;

int nros_board_register_netif(
    const uint8_t mac[6],
    const uint8_t ip[4],
    const uint8_t netmask[4],
    const uint8_t gw[4])
{
    ip4_addr_t ipaddr, mask, gateway;

    IP4_ADDR(&ipaddr,  ip[0], ip[1], ip[2], ip[3]);
    IP4_ADDR(&mask,    netmask[0], netmask[1], netmask[2], netmask[3]);
    IP4_ADDR(&gateway, gw[0], gw[1], gw[2], gw[3]);

    /* Same driver as the MPS2 sibling; only the base address differs, which
     * is exactly why `lan9118_config` carries one. */
    lan9118_cfg.base_addr = AN536_LAN9118_BASE;
    memcpy(lan9118_cfg.mac_addr, mac, 6);

    /* netifapi (thread-safe: runs in tcpip_thread). netif_add() does NOT set
     * netif_default even with LWIP_SINGLE_NETIF — set it explicitly. */
    if (netifapi_netif_add(&lan9118_netif, &ipaddr, &mask, &gateway,
                           &lan9118_cfg, lan9118_lwip_init, tcpip_input) != ERR_OK) {
        return -1;
    }

    netifapi_netif_set_default(&lan9118_netif);
    netifapi_netif_set_up(&lan9118_netif);
    netifapi_netif_set_link_up(&lan9118_netif);
    return 0;
}

void nros_board_poll_netif(void) {
    lan9118_lwip_poll(&lan9118_netif);
}

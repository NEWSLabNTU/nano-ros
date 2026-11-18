/*
 * board_mps2.c — everything about this board that is not shared.
 *
 * phase-337 W5 — this is the whole per-board C surface, and BOTH lanes (cargo
 * and CMake) compile it. It contains, and should only ever contain:
 *   - the Cortex-M3 vector table (`isr_vector`)
 *   - `Reset_Handler` (data copy + bss zero + jump to `main`)
 *   - `Default_Handler` (infinite loop for unhandled IRQs)
 *   - the LAN9118 netif registration + poll (the strong overrides for
 *     `network_glue.c`'s weak `nros_board_*` hooks)
 *
 * W5.c removed ~~`nros_freertos_diag_network`~~ — ~180 lines of raw LAN9118 CSR
 * pokes and a hand-assembled ARP frame, duplicated into the retired
 * `startup.c` as well, and called from no path in either lane. The technique
 * it demonstrated is written up in
 * `book-internals/src/testing/lan9118-reference.md`, which is where a debugging aid
 * belongs.
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

/* Drain-loop hooks in nros-board-freertos/c/network_glue.c (issue 0917). */
void nros_freertos_net_rx_backlog(void);

/* ---- Linker symbols ---- */
extern uint32_t _etext;
extern uint32_t _sdata;
extern uint32_t _edata;
extern uint32_t _sbss;
extern uint32_t _ebss;
extern uint32_t _estack;

/* ---- Forward declarations ---- */
void Reset_Handler(void);
void Default_Handler(void);
void HardFault_Handler(void);
void SysTick_Handler(void); /* defined in freertos_hooks.c */

/* Issue 2696 — the 32 external interrupts of the AN385 (`mps2_an385_pac`'s
 * order, NVIC 0..31). Each is a weak alias of `Default_Handler`, so an image
 * overrides one by defining it (`TIMER1_IRQHandler`). An image that
 * defines none behaves as before: an unexpected interrupt still lands in
 * `Default_Handler`. A handler that calls a FreeRTOS `...FromISR` API needs a
 * priority numerically at or above `configMAX_SYSCALL_INTERRUPT_PRIORITY`. */
void UARTRX0_IRQHandler(void) __attribute__((weak, alias("Default_Handler")));
void UARTTX0_IRQHandler(void) __attribute__((weak, alias("Default_Handler")));
void UARTRX1_IRQHandler(void) __attribute__((weak, alias("Default_Handler")));
void UARTTX1_IRQHandler(void) __attribute__((weak, alias("Default_Handler")));
void UARTRX2_IRQHandler(void) __attribute__((weak, alias("Default_Handler")));
void UARTTX2_IRQHandler(void) __attribute__((weak, alias("Default_Handler")));
void GPIO0ALL_IRQHandler(void) __attribute__((weak, alias("Default_Handler")));
void GPIO1ALL_IRQHandler(void) __attribute__((weak, alias("Default_Handler")));
void TIMER0_IRQHandler(void) __attribute__((weak, alias("Default_Handler")));
void TIMER1_IRQHandler(void) __attribute__((weak, alias("Default_Handler")));
void DUALTIMER_IRQHandler(void) __attribute__((weak, alias("Default_Handler")));
void SPI_IRQHandler(void) __attribute__((weak, alias("Default_Handler")));
void UARTOVF_IRQHandler(void) __attribute__((weak, alias("Default_Handler")));
void ETHERNET_IRQHandler(void) __attribute__((weak, alias("Default_Handler")));
void I2S_IRQHandler(void) __attribute__((weak, alias("Default_Handler")));
void TSC_IRQHandler(void) __attribute__((weak, alias("Default_Handler")));
void GPIO2_IRQHandler(void) __attribute__((weak, alias("Default_Handler")));
void GPIO3_IRQHandler(void) __attribute__((weak, alias("Default_Handler")));
void UARTRX3_IRQHandler(void) __attribute__((weak, alias("Default_Handler")));
void UARTTX3_IRQHandler(void) __attribute__((weak, alias("Default_Handler")));
void UARTRX4_IRQHandler(void) __attribute__((weak, alias("Default_Handler")));
void UARTTX4_IRQHandler(void) __attribute__((weak, alias("Default_Handler")));
void ADCSPI_IRQHandler(void) __attribute__((weak, alias("Default_Handler")));
void SHIELDSPI_IRQHandler(void) __attribute__((weak, alias("Default_Handler")));
void GPIO0_0_IRQHandler(void) __attribute__((weak, alias("Default_Handler")));
void GPIO0_1_IRQHandler(void) __attribute__((weak, alias("Default_Handler")));
void GPIO0_2_IRQHandler(void) __attribute__((weak, alias("Default_Handler")));
void GPIO0_3_IRQHandler(void) __attribute__((weak, alias("Default_Handler")));
void GPIO0_4_IRQHandler(void) __attribute__((weak, alias("Default_Handler")));
void GPIO0_5_IRQHandler(void) __attribute__((weak, alias("Default_Handler")));
void GPIO0_6_IRQHandler(void) __attribute__((weak, alias("Default_Handler")));
void GPIO0_7_IRQHandler(void) __attribute__((weak, alias("Default_Handler")));

/* FreeRTOS port handlers — installed directly in the vector table.
 * FreeRTOS asserts that these exact function pointers appear in the
 * vector table, so wrapper functions are not allowed. */
extern void xPortPendSVHandler(void);
extern void vPortSVCHandler(void);

/* Firmware entry point.
 *
 * phase-337 W5.b — ONE symbol for both lanes: on the Rust lane `main` is the
 * `#[unsafe(no_mangle)] pub extern "C" fn main() -> i32` the Entry pkg emits;
 * on the C/C++ lane it is `nros-board-freertos/c/freertos_c_entry.c::main`.
 * The retired `startup.c` had its own `Reset_Handler` calling its own
 * `_start`, which is exactly how the shadow copy stayed alive.
 *
 * Phase 212.M-F.15 — the firmware binary's entry point is the standard
 * `#[unsafe(no_mangle)] pub extern "C" fn main() -> i32` symbol emitted
 * by the Phase 212.N Entry pkg shape (`<Board as BoardEntry>::run(...)`
 * → see `examples/mps2-an385-freertos/rust/<role>_entry/src/main.rs`). The
 * legacy `_start` shape used by the pre-N.7 M.5.a baker fixture was
 * retired together with the `freertos-qemu-mps2-an385-bsp` crate
 * (commit `d99386173`); calling `_start` from `Reset_Handler` left a
 * `rust-lld: error: undefined symbol: _start` regression that this
 * Phase 212.M-F.15 fix closes.
 */
extern int main(void);

/* ---- LAN9118 netif globals (152.1.B.2 lift) ---- *
 * Phase 152.1.B.2 — these lived in `network_glue.c` until 152.1.B.1;
 * 152.1.B.2 moved them into the board-specific TU together with
 * the strong `nros_board_register_netif` + `nros_board_poll_netif`
 * implementations the generic glue invokes through its weak hooks. */
struct netif lan9118_netif;
struct lan9118_config lan9118_cfg;

/* ---- Strong overrides for the generic network_glue.c hooks ---- */

int nros_board_register_netif(const uint8_t mac[6], const uint8_t ip[4], const uint8_t netmask[4],
                              const uint8_t gw[4]) {
    ip4_addr_t ipaddr, mask, gateway;

    IP4_ADDR(&ipaddr, ip[0], ip[1], ip[2], ip[3]);
    IP4_ADDR(&mask, netmask[0], netmask[1], netmask[2], netmask[3]);
    IP4_ADDR(&gateway, gw[0], gw[1], gw[2], gw[3]);

    lan9118_cfg.base_addr = LAN9118_BASE_DEFAULT;
    memcpy(lan9118_cfg.mac_addr, mac, 6);

    /* Register netif via netifapi (thread-safe: executes in
     * tcpip_thread). netif_add() does NOT set netif_default even
     * with LWIP_SINGLE_NETIF; call netif_set_default() explicitly. */
    if (netifapi_netif_add(&lan9118_netif, &ipaddr, &mask, &gateway, &lan9118_cfg,
                           lan9118_lwip_init, tcpip_input) != ERR_OK) {
        return -1;
    }

    netifapi_netif_set_default(&lan9118_netif);
    netifapi_netif_set_up(&lan9118_netif);
    netifapi_netif_set_link_up(&lan9118_netif);
    return 0;
}

void nros_board_poll_netif(void) {
    lan9118_lwip_poll(&lan9118_netif);
    /* No RX interrupt is wired here, so a drain that stopped at its budget
     * would otherwise wait the full poll interval with frames in the FIFO.
     * Issue 0917. */
    if (lan9118_lwip_rx_pending(&lan9118_netif)) {
        nros_freertos_net_rx_backlog();
    }
}

/* ---- Interrupt vector table ---- */
typedef void (*vector_fn)(void);

__attribute__((section(".isr_vector"), used)) const vector_fn isr_vector[] = {
    (vector_fn)(uintptr_t)&_estack, /* Initial MSP */
    Reset_Handler,
    Default_Handler, /* NMI */
    HardFault_Handler,
    Default_Handler, /* MemManage */
    Default_Handler, /* BusFault */
    Default_Handler, /* UsageFault */
    0,
    0,
    0,
    0, /* Reserved */
    vPortSVCHandler,
    Default_Handler, /* DebugMon */
    0,               /* Reserved */
    xPortPendSVHandler,
    SysTick_Handler,
    /* External interrupts (issue 2696). */
    UARTRX0_IRQHandler, /* IRQ 0 */
    UARTTX0_IRQHandler, /* IRQ 1 */
    UARTRX1_IRQHandler, /* IRQ 2 */
    UARTTX1_IRQHandler, /* IRQ 3 */
    UARTRX2_IRQHandler, /* IRQ 4 */
    UARTTX2_IRQHandler, /* IRQ 5 */
    GPIO0ALL_IRQHandler, /* IRQ 6 */
    GPIO1ALL_IRQHandler, /* IRQ 7 */
    TIMER0_IRQHandler, /* IRQ 8 */
    TIMER1_IRQHandler, /* IRQ 9 */
    DUALTIMER_IRQHandler, /* IRQ 10 */
    SPI_IRQHandler, /* IRQ 11 */
    UARTOVF_IRQHandler, /* IRQ 12 */
    ETHERNET_IRQHandler, /* IRQ 13 */
    I2S_IRQHandler, /* IRQ 14 */
    TSC_IRQHandler, /* IRQ 15 */
    GPIO2_IRQHandler, /* IRQ 16 */
    GPIO3_IRQHandler, /* IRQ 17 */
    UARTRX3_IRQHandler, /* IRQ 18 */
    UARTTX3_IRQHandler, /* IRQ 19 */
    UARTRX4_IRQHandler, /* IRQ 20 */
    UARTTX4_IRQHandler, /* IRQ 21 */
    ADCSPI_IRQHandler, /* IRQ 22 */
    SHIELDSPI_IRQHandler, /* IRQ 23 */
    GPIO0_0_IRQHandler, /* IRQ 24 */
    GPIO0_1_IRQHandler, /* IRQ 25 */
    GPIO0_2_IRQHandler, /* IRQ 26 */
    GPIO0_3_IRQHandler, /* IRQ 27 */
    GPIO0_4_IRQHandler, /* IRQ 28 */
    GPIO0_5_IRQHandler, /* IRQ 29 */
    GPIO0_6_IRQHandler, /* IRQ 30 */
    GPIO0_7_IRQHandler, /* IRQ 31 */
};

/* ---- Reset handler ---- */
void Reset_Handler(void) {
    /* Copy .data from flash to RAM */
    uint32_t* src = &_etext;
    uint32_t* dst = &_sdata;
    while (dst < &_edata) {
        *dst++ = *src++;
    }
    /* Zero .bss */
    dst = &_sbss;
    while (dst < &_ebss) {
        *dst++ = 0;
    }
    /* Jump to the firmware entry. `main` returns `i32`; ignore the value here
     * — `BoardEntry::run` is divergent in practice (FreeRTOS scheduler
     * never returns under normal operation; `exit_success`/`failure`
     * trigger semihosting exit). The trailing `for(;;)` keeps the
     * Cortex-M3 from executing garbage instructions if we ever do
     * fall through. */
    (void)main();
    for (;;) {
    }
}

/* ---- Default handler (infinite loop) ---- */
void Default_Handler(void) {
    for (;;) {
    }
}

/* ---- Task stack guard (issue 2507) ----
 *
 * Task stacks are heap_4 blocks (or `.bss` arrays), so the memory below a
 * stack is another heap block's header. An overflow used to write over it,
 * and the next `pvPortMalloc` reported `*** MALLOC FAILED ***`: the kernel's
 * own pattern check (configCHECK_FOR_STACK_OVERFLOW 2) runs only at a context
 * switch, and the heap usually failed first.
 *
 * So on every switch the kernel calls `nros_board_task_stack_guard_arm` with
 * the incoming task's lowest stack address (`portTASK_SWITCH_HOOK`), and MPU
 * region 0 is programmed over the lowest GUARD_BYTES of that stack, aligned
 * up. The region is READ-ONLY for privileged code, not no-access: the kernel's
 * pattern check and `uxTaskGetStackHighWaterMark` read those bytes, and a
 * write is what an overflow does. The first store into the guard faults, on
 * the overflowing instruction, before anything below the stack is touched.
 *
 * - Nothing else on this board programs the MPU (the GCC ARM_CM3 port is the
 *   non-MPU port), so region 0 is free. PRIVDEFENA keeps the default memory
 *   map for every other address.
 * - MemManage stays disabled, so the fault escalates to HardFault. With
 *   MPU_CTRL.HFNMIENA clear the MPU is off while HardFault runs, and the
 *   handler runs on the main stack, so it can report.
 * - The first task runs unguarded until the first switch: the scheduler's
 *   start does not call the hook.
 * - A single frame larger than the guard can store below it without touching
 *   it. GUARD_BYTES trades stack for that reach.
 */
#if defined(NROS_BOARD_TASK_STACK_GUARD) && NROS_BOARD_TASK_STACK_GUARD

#define NROS_GUARD_BYTES 256u

#define MPU_TYPE (*(volatile uint32_t*)0xE000ED90u)
#define MPU_CTRL (*(volatile uint32_t*)0xE000ED94u)
#define MPU_RBAR (*(volatile uint32_t*)0xE000ED9Cu)
#define MPU_RASR (*(volatile uint32_t*)0xE000EDA0u)
#define SCB_CFSR (*(volatile uint32_t*)0xE000ED28u)
#define SCB_MMFAR (*(volatile uint32_t*)0xE000ED34u)

/* MPU_CTRL: ENABLE | PRIVDEFENA, HFNMIENA clear. */
#define MPU_CTRL_ENABLE_PRIVDEFENA 0x5u
/* MPU_RBAR: VALID, region 0. */
#define MPU_RBAR_VALID_REGION0 (1u << 4)
/* MPU_RASR: XN, AP=0b101 (privileged read-only, unprivileged no access),
 * TEX=0b001 C=1 B=1 (normal memory, write-back, as the SRAM's default map),
 * SIZE = log2(NROS_GUARD_BYTES) - 1, ENABLE. */
#define MPU_RASR_GUARD                                                                             \
    ((1u << 28) | (0x5u << 24) | (0x1u << 19) | (1u << 17) | (1u << 16) |                          \
     ((uint32_t)(__builtin_ctz(NROS_GUARD_BYTES) - 1) << 1) | 1u)

/* CFSR MemManage bits: MMARVALID, MSTKERR (exception stacking). */
#define CFSR_MMARVALID (1u << 7)
#define CFSR_MSTKERR (1u << 4)

/* 0: not yet probed; 1: armed; 2: this core has no MPU. */
static uint8_t nros_guard_state;
/* The armed guard's base, for the fault handler (0 until the first arm). */
static volatile uint32_t nros_guard_lo;

/* The first arm: probe for an MPU, then place the region, set its size and
 * permissions, and turn the MPU on. Every later switch only moves its base. */
__attribute__((noinline)) static void nros_board_task_stack_guard_enable(uint32_t base) {
    if (((MPU_TYPE >> 8) & 0xffu) == 0u) {
        nros_guard_state = 2u;
        return;
    }
    nros_guard_lo = base;
    MPU_RBAR = base | MPU_RBAR_VALID_REGION0;
    MPU_RASR = MPU_RASR_GUARD;
    MPU_CTRL = MPU_CTRL_ENABLE_PRIVDEFENA;
    __asm__ volatile("dsb" ::: "memory");
    nros_guard_state = 1u;
}

/* Runs in PendSV on every switch: a base computation, one register write and
 * a barrier. No ISB: the exception return that resumes the incoming task
 * synchronises the context. */
void nros_board_task_stack_guard_arm(const void* stack_lowest) {
    uint32_t base =
        ((uint32_t)(uintptr_t)stack_lowest + NROS_GUARD_BYTES - 1u) & ~(NROS_GUARD_BYTES - 1u);
    if (__builtin_expect(nros_guard_state != 1u, 0)) {
        if (nros_guard_state == 0u) {
            nros_board_task_stack_guard_enable(base);
        }
        return;
    }
    nros_guard_lo = base;
    /* RBAR with VALID set also selects the region, so the base moves in one
     * write and the size and permissions written at enable stay. */
    MPU_RBAR = base | MPU_RBAR_VALID_REGION0;
    __asm__ volatile("dsb" ::: "memory");
}

void semihosting_write0(const char* s); /* freertos_hooks.c */

static void nros_semihost_exit_failure(void) {
    /* SYS_EXIT with ADP_Stopped_RunTimeErrorUnknown: QEMU exits non-zero. */
    register uint32_t op __asm__("r0") = 0x18u;
    register uint32_t reason __asm__("r1") = 0x20023u;
    __asm__ volatile("bkpt #0xAB" : : "r"(op), "r"(reason) : "memory");
}

void HardFault_Handler(void) {
    uint32_t cfsr = SCB_CFSR;
    uint32_t psp;
    __asm__ volatile("mrs %0, psp" : "=r"(psp));
    uint32_t lo = nros_guard_lo;
    uint32_t hi = lo == 0u ? 0u : lo + NROS_GUARD_BYTES;
    int in_guard = 0;
    if (hi != 0u) {
        if ((cfsr & CFSR_MMARVALID) && SCB_MMFAR >= lo && SCB_MMFAR < hi) {
            in_guard = 1;
        } else if ((cfsr & CFSR_MSTKERR) && psp < hi) {
            in_guard = 1;
        }
    }
    if (in_guard) {
        semihosting_write0("*** STACK OVERFLOW: ");
        semihosting_write0(pcTaskGetName(NULL));
        semihosting_write0(" *** (the task's stack hit its guard)\n");
    } else {
        semihosting_write0("*** HARD FAULT ***\n");
    }
    nros_semihost_exit_failure();
    for (;;) {
    }
}

#else

void HardFault_Handler(void) {
    for (;;) {
    }
}

#endif /* NROS_BOARD_TASK_STACK_GUARD */

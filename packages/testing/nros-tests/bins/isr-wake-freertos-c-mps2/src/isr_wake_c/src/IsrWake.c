/// @file IsrWake.c
/// @brief A C interrupt handler wakes a parked executor through a device
/// completion handle, on FreeRTOS + MPS2-AN385.
///
/// The component registers a device event source with
/// `nros_executor_register_device` and keeps the completion handle where the
/// CMSDK TIMER1 handler can reach it. Every PERIOD_US the handler calls
/// `nros_completion_signal_from_isr`, the one call a port may make in an
/// interrupt. It reaches FreeRTOS's `xSemaphoreGiveFromISR` and a yield. The
/// generated entry's executor has nothing else to do, so only the interrupt
/// can end its park.
///
/// The continuation reads how long ago the interrupt fired from TIMER1's own
/// counter, which reloads at the interrupt, so the measurement needs no clock
/// that is safe to read from an interrupt. It prints `C_ISR_WAKE <latency_us>`
/// for each wake, and after SAMPLES wakes `C_ISR_WAKE_DONE <interrupts>
/// <continuations>`. The host test holds the latencies to a bound.

#include <stddef.h>
#include <stdint.h>
#include <stdio.h>

#include <nros/device_hook.h>
#include <rclc/rclc.h>

#define PERIOD_US 20000u
#define SAMPLES 50u

/* CMSDK APB TIMER1 (NVIC 9), clocked at the 25 MHz system clock. */
#define TIMER1_BASE 0x40001000u
#define TIMER_CTRL 0x00u
#define TIMER_VALUE 0x04u
#define TIMER_RELOAD 0x08u
#define TIMER_INTCLEAR 0x0Cu
#define CTRL_EN (1u << 0)
#define CTRL_INTEN (1u << 3)
#define TICKS_PER_US 25u
#define RELOAD_TICKS (PERIOD_US * TICKS_PER_US)
#define TIMER1_IRQ 9u

typedef struct {
    size_t handle;
    uint32_t continuations;
    /* The first continuation is not a parked wake: the timer starts in
     * configure, before the entry's first spin, so that one measures start-up
     * (14 ms measured), not the interrupt-safe wake. It is skipped. */
    uint32_t warm;
} isr_wake_t;

/* The handle the interrupt handler signals, as a port's would. */
static nros_completion_t s_done;
static volatile uint32_t s_fires;

static void timer_write(uint32_t off, uint32_t v) {
    *(volatile uint32_t*)(TIMER1_BASE + off) = v;
}

static uint32_t timer_read(uint32_t off) {
    return *(volatile uint32_t*)(TIMER1_BASE + off);
}

/* The board's vector table names this handler (a weak alias of the default
 * handler until defined here). */
void TIMER1_IRQHandler(void);
void TIMER1_IRQHandler(void) {
    timer_write(TIMER_INTCLEAR, 1u);
    s_fires++;
    nros_completion_signal_from_isr(s_done);
}

/* TIMER1 and the NVIC exist only on the Cortex-M3 this image runs on. The
 * build's metadata probe runs this component's configure on the HOST, where
 * these addresses are not mapped, so it registers the device and starts no
 * hardware. */
#if defined(__ARM_ARCH_7M__)
#define HAVE_TIMER1 1
#else
#define HAVE_TIMER1 0
#endif

static void start_timer(void) {
#if !HAVE_TIMER1
    return;
#else
    /* NVIC: priority 6 (of 8), below configMAX_SYSCALL_INTERRUPT_PRIORITY's 5
     * as FreeRTOS requires of a handler that calls a ...FromISR API; then
     * enable. IPR is byte-addressed, top 3 bits significant. */
    *(volatile uint8_t*)(0xE000E400u + TIMER1_IRQ) = (uint8_t)(6u << 5);
    *(volatile uint32_t*)0xE000E100u = 1u << TIMER1_IRQ;
    timer_write(TIMER_CTRL, 0u);
    timer_write(TIMER_RELOAD, RELOAD_TICKS);
    timer_write(TIMER_VALUE, RELOAD_TICKS);
    timer_write(TIMER_INTCLEAR, 1u);
    timer_write(TIMER_CTRL, CTRL_EN | CTRL_INTEN);
#endif
}

/* The device's continuation, run by the executor in thread context. */
static void on_device_done(void* context) {
    isr_wake_t* self = (isr_wake_t*)context;
    /* Ticks since the interrupt reloaded TIMER1: how late this ran. */
    uint32_t value = HAVE_TIMER1 ? timer_read(TIMER_VALUE) : RELOAD_TICKS;
    uint32_t elapsed = value <= RELOAD_TICKS ? RELOAD_TICKS - value : 0u;
    if (!self->warm) {
        self->warm = 1u;
        return;
    }
    if (self->continuations >= SAMPLES) {
        return;
    }
    self->continuations++;
    printf("C_ISR_WAKE %lu\n", (unsigned long)(elapsed / TICKS_PER_US));
    if (self->continuations == SAMPLES) {
        printf("C_ISR_WAKE_DONE %lu %lu\n", (unsigned long)s_fires,
               (unsigned long)self->continuations);
    }
}

static rcl_ret_t isr_wake_configure(rcl_node_t* node, rclc_executor_t* executor, isr_wake_t* self) {
    (void)node;
    self->continuations = 0;
    self->warm = 0;
    rcl_ret_t rc =
        nros_executor_register_device(executor, on_device_done, self, &self->handle, &s_done);
    if (rc != RCL_RET_OK) {
        printf("C_ISR_WAKE_REFUSED %d\n", (int)rc);
        return rc;
    }
    printf("C_ISR_WAKE_READY period_us=%u\n", PERIOD_US);
    start_timer();
    return RCL_RET_OK;
}

RCLC_C_COMPONENT(isr_wake_t, isr_wake_configure)

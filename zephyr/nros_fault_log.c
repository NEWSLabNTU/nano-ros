/*
 * Post-mortem fault log — phase-392 amendment D.
 *
 * WHY THIS EXISTS, precisely.
 *
 * The MR-CANHUBK344 could not be observed without being disturbed. Attaching
 * pyocd's RTT to read the board's console halts the core often enough that the
 * zenoh session misses its keepalives and expires; issue 0879 then makes that
 * permanent, because the serial link cannot resynchronise after the peer goes
 * quiet. Measured: with no probe attached the ROS graph resolves on every
 * query; with the probe attached it resolves on none, and it does not come back
 * when the probe detaches -- only a router restart recovers it.
 *
 * So the debugger is not a passive instrument here. Any fault we can only see
 * by attaching it is a fault we have partly caused.
 *
 * This records the fault WITHOUT a debugger. The cost to SRAM is one small
 * `.noinit` record; the durable copy lives in flash, of which this board uses
 * 8.3 % of 4 MiB.
 *
 * WHY TWO STAGES.
 *
 * The handler runs in fault context: interrupts are off, the faulting stack may
 * be the one that is damaged, and the flash driver wants neither. So the
 * handler only fills a `.noinit` struct -- a few stores, no calls that can
 * fail -- and the flash write happens at the NEXT boot, from thread context,
 * where it is safe and where a failure can be reported.
 *
 * WHAT THIS DOES NOT SURVIVE, measured on the S32K344 and NOT a general Zephyr
 * property.
 *
 * `.noinit` is not zeroed by Zephyr, but this SoC zeroes it below Zephyr. Its
 * `soc_early_reset_hook` (`soc/nxp/s32/s32k3/s32k3xx_startup.S`) must
 * initialise SRAM and both TCMs to a known value after a DESTRUCTIVE reset,
 * because ECC-protected memory cannot be read by a 32-bit master until a 64-bit
 * master has written it. So:
 *
 *     destructive reset (power-on, pin reset)  -> SRAM, ITCM and DTCM ZEROED
 *     functional reset  (SYSRESETREQ)          -> contents retained
 *
 * Verified: after a fault, `pyocd reset` (a hardware reset by default) produced
 * a clean boot with no record, because the ECC init had already run over it.
 *
 * The consequence is that `.noinit` alone covers only the functional-reset case,
 * and the flash copy below is therefore NOT a nice-to-have on this part -- it is
 * the only thing that survives a destructive reset. But it is written at the
 * NEXT boot from a record that a destructive reset has already destroyed, so as
 * written this file does not yet close that gap.
 *
 * Closing it means writing to flash FROM THE FAULT HANDLER, which is a
 * different risk profile: a flash write with interrupts off, on a stack that
 * may be the damaged one. That is the follow-up, and it is deliberately not
 * smuggled in here. Until then this is honest about its coverage:
 *
 *     fault -> functional reset  -> record reported, and persisted   [works]
 *     fault -> destructive reset -> record already gone              [GAP]
 */

#include <zephyr/kernel.h>
#include <zephyr/init.h>
#include <zephyr/fatal.h>
#include <zephyr/sys/printk.h>
#include <zephyr/storage/flash_map.h>
#include <zephyr/drivers/flash.h>
#include <string.h>

#define NROS_FAULT_MAGIC 0x4E524F53u /* "NROS" */

/* Bumped when the record layout changes, so a stale flash record from an older
 * image is reported as unreadable rather than mis-decoded into plausible
 * nonsense. */
#define NROS_FAULT_VERSION 1u

struct nros_fault_record {
    uint32_t magic;
    uint32_t version;
    uint32_t reason;       /* K_ERR_* */
    uint32_t pc;           /* faulting instruction */
    uint32_t lr;
    uint32_t psr;
    uint32_t uptime_ms;
    char     thread[24];   /* CONFIG_THREAD_NAME, else "?" */
};

/* NOT zeroed at boot — that is the whole mechanism. */
static struct nros_fault_record nros_fault_scratch __attribute__((section(".noinit")));

static const char *nros_fault_reason_str(uint32_t reason)
{
    switch (reason) {
    case K_ERR_CPU_EXCEPTION:      return "CPU exception";
    case K_ERR_SPURIOUS_IRQ:       return "spurious IRQ";
    case K_ERR_STACK_CHK_FAIL:     return "STACK OVERFLOW";
    case K_ERR_KERNEL_OOPS:        return "kernel oops";
    case K_ERR_KERNEL_PANIC:       return "kernel panic";
    default:                       return "unknown";
    }
}

/* Overrides the weak symbol in the kernel. Keep this SHORT and call-free where
 * possible: the stack under it may be the one that overflowed. */
void k_sys_fatal_error_handler(unsigned int reason, const struct arch_esf *esf)
{
    nros_fault_scratch.magic = NROS_FAULT_MAGIC;
    nros_fault_scratch.version = NROS_FAULT_VERSION;
    nros_fault_scratch.reason = (uint32_t) reason;
    nros_fault_scratch.uptime_ms = (uint32_t) k_uptime_get_32();

#if defined(CONFIG_ARM)
    if (esf != NULL) {
        nros_fault_scratch.pc = esf->basic.pc;
        nros_fault_scratch.lr = esf->basic.lr;
        nros_fault_scratch.psr = esf->basic.xpsr;
    }
#else
    ARG_UNUSED(esf);
#endif

    /* The thread name is the single most useful field for a stack overflow --
     * "which stack" is the whole question -- so it is worth the strncpy. */
    const char *name = NULL;
#if defined(CONFIG_THREAD_NAME)
    name = k_thread_name_get(k_current_get());
#endif
    if (name == NULL) {
        name = "?";
    }
    strncpy(nros_fault_scratch.thread, name, sizeof(nros_fault_scratch.thread) - 1U);
    nros_fault_scratch.thread[sizeof(nros_fault_scratch.thread) - 1U] = '\0';

    /* Hand back to Zephyr, which halts. The record is already written, so it
     * does not matter what the default policy does from here. */
    k_fatal_halt(reason);
}

/* ---- boot-time report + durable copy ------------------------------------ */

#if defined(CONFIG_NROS_FAULT_LOG_FLASH)
#define NROS_FAULT_FLASH_ID FIXED_PARTITION_ID(storage_partition)

static void nros_fault_persist(const struct nros_fault_record *rec)
{
    const struct flash_area *fa;
    int rc = flash_area_open(NROS_FAULT_FLASH_ID, &fa);
    if (rc != 0) {
        printk("nros: fault log: flash_area_open failed (%d)\n", rc);
        return;
    }
    /* One slot at offset 0. Erase then write: a fault record is small and rare,
     * and a ring buffer here would be a second thing to get wrong while
     * debugging the first. */
    rc = flash_area_erase(fa, 0, fa->fa_size < 8192U ? fa->fa_size : 8192U);
    if (rc == 0) {
        rc = flash_area_write(fa, 0, rec, sizeof(*rec));
    }
    if (rc != 0) {
        printk("nros: fault log: flash write failed (%d)\n", rc);
    }
    flash_area_close(fa);
}
#endif

static void nros_fault_report(const struct nros_fault_record *rec, const char *src)
{
    printk("\n"
           "nros: ==== FAULT RECORD (%s) ====\n"
           "nros:   reason : %u (%s)\n"
           "nros:   thread : %s\n"
           "nros:   pc     : 0x%08x\n"
           "nros:   lr     : 0x%08x\n"
           "nros:   psr    : 0x%08x\n"
           "nros:   uptime : %u ms\n"
           "nros: ==== END ====\n\n",
           src, rec->reason, nros_fault_reason_str(rec->reason),
           rec->thread, rec->pc, rec->lr, rec->psr, rec->uptime_ms);
}

static int nros_fault_log_init(void)
{
    if (nros_fault_scratch.magic != NROS_FAULT_MAGIC ||
        nros_fault_scratch.version != NROS_FAULT_VERSION) {
        /* Either a cold boot, or a record from an incompatible layout. Say
         * nothing: a boot message on every clean start would train the reader
         * to ignore this block, which is the opposite of the point. */
        return 0;
    }

    nros_fault_report(&nros_fault_scratch, "previous boot");

#if defined(CONFIG_NROS_FAULT_LOG_FLASH)
    nros_fault_persist(&nros_fault_scratch);
#endif

    /* Consume it, so the next boot does not re-report a fault that has already
     * been read. The flash copy is the durable one. */
    nros_fault_scratch.magic = 0U;
    return 0;
}

/* APPLICATION level: the flash driver and the console are both up by then, and
 * this must not run before either. */
SYS_INIT(nros_fault_log_init, APPLICATION, 99);

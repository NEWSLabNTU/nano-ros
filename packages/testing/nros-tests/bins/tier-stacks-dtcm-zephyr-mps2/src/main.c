/*
 * Issue 2084 -- tier stacks in DTCM, link-layout witness.
 *
 * Spawns one level on the nano-ros shim's tier pool
 * (`nros_zephyr_tier_task_create`), which is what keeps `nros_tier_stacks` in
 * the link: --gc-sections drops the pool from an image that spawns no tier.
 * A second level runs on a DECLARED stack, spelled the way a generated C/C++
 * entry spells it (`NROS_TIER_TASK_MEMORY_DEFINE`,
 * <rclc/detail/tier_task_memory_zephyr.h>), which must follow the same rule.
 * Each level reports whether its stack lies in the devicetree's DTCM region,
 * so a QEMU boot of the image shows the placement at run time as well; the
 * test reads it from the ELF.
 */

#include <stdint.h>
#include <stddef.h>

#include <zephyr/devicetree.h>
#include <zephyr/kernel.h>
#include <zephyr/sys/printk.h>

#include <rclc/detail/tier_task_memory_zephyr.h>

extern int nros_zephyr_tier_task_create_static(void* (*entry)(void*), void* arg, int32_t priority,
                                               const char* name, void* stack, size_t stack_size,
                                               void* thread, uint32_t core_plus1, int* pin_rc);
extern int nros_zephyr_tier_task_create(void* (*entry)(void*), void* arg, int32_t priority,
                                        const char* name, size_t stack_bytes,
                                        uint32_t core_plus1, int* pin_rc);

#define DTCM_NODE DT_NODELABEL(dtcm)
#define DTCM_LO ((uintptr_t)DT_REG_ADDR(DTCM_NODE))
#define DTCM_HI (DTCM_LO + (uintptr_t)DT_REG_SIZE(DTCM_NODE))

/* The declared level's stack: `__nros_tier_stack_1`, 4 KiB. */
NROS_TIER_TASK_MEMORY_DEFINE(1, 4096u);

static void* level(void* arg) {
    uintptr_t sp = (uintptr_t)&arg;
    printk("tier-stacks-dtcm: level `%s` stack %s DTCM (sp=0x%08lx)\n", (const char*)arg,
           (sp >= DTCM_LO && sp < DTCM_HI) ? "in" : "NOT in", (unsigned long)sp);
    for (;;) {
        k_sleep(K_FOREVER);
    }
    return NULL;
}

int main(void) {
    int pin_rc = 0;
    int rc = nros_zephyr_tier_task_create(level, (void*)"pool", 5, "pool", 0u, 0u, &pin_rc);
    printk("tier-stacks-dtcm: pool spawn rc=%d\n", rc);
    const nros_tier_task_memory_t mem = NROS_TIER_TASK_MEMORY(1);
    rc = nros_zephyr_tier_task_create_static(level, (void*)"declared", 6, "declared", mem.stack,
                                             mem.stack_bytes, mem.tcb, 0u, &pin_rc);
    printk("tier-stacks-dtcm: declared spawn rc=%d\n", rc);
    return 0;
}

/*
 * Issue 2206 -- the GCC `__atomic_*_4` libcalls an ARMv6-M (Cortex-M0/M0+)
 * image needs, built on Zephyr's own `atomic_*` API.
 *
 * ARMv6-M has no LDREX/STREX, so GCC lowers a C11 read-modify-write
 * (`atomic_fetch_add`, `atomic_compare_exchange_*`, `atomic_exchange`) to an
 * out-of-line `__atomic_<op>_4` call. Neither libgcc nor the Zephyr SDK ships
 * `libatomic` for arm-zephyr-eabi, and Zephyr does not define the libcalls
 * either: on this architecture it selects `CONFIG_ATOMIC_OPERATIONS_C` and
 * implements its OWN `atomic_t` API in C (`kernel/atomic_c.c`, a spinlock over
 * `irq_lock`). zenoh-pico's `src/collections/atomic.c` and
 * `zpico-sys/c/zpico/zpico.c` use C11 atomics, so a Pico image failed to link
 * with `undefined reference to '__atomic_fetch_add_4'`.
 *
 * This file is the one place those calls are answered, and every body
 * delegates to Zephyr's API rather than taking a lock of its own, so the C
 * side of an image has a single atomic implementation -- the kernel's, which
 * stays correct if a Cortex-M0+ SoC ever runs Zephyr SMP. Compiled only for
 * ARMv6-M (see zephyr/CMakeLists.txt); every other Cortex-M has the
 * instructions and GCC never emits these calls there.
 *
 * Only the 4-byte width: `atomic_t` is 32 bits on ARM, and the 4-byte ops are
 * what the C in an image uses (`size_t`, `uint32_t`). A narrower or wider
 * atomic read-modify-write still fails at LINK time, naming its libcall,
 * rather than silently taking a second implementation.
 *
 * The signatures are GCC's builtin ones (`unsigned int` for I4) so the
 * definitions do not trip -Wbuiltin-declaration-mismatch. The memory-order
 * arguments are ignored: Zephyr's C atomics are sequentially consistent.
 */

#include <stdbool.h>

#include <zephyr/sys/atomic.h>

#define NROS_ATOMIC_PTR(p) ((atomic_t *)(void *)(p))

unsigned int __atomic_exchange_4(volatile void *ptr, unsigned int val, int memorder)
{
    (void)memorder;
    return (unsigned int)atomic_set(NROS_ATOMIC_PTR(ptr), (atomic_val_t)val);
}

bool __atomic_compare_exchange_4(volatile void *ptr, void *expected, unsigned int desired,
                                 bool weak, int success_memorder, int failure_memorder)
{
    (void)weak;
    (void)success_memorder;
    (void)failure_memorder;
    unsigned int *exp = expected;
    for (;;) {
        if (atomic_cas(NROS_ATOMIC_PTR(ptr), (atomic_val_t)*exp, (atomic_val_t)desired)) {
            return true;
        }
        /* Report what the CAS saw. A strong CAS may only fail if the value
         * differed, so retry when it reads back equal (a concurrent writer
         * restored it between the two steps). */
        unsigned int cur = (unsigned int)atomic_get(NROS_ATOMIC_PTR(ptr));
        if (cur != *exp) {
            *exp = cur;
            return false;
        }
    }
}

#define NROS_FETCH_OP(name, zop)                                                                   \
    unsigned int __atomic_fetch_##name##_4(volatile void *ptr, unsigned int val, int memorder)    \
    {                                                                                              \
        (void)memorder;                                                                            \
        return (unsigned int)zop(NROS_ATOMIC_PTR(ptr), (atomic_val_t)val);                         \
    }

NROS_FETCH_OP(add, atomic_add)
NROS_FETCH_OP(sub, atomic_sub)
NROS_FETCH_OP(and, atomic_and)
NROS_FETCH_OP(or, atomic_or)
NROS_FETCH_OP(xor, atomic_xor)
NROS_FETCH_OP(nand, atomic_nand)

#define NROS_OP_FETCH(name, expr)                                                                  \
    unsigned int __atomic_##name##_fetch_4(volatile void *ptr, unsigned int val, int memorder)    \
    {                                                                                              \
        unsigned int old = __atomic_fetch_##name##_4(ptr, val, memorder);                          \
        return expr;                                                                               \
    }

NROS_OP_FETCH(add, old + val)
NROS_OP_FETCH(sub, old - val)
NROS_OP_FETCH(and, old & val)
NROS_OP_FETCH(or, old | val)
NROS_OP_FETCH(xor, old ^ val)
NROS_OP_FETCH(nand, ~(old & val))

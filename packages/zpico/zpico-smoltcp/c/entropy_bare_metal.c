/**
 * Bare-metal entropy source for mbedTLS.
 *
 * Provides mbedtls_hardware_poll() using the DWT cycle counter as a
 * low-quality entropy source. This is a weak symbol — platform crates
 * with hardware RNG can override it with a stronger implementation.
 *
 * WARNING: DWT cycle counter is NOT cryptographically secure entropy.
 * For production use, override with a hardware RNG (TRNG/RNG peripheral).
 */

#include "mbedtls_config.h"

#include <stddef.h>
#include <stdint.h>
#include <string.h>

#include "mbedtls/entropy.h"

/* DWT (Data Watchpoint and Trace) register addresses for ARM Cortex-M */
#define DWT_CYCCNT  (*(volatile uint32_t *)0xE0001004)
#define DWT_CONTROL (*(volatile uint32_t *)0xE0001000)
#define SCB_DEMCR   (*(volatile uint32_t *)0xE000EDFC)

/**
 * Simple hash-mix function to spread entropy from cycle counter.
 * Based on splitmix32.
 */
static uint32_t mix32(uint32_t x) {
    x ^= x >> 16;
    x *= 0x45d9f3b;
    x ^= x >> 16;
    x *= 0x45d9f3b;
    x ^= x >> 16;
    return x;
}

/**
 * Hardware entropy poll function for mbedTLS.
 *
 * Weak symbol — platform crates with hardware RNG should provide a
 * stronger implementation that overrides this default.
 *
 * This implementation samples the DWT cycle counter multiple times with
 * hash mixing. The entropy quality depends on timing jitter in the
 * sampling loop, which is architecture-dependent.
 */
__attribute__((weak))
int mbedtls_hardware_poll(void *data, unsigned char *output,
                          size_t len, size_t *olen) {
    (void)data;

    /* Ensure DWT cycle counter is enabled */
    SCB_DEMCR |= (1 << 24);   /* TRCENA */
    DWT_CONTROL |= 1;          /* CYCCNTENA */

    size_t pos = 0;
    while (pos < len) {
        /* Sample cycle counter multiple times and mix */
        uint32_t val = DWT_CYCCNT;
        val = mix32(val);

        /* Add a second sample for more jitter */
        val ^= mix32(DWT_CYCCNT);

        size_t remaining = len - pos;
        size_t chunk = (remaining < sizeof(val)) ? remaining : sizeof(val);
        memcpy(output + pos, &val, chunk);
        pos += chunk;
    }

    if (olen != NULL) {
        *olen = len;
    }

    return 0;
}

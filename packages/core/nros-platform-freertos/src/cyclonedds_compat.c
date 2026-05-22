/*
 * FreeRTOS compatibility symbols required by Cyclone DDS's upstream ddsrt
 * FreeRTOS/lwIP port when linked into the nano-ros MPS2 bare-metal image.
 */

#include <FreeRTOS.h>
#include <task.h>

#include <stddef.h>
#include <stdint.h>
#include <string.h>
#include <time.h>

extern char __tls_base[];

void *__aeabi_read_tp(void) {
    return __tls_base;
}

int gethostname(char *name, size_t len) {
    static const char hostname[] = "nano-ros";

    if (name == NULL || len == 0) {
        return -1;
    }

    size_t copy_len = sizeof(hostname);
    if (copy_len > len) {
        copy_len = len;
    }
    memcpy(name, hostname, copy_len);
    name[len - 1] = '\0';
    return 0;
}

int clock_gettime(clockid_t clock_id, struct timespec *ts) {
    (void) clock_id;

    if (ts == NULL) {
        return -1;
    }

    const uint64_t nsec_per_tick = 1000000000ull / (uint64_t) configTICK_RATE_HZ;
    const uint64_t now_ns = (uint64_t) xTaskGetTickCount() * nsec_per_tick;
    ts->tv_sec = (time_t) (now_ns / 1000000000ull);
    ts->tv_nsec = (long) (now_ns % 1000000000ull);
    return 0;
}

/**
 * @file trace_dump.c
 * @brief Dump Tonbandgeraet snapshot buffer to file via ARM semihosting.
 *
 * Called from the Rust board crate after the application completes.
 * Writes two files:
 *   - trace_meta.bin: metadata buffer (task names, event types)
 *   - trace_data.bin: snapshot event buffer
 *
 * These can be combined and converted to Perfetto format via tband-cli.
 */

#ifdef NROS_TRACE

#include "tband.h"
#include <stdint.h>
#include <stddef.h>

/* Semihosting file I/O via ARM BKPT #0xAB */

#define SH_OPEN   0x01
#define SH_CLOSE  0x02
#define SH_WRITE  0x05

static inline int sh_call(int op, void *args) {
    int result;
    __asm__ volatile (
        "mov r0, %[op]  \n"
        "mov r1, %[args]\n"
        "bkpt #0xAB     \n"
        "mov %[res], r0 \n"
        : [res] "=r" (result)
        : [op] "r" (op), [args] "r" (args)
        : "r0", "r1", "memory"
    );
    return result;
}

static int sh_open(const char *name, int mode) {
    /* SYS_OPEN args: [name_ptr, mode, name_len] */
    size_t len = 0;
    while (name[len]) len++;
    uint32_t args[3] = { (uint32_t)name, (uint32_t)mode, (uint32_t)len };
    return sh_call(SH_OPEN, args);
}

static int sh_write(int fd, const void *buf, size_t len) {
    /* SYS_WRITE args: [fd, buf_ptr, len] — returns bytes NOT written */
    uint32_t args[3] = { (uint32_t)fd, (uint32_t)buf, (uint32_t)len };
    return sh_call(SH_WRITE, args);
}

static void sh_close(int fd) {
    uint32_t args[1] = { (uint32_t)fd };
    sh_call(SH_CLOSE, args);
}

static void write_file(const char *path, const uint8_t *buf, size_t len) {
    if (buf == NULL || len == 0) return;
    /* mode 5 = w+b (create/truncate, binary) */
    int fd = sh_open(path, 5);
    if (fd < 0) return;
    sh_write(fd, buf, len);
    sh_close(fd);
}

volatile bool nros_trace_snapshot_full = false;
volatile uint32_t nros_trace_tick_count = 0;

/* FreeRTOS tick hook — called from ISR context every tick (1 kHz).
 * Provides the millisecond counter for tband_portTIMESTAMP(). */
void vApplicationTickHook(void) {
    nros_trace_tick_count++;
}

void nros_trace_scheduler_started(void) {
    tband_gather_system_metadata();
    tband_freertos_scheduler_started();
}

void nros_trace_trigger_and_dump(void) {
    tband_trigger_snapshot();

    const uint8_t *meta = tband_get_metadata_buf(0);
    size_t meta_len = tband_get_metadata_buf_amnt(0);
    const uint8_t *data = tband_get_core_snapshot_buf(0);
    size_t data_len = tband_get_core_snapshot_buf_amnt(0);

    if ((meta == NULL && data == NULL) || (meta_len == 0 && data_len == 0)) return;

    /* Write metadata + data concatenated into a single file
     * (tband-cli expects both in one stream) */
    int fd = sh_open("trace.bin", 5);  /* mode 5 = w+b */
    if (fd < 0) return;
    if (meta && meta_len > 0) sh_write(fd, meta, meta_len);
    if (data && data_len > 0) sh_write(fd, data, data_len);
    sh_close(fd);
}

#else /* !NROS_TRACE */

/* Stubs when tracing is disabled */
#include <stdbool.h>
#include <stdint.h>
volatile bool nros_trace_snapshot_full = false;
volatile uint32_t nros_trace_tick_count = 0;
void nros_trace_scheduler_started(void) {}
void nros_trace_trigger_and_dump(void) {}

#endif /* NROS_TRACE */

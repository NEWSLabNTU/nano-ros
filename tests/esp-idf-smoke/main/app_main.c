/*
 * Smoke test: pulls `nros-platform-esp-idf-c` into a minimal ESP-IDF
 * project + calls one symbol from each capability category. Prints
 * results over the default UART so qemu-system-{riscv32,xtensa}
 * captures them.
 *
 * Built by `just esp_idf build-c-port`; run by `just esp_idf
 * test-c-port`.
 */

#include <nros/platform.h>
#include <nros/platform_timer.h>

#include <esp_log.h>
#include <freertos/FreeRTOS.h>
#include <freertos/task.h>

#include <inttypes.h>
#include <stdio.h>
#include <string.h>

static const char *TAG = "nros_smoke";

static volatile int s_timer_fires = 0;
static void on_timer(void *user_data) {
    (void) user_data;
    s_timer_fires++;
}

void app_main(void) {
    ESP_LOGI(TAG, "nros-platform-esp-idf-c smoke test");

    /* Clock */
    uint64_t t0 = nros_platform_clock_ms();
    vTaskDelay(pdMS_TO_TICKS(50));
    uint64_t t1 = nros_platform_clock_ms();
    ESP_LOGI(TAG, "clock_ms: %" PRIu64 " -> %" PRIu64 " (delta=%" PRIu64 ")",
             t0, t1, t1 - t0);
    if (t1 < t0 + 20) {
        ESP_LOGE(TAG, "FAIL clock_ms not advancing");
        return;
    }

    /* Alloc */
    void *p = nros_platform_alloc(64);
    if (p == NULL) {
        ESP_LOGE(TAG, "FAIL alloc");
        return;
    }
    memset(p, 0xCC, 64);
    nros_platform_dealloc(p);

    /* Yield + sleep */
    nros_platform_yield_now();
    nros_platform_sleep_ms(10);

    /* Random */
    uint32_t r = nros_platform_random_u32();
    ESP_LOGI(TAG, "random_u32: 0x%08" PRIx32, r);

    /* Mutex round-trip */
    StaticSemaphore_t mtx_storage;
    SemaphoreHandle_t mtx = xSemaphoreCreateMutexStatic(&mtx_storage);
    if (mtx == NULL) {
        ESP_LOGE(TAG, "FAIL mutex static create");
        return;
    }
    /* The canonical mutex_* operates on our own opaque storage; the
     * call above is just to confirm FreeRTOS sync primitives work. */

    /* Timer */
    void *th = nros_platform_timer_create_periodic(
        20 * 1000 /* 20 ms */, on_timer, NULL);
    if (th == NULL) {
        ESP_LOGE(TAG, "FAIL timer create");
        return;
    }
    vTaskDelay(pdMS_TO_TICKS(150));
    nros_platform_timer_destroy(th);
    ESP_LOGI(TAG, "timer fires over 150ms: %d", s_timer_fires);
    if (s_timer_fires < 4) {
        ESP_LOGE(TAG, "FAIL timer fired too few times");
        return;
    }

    ESP_LOGI(TAG, "nros esp-idf-c smoke PASS");
}

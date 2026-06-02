/* Phase 212.M.7 — ESP-IDF listener app_main.
 *
 * IDF entry point. Boots IDF, then hands control to Rust via
 * `rust_app_main()` (exported by the sibling `esp32-bsp-listener`
 * Rust staticlib). Wi-Fi bring-up via `esp_wifi_*` is a follow-up;
 * once landed it goes here before the Rust call so the nros executor
 * sees a live network stack. */

#include <stdio.h>

extern int rust_app_main(void);

void app_main(void)
{
    printf("nano-ros esp-idf listener: app_main\n");
    /* TODO(212.M.7 follow-up): esp_netif_init / esp_event_loop_create_default /
     * esp_wifi_init / esp_wifi_start before handing off to Rust. */
    int rc = rust_app_main();
    printf("nano-ros esp-idf listener: rust_app_main returned %d\n", rc);
}

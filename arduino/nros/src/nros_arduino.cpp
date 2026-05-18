// nros_arduino.cpp — Arduino IDE transport glue. Only ESP32 today
// (arduino-esp32 core); the precompiled `.a` slots under
// `src/<arch>/` decide which chip variant gets linked.

#include "nros_arduino.h"

#include <cstring>

#include <Arduino.h>
#include <WiFi.h>

// `nros/init.h` deliberately NOT included — see nros_arduino.h
// for the size-macro / build-dir reason.

extern "C" {

// Stashed at WiFi-setup time, consumed by `nros_init` via the
// nros-c init-config hook. The precompiled `libnanoros.a`'s `nros_init`
// reads this through the symbol below; if the symbol is missing
// (linked against an older `libnanoros.a`), the locator falls back
// to compile-time defaults baked into the platform layer.
static const char* g_nros_arduino_locator = nullptr;

// Weak shim so older `libnanoros.a` snapshots still link. The real
// symbol lives in the cffi platform shim once Phase 21.6 lands.
__attribute__((weak)) void nros_platform_set_zenoh_locator(const char* /*locator*/) {}

void set_nanoros_wifi_transports(const char* ssid, const char* pass,
                                  const char* zenoh_locator) {
    Serial.printf("[nros] Connecting to WiFi: %s\n", ssid);
    WiFi.mode(WIFI_STA);
    WiFi.begin(ssid, pass);

    // Block until associated. Arduino sketches almost always set
    // up WiFi in setup() before publishing — keep it simple.
    while (WiFi.status() != WL_CONNECTED) {
        delay(250);
        Serial.print('.');
    }
    Serial.printf("\n[nros] WiFi up, IP %s\n", WiFi.localIP().toString().c_str());

    g_nros_arduino_locator = zenoh_locator;
    nros_platform_set_zenoh_locator(zenoh_locator);
    Serial.printf("[nros] zenoh locator: %s\n", zenoh_locator);
}

void set_nanoros_serial_transports(void) {
    // Reserved for future use — nros's primary value over micro-ROS
    // is the no-agent WiFi path; Serial transport lands when a Serial
    // zenoh-pico link is supported on arduino-esp32.
    Serial.println("[nros] set_nanoros_serial_transports() is not yet implemented");
}

bool nanoros_ping(uint32_t timeout_ms) {
    if (g_nros_arduino_locator == nullptr) {
        return false;
    }
    // Phase 23.3.2 will hook this through zenoh-pico scout. The
    // skeleton returns `WiFi.status() == WL_CONNECTED` as a coarse
    // proxy so the API shape is reviewable before the full
    // implementation lands.
    (void)timeout_ms;
    return WiFi.status() == WL_CONNECTED;
}

}  // extern "C"

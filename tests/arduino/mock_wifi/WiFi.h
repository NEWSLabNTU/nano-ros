// Phase 23.5d — mock WiFi.h for host transport-glue tests.
//
// Stubs out the Arduino-esp32 WiFi.h surface that
// `arduino/nros/src/nros_arduino.cpp` calls
// (`WiFi.mode(...)`, `WiFi.begin(ssid, pass)`,
// `WiFi.status()`, `WiFi.localIP()`). Returns "connected" on the
// first poll so `set_nanoros_wifi_transports()` does not block.
//
// Linked only by the host test target; never shipped in the
// precompiled Arduino library.

#ifndef NANO_ROS_TESTS_MOCK_WIFI_H
#define NANO_ROS_TESTS_MOCK_WIFI_H

#include <stdint.h>

#include <string>

enum WiFiMode_t { WIFI_OFF = 0, WIFI_STA = 1, WIFI_AP = 2 };

enum wl_status_t {
    WL_NO_SHIELD = 255,
    WL_IDLE_STATUS = 0,
    WL_CONNECTED = 3,
    WL_DISCONNECTED = 6,
};

struct IPAddress {
    uint32_t addr = 0x0100007f;  // 127.0.0.1
    std::string toString() const { return std::string("127.0.0.1"); }
};

class WiFiClass {
public:
    void mode(WiFiMode_t /*m*/) {}
    void begin(const char* /*ssid*/, const char* /*pass*/) { connected_ = true; }
    wl_status_t status() { return connected_ ? WL_CONNECTED : WL_IDLE_STATUS; }
    IPAddress localIP() { return IPAddress{}; }

private:
    bool connected_ = true;  // pretend we are already on the AP
};

inline WiFiClass WiFi;

#endif  // NANO_ROS_TESTS_MOCK_WIFI_H

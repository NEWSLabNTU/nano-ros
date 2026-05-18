// nros_arduino.h — Arduino IDE glue for the nros C API.
//
// Wraps WiFi + zenoh-locator setup so the Arduino-specific code is
// confined to ~70 lines. The rest of the C API
// (`nros_init`, `nros_node_create`, etc.) lives in the precompiled
// `libnanoros.a` shipped under `src/<arch>/`.
//
// Mirrors the micro-ROS Arduino API shape on purpose: users
// migrating from `set_microros_wifi_transports()` should find the
// nros equivalent immediately recognisable. The key difference is
// the absence of a host-side agent — nros sketches connect directly
// to `zenohd`.

#ifndef NANO_ROS_ARDUINO_H
#define NANO_ROS_ARDUINO_H

#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

#include <nros/init.h>

// ────────────────────────────────────────────────────────────────────
// Error-handling macros (mirror micro-ROS's RCCHECK / RCSOFTCHECK)
// ────────────────────────────────────────────────────────────────────

#define NRCHECK(fn)                                                              \
    {                                                                            \
        int _nros_rc = (fn);                                                     \
        if (_nros_rc != 0) {                                                     \
            Serial.printf("[nros] Error %d at %s:%d\n", _nros_rc, __FILE__,      \
                          __LINE__);                                             \
            while (1) {                                                          \
                delay(1000);                                                     \
            }                                                                    \
        }                                                                        \
    }

#define NRSOFTCHECK(fn)                                                          \
    {                                                                            \
        int _nros_rc = (fn);                                                     \
        if (_nros_rc != 0) {                                                     \
            Serial.printf("[nros] Warning %d at %s:%d\n", _nros_rc, __FILE__,    \
                          __LINE__);                                             \
        }                                                                        \
    }

// ────────────────────────────────────────────────────────────────────
// Transport setup
// ────────────────────────────────────────────────────────────────────

/// Connect to a WiFi network and configure the zenoh locator for the
/// next `nros_init()` call. Blocks until `WiFi.status() ==
/// WL_CONNECTED`. The locator string is in zenoh's standard form
/// (`tcp/<host>:<port>`, `udp/<host>:<port>`, …).
void set_nanoros_wifi_transports(const char* ssid, const char* pass,
                                  const char* zenoh_locator);

/// Future: configure Serial as a custom zenoh-pico transport.
void set_nanoros_serial_transports(void);

/// Lightweight reachability check against the configured zenoh
/// router. Returns `true` if the router answers within
/// `timeout_ms`, `false` otherwise. Useful for `Reconnection.ino`.
bool nanoros_ping(uint32_t timeout_ms);

#ifdef __cplusplus
}
#endif

#endif  // NANO_ROS_ARDUINO_H

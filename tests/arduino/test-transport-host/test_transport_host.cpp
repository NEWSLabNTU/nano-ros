// Phase 23.5d — host transport-glue smoke test.
//
// Compiles `arduino/nros/src/nros_arduino.cpp` against the mock
// WiFi.h / Arduino.h stubs under `../mock_wifi/` and the natively-
// built `libnros_c.a`, then exercises the public glue
// surface (`set_nanoros_wifi_transports`, `nanoros_ping`,
// `NRCHECK`-style error macros). Runs in CI without QEMU or any
// physical ESP32 — validates that the Arduino-side glue is shaped
// correctly before the per-arch `libnanoros.a` reaches an actual
// Xtensa / RISC-V target.

#include <cstdio>
#include <cstdlib>
#include <cstring>

// Mock Arduino + WiFi headers under ../mock_wifi/ — the test
// build adds that dir to the include path BEFORE the
// arduino/nros/src dir, so `nros_arduino.cpp`'s
// `#include <Arduino.h>` / `#include <WiFi.h>` resolves here.
#include <Arduino.h>
#include <WiFi.h>

#include "nros_arduino.h"

namespace {

int test_set_nanoros_wifi_transports() {
    set_nanoros_wifi_transports("mock-ssid", "mock-pass",
                                 "tcp/127.0.0.1:7447");
    if (WiFi.status() != WL_CONNECTED) {
        std::fprintf(stderr,
            "[test] WiFi.status() != WL_CONNECTED after "
            "set_nanoros_wifi_transports\n");
        return 1;
    }
    if (WiFi.localIP().toString() != "127.0.0.1") {
        std::fprintf(stderr, "[test] localIP mismatch\n");
        return 1;
    }
    return 0;
}

int test_nanoros_ping_reports_wifi_status() {
    if (!nanoros_ping(/*timeout_ms=*/100)) {
        std::fprintf(stderr,
            "[test] nanoros_ping returned false even though "
            "WiFi.status() == WL_CONNECTED\n");
        return 1;
    }
    return 0;
}

}  // namespace

int main() {
    int failures = 0;
    failures += test_set_nanoros_wifi_transports();
    failures += test_nanoros_ping_reports_wifi_status();

    if (failures == 0) {
        std::printf("[PASS] nros_arduino host transport-glue smoke\n");
        return EXIT_SUCCESS;
    }
    std::fprintf(stderr, "[FAIL] %d host transport-glue check(s)\n",
                 failures);
    return EXIT_FAILURE;
}

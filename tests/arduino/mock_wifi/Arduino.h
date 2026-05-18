// Phase 23.5d — mock Arduino.h for host transport-glue tests.
//
// Provides the subset of Arduino-core symbols
// `arduino/nros/src/nros_arduino.cpp` references: `Serial` (with
// `begin` + `print*`), `delay`. Backed by libc printf / usleep.

#ifndef NANO_ROS_TESTS_MOCK_ARDUINO_H
#define NANO_ROS_TESTS_MOCK_ARDUINO_H

#include <stdarg.h>
#include <stdint.h>
#include <stdio.h>
#include <unistd.h>

class HardwareSerial {
public:
    void begin(uint32_t /*baud*/) {}
    void print(const char* s) { fputs(s, stdout); }
    void println(const char* s) {
        fputs(s, stdout);
        fputc('\n', stdout);
    }
    void println() { fputc('\n', stdout); }
    int printf(const char* fmt, ...) {
        va_list ap;
        va_start(ap, fmt);
        int n = vprintf(fmt, ap);
        va_end(ap);
        return n;
    }
    void print(char c) { fputc(c, stdout); }
};

inline HardwareSerial Serial;

inline void delay(uint32_t ms) { usleep(static_cast<useconds_t>(ms) * 1000u); }

#endif  // NANO_ROS_TESTS_MOCK_ARDUINO_H

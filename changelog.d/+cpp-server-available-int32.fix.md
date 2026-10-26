C++ `service_is_ready()` on a client compiles for 32-bit ARM: the three call sites passed an `int*` where the FFI takes `int32_t*`, which is `long` there.

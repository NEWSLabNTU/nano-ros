// phase-241.A cross gate probe (issues #27/#36 — the two-libc `.c`/`.cpp`-TU
// class). A C++ TU that pulls libstdc++'s <cstdlib> (which `#include_next`s
// <stdlib.h>) plus <stdlib.h> + <type_traits> and uses div_t — the same shape
// the NuttX cpp entry compiled when the clash bit. With the RTOS sysroot
// reachable but NOT winning <cstdlib>, the cross newlib's div_t and the RTOS
// div_t collide; with the RTOS `include/cxx` prepended, only the RTOS div_t
// exists → clean.
#include <type_traits>
#include <cstdlib>
#include <stdlib.h>

static_assert(std::is_same<div_t, div_t>::value, "div_t resolves");
div_t nros_gate_probe_div{};
int main() { return nros_gate_probe_div.quot; }

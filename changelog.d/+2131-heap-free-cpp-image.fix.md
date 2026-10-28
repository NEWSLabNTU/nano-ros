The C and C++ surfaces are now checked heap-free at link time, not only the
Rust runtime. `just ci l3` builds a bare-metal Cortex-M3 C++ image over
nros-cpp and nros-c without `alloc`, with the whole C/C++ ABI anchored and a
program that creates a node, uses its parameters and spins. It then asserts
that the linked ELF has no allocation symbol (issue 2131).

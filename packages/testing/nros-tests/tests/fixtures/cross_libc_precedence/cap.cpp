// issue 1656 — toolchain capability probe for the #27/#36 two-libc gate
// (`cross_libc_cxx_stdlib_probe` row). A bare-metal cross g++ that ships only
// the newlib C library cannot compile this; that is an unmet precondition for
// the gate, NOT the two-libc clash, so the test skips on a failure here.
#include <type_traits>
#include <cstdlib>
int main() { return 0; }

/* Minimal RTOS-sysroot stub for the phase-241.A cross gate (issues #27/#36).
 *
 * An RTOS (NuttX) ships its own <stdlib.h> declaring div_t/ldiv_t as a NAMED
 * struct (`typedef struct div_s div_t;`). The cross toolchain's newlib ships a
 * <stdlib.h> declaring them as an ANONYMOUS-struct typedef (`typedef struct {…}
 * div_t;`). C++ treats those as conflicting declarations. When BOTH headers
 * reach a C++ TU — the RTOS one via a plain `-I <rtos>/include` and newlib's via
 * libstdc++ `<cstdlib>`'s `#include_next <stdlib.h>` — the compile dies on
 * `conflicting declaration '…div_t'` (the exact #36 failure).
 *
 * This stub carries ONLY the conflicting decls, so the gate needs the cross
 * toolchain but NOT the full RTOS submodule. Mirrors
 * third-party/nuttx/nuttx/include/stdlib.h's div_t shape.
 */
#ifndef NROS_GATE_RTOS_STUB_STDLIB_H
#define NROS_GATE_RTOS_STUB_STDLIB_H
struct div_s  { int quot; int rem; };
struct ldiv_s { long quot; long rem; };
typedef struct div_s  div_t;
typedef struct ldiv_s ldiv_t;
#endif /* NROS_GATE_RTOS_STUB_STDLIB_H */

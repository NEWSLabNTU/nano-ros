// phase-329 W5 — build-stage form of the platform_header_matrix `posix/c/platform`
// cell. It parses the canonical C header <nros/platform.h> and uses the malloc
// surface a POSIX build provides. The original ran `cc -std=c11
// -Werror=implicit-function-declaration`; under the shared cxx-syntax builder
// (`c++ -std=c++14`) the same intent holds — an undeclared nros_platform_malloc
// is a HARD error in C++ too, so a header that fails to declare the malloc surface
// still reds. See tests/platform_header_compile.rs.
#define NROS_PLATFORM_POSIX
#define _POSIX_C_SOURCE 200809L
#define _DEFAULT_SOURCE
#include <nros/platform.h>
namespace {
void* use_it() {
    void* p = nros_platform_malloc(8);
    nros_platform_free(p);
    return p;
}
} // namespace

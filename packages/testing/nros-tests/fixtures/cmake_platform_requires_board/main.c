/* Link-correctness only — the configure is expected to fail before this
 * is ever compiled (see CMakeLists.txt). */
#include <nros/init.h>

int main(void) {
    nros_support_t support = nros_support_get_zero_initialized();
    (void)support;
    return 0;
}

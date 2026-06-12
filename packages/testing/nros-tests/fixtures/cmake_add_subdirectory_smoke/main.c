/* Smoke test — link-correctness only (issue 0041 build-stage fixture). */
#include <nros/init.h>

int main(void) {
    nros_support_t support = nros_support_get_zero_initialized();
    (void)support;
    return 0;
}

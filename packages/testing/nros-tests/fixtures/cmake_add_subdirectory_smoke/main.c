/* Smoke test — link-correctness only (issue 0041 build-stage fixture). */
#include <nros/init.h>

int main(void) {
    rclc_support_t support = rclc_support_get_zero_initialized();
    (void)support;
    return 0;
}

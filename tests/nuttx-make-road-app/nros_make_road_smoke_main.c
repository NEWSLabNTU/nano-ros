/* phase-550 W1 — the make-road lane's smoke app.
 *
 * Built, never run: `scripts/nuttx/make-road-build.sh` checks that the kernel
 * ELF holds this entry, `rcl_publish` and `nros_*` text. Publishing through a
 * zero-initialised publisher reaches the RMW dispatch, so the link pulls the
 * archive's runtime in rather than one leaf function. */
#include <stdio.h>

#include <rclc/rclc.h>

int main(int argc, char* argv[]) {
    (void)argc;
    (void)argv;
    rcl_publisher_t pub = rcl_get_zero_initialized_publisher();
    rcl_ret_t rc = rcl_publish(&pub, NULL, NULL);
    printf("nros make-road smoke: rcl_publish on an unset publisher -> %d\n", (int)rc);
    return rc == RCL_RET_OK ? 1 : 0;
}

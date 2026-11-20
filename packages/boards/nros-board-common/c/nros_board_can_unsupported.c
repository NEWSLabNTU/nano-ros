/*
 * RFC-0132 D3 -- the `board` backend's fail-loud defaults.
 *
 * On a `"board"`-stack board (FreeRTOS, ThreadX or bare metal over a vendor
 * driver), the CAN frame driver is the board package's: it defines the five
 * `<nros/platform_can.h>` calls as STRONG symbols over the driver nano-ros
 * cannot ship. These WEAK definitions are what an image links when nothing
 * defines them: `open` reports `NROS_PLATFORM_CAN_ENOTSUP` and prints one line
 * naming the board, so an image whose board has no CAN driver says why at its
 * first `open` instead of failing at link or silently.
 *
 * The board compiles this file as an OBJECT of its startup sources, never
 * into an archive a consumer's driver competes with by link order: a strong
 * definition in any object of the image wins over these, and a weak
 * definition here never hides a strong one that an archive could have given.
 * The board's name arrives as `NROS_BOARD_CAN_BOARD_NAME` (a string literal);
 * without it the line says "this board".
 */

#include <stdio.h>

#include <nros/platform_can.h>

#ifndef NROS_BOARD_CAN_BOARD_NAME
#define NROS_BOARD_CAN_BOARD_NAME "this board"
#endif

__attribute__((weak)) int8_t nros_platform_can_open(nros_platform_can_t* can, const char* device,
                                                    uint8_t want_fd) {
    (void)can;
    (void)want_fd;
    printf("nros: CAN frame driver: board `%s` defines no nros_platform_can_* "
           "(its board package supplies them over the vendor driver); "
           "open(%s) is not supported\n",
           NROS_BOARD_CAN_BOARD_NAME, device != NULL ? device : "NULL");
    return NROS_PLATFORM_CAN_ENOTSUP;
}

__attribute__((weak)) int8_t nros_platform_can_send(nros_platform_can_t* can,
                                                    const nros_platform_can_frame_t* frame,
                                                    uint32_t timeout_us) {
    (void)can;
    (void)frame;
    (void)timeout_us;
    return NROS_PLATFORM_CAN_ENOTSUP;
}

__attribute__((weak)) uint8_t nros_platform_can_fd_active(const nros_platform_can_t* can) {
    (void)can;
    return 0u;
}

__attribute__((weak)) int8_t nros_platform_can_recv(nros_platform_can_t* can,
                                                    nros_platform_can_frame_t* frame,
                                                    uint32_t timeout_us) {
    (void)can;
    (void)frame;
    (void)timeout_us;
    return NROS_PLATFORM_CAN_ENOTSUP;
}

__attribute__((weak)) void nros_platform_can_close(nros_platform_can_t* can) {
    (void)can;
}

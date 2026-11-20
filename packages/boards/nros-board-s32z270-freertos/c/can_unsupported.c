/*
 * RFC-0132 D3 -- this board's CAN frame driver defaults. The S32Z270's CAN
 * controllers are driven by the vendor's real-time drivers, which nano-ros
 * cannot ship: the board package (or a consumer's board glue) supplies the
 * five `nros_platform_can_*` calls as strong symbols over them. Without
 * them, these weak defaults make `open` report "not supported", naming
 * this board.
 */
#define NROS_BOARD_CAN_BOARD_NAME "s32z270-freertos"
#include "../../nros-board-common/c/nros_board_can_unsupported.c"

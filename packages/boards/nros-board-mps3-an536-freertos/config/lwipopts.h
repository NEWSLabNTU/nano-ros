/* lwIP options — QEMU MPS3-AN536. phase-385 W1: the shared family defaults
 * (IGMP on, BSD sockets, DNS) carry unchanged; the emulated LAN9118 needs
 * no board-specific pbuf tuning — the MPS2 sibling drives the same part. */
#include "../../nros-board-freertos/config/lwipopts.h"
